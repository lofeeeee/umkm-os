//! Circuit breaker per provider (P5-8, `PRD.md §4.15`, §6.6): provider yang terus gagal sementara
//! tidak dipanggil selama jeda, agar API & worker tetap responsif (permintaan langsung dialihkan).
//! In-memory per proses (cukup untuk VPS tunggal; setiap instance belajar sendiri).

use std::{
    collections::HashMap,
    sync::Mutex,
    time::{Duration, Instant},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum State {
    Closed {
        failures: u32,
    },
    Open {
        until: Instant,
    },
    /// Satu panggilan percobaan sedang berjalan.
    HalfOpen,
}

#[derive(Debug)]
pub struct CircuitBreaker {
    threshold: u32,
    cooldown: Duration,
    states: Mutex<HashMap<String, State>>,
}

impl CircuitBreaker {
    pub fn new(threshold: u32, cooldown: Duration) -> Self {
        Self {
            threshold: threshold.max(1),
            cooldown,
            states: Mutex::default(),
        }
    }

    /// `true` = boleh memanggil provider sekarang.
    pub fn allow(&self, key: &str, now: Instant) -> bool {
        let mut m = self.states.lock().expect("breaker lock");
        let s = m
            .entry(key.to_owned())
            .or_insert(State::Closed { failures: 0 });
        match *s {
            State::Closed { .. } => true,
            State::Open { until } if now >= until => {
                *s = State::HalfOpen;
                true
            }
            State::Open { .. } | State::HalfOpen => false,
        }
    }

    pub fn success(&self, key: &str) {
        self.states
            .lock()
            .expect("breaker lock")
            .insert(key.to_owned(), State::Closed { failures: 0 });
    }

    /// Hanya kegagalan sementara (timeout, 5xx, 429, jaringan) yang dihitung.
    pub fn failure(&self, key: &str, now: Instant) {
        let mut m = self.states.lock().expect("breaker lock");
        let s = m
            .entry(key.to_owned())
            .or_insert(State::Closed { failures: 0 });
        *s = match *s {
            State::Closed { failures } if failures + 1 < self.threshold => State::Closed {
                failures: failures + 1,
            },
            _ => State::Open {
                until: now + self.cooldown,
            },
        };
    }

    pub fn is_open(&self, key: &str, now: Instant) -> bool {
        matches!(self.states.lock().expect("breaker lock").get(key), Some(State::Open { until }) if now < *until)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn opens_after_threshold_and_recovers_via_half_open() {
        let b = CircuitBreaker::new(3, Duration::from_secs(30));
        let t0 = Instant::now();
        for _ in 0..2 {
            assert!(b.allow("p", t0));
            b.failure("p", t0);
        }
        assert!(b.allow("p", t0));
        b.failure("p", t0);
        assert!(
            !b.allow("p", t0) && b.is_open("p", t0),
            "terbuka setelah 3 gagal"
        );
        assert!(b.allow("other", t0), "per provider");
        let later = t0 + Duration::from_secs(31);
        assert!(b.allow("p", later), "half-open: satu percobaan");
        assert!(!b.allow("p", later), "percobaan kedua ditahan");
        b.failure("p", later);
        assert!(
            !b.allow("p", later + Duration::from_secs(1)),
            "gagal saat half-open → terbuka lagi"
        );
        let much_later = later + Duration::from_secs(31);
        assert!(b.allow("p", much_later));
        b.success("p");
        assert!(b.allow("p", much_later) && b.allow("p", much_later));
        // Sukses mereset hitungan.
        b.failure("p", much_later);
        b.success("p");
        b.failure("p", much_later);
        b.failure("p", much_later);
        assert!(b.allow("p", much_later));
    }
}
