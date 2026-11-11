//! Rate limiter fixed-window di Redis (AUTH-007, `PRD.md §6.1`).
//!
//! Key: `{prefix}:platform:rl_{rule}:{sha256(subject)}` — subjek (email/IP) di-hash.
//! INCR + PEXPIRE dijalankan atomik via Lua sehingga tidak ada key tanpa TTL.
//! Bila Redis tidak tersedia, `check` mengembalikan `Err`; pemanggil endpoint
//! autentikasi WAJIB menolak request (fail-closed), bukan melewatkan pembatasan.

use redis::{Script, aio::ConnectionManager};
use umkm_domain::auth::RateRule;

use crate::{
    auth::subject_digest,
    redis::{KeyError, KeyNamespace, RedisError},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Decision {
    pub allowed: bool,
    pub count: u32,
    pub limit: u32,
    /// Sisa waktu window (detik, dibulatkan ke atas).
    pub retry_after_secs: u64,
}

#[derive(Debug, thiserror::Error)]
pub enum RateLimitError {
    #[error(transparent)]
    Key(#[from] KeyError),
    #[error(transparent)]
    Redis(#[from] RedisError),
}

const SCRIPT: &str = r"
local c = redis.call('INCR', KEYS[1])
if c == 1 then redis.call('PEXPIRE', KEYS[1], ARGV[1]) end
local ttl = redis.call('PTTL', KEYS[1])
if ttl < 0 then redis.call('PEXPIRE', KEYS[1], ARGV[1]); ttl = tonumber(ARGV[1]) end
return {c, ttl}
";

#[derive(Clone)]
pub struct RateLimiter {
    conn: ConnectionManager,
    ns: KeyNamespace,
    script: Script,
}

impl RateLimiter {
    pub fn new(conn: ConnectionManager, ns: KeyNamespace) -> Self {
        Self {
            conn,
            ns,
            script: Script::new(SCRIPT),
        }
    }

    pub fn key(&self, rule: &RateRule, subject: &str) -> Result<String, KeyError> {
        Ok(self
            .ns
            .platform()
            .key(&format!("rl_{}", rule.name), &subject_digest(subject))?
            .as_str()
            .to_owned())
    }

    /// Catat satu percobaan dan putuskan apakah diizinkan.
    pub async fn check(&self, rule: &RateRule, subject: &str) -> Result<Decision, RateLimitError> {
        let key = self.key(rule, subject)?;
        let window_ms = u64::try_from(rule.window.as_millis())
            .unwrap_or(u64::MAX)
            .max(1);
        let mut conn = self.conn.clone();
        let (count, ttl_ms): (u64, i64) = self
            .script
            .key(&key)
            .arg(window_ms)
            .invoke_async(&mut conn)
            .await
            .map_err(RedisError::from)?;
        let count = u32::try_from(count).unwrap_or(u32::MAX);
        Ok(Decision {
            allowed: count <= rule.limit,
            count,
            limit: rule.limit,
            retry_after_secs: u64::try_from(ttl_ms.max(0)).unwrap_or(0).div_ceil(1000),
        })
    }

    /// Hapus hitungan (mis. setelah login berhasil untuk aturan per-email).
    pub async fn reset(&self, rule: &RateRule, subject: &str) -> Result<(), RateLimitError> {
        let key = self.key(rule, subject)?;
        let mut conn = self.conn.clone();
        let _: i64 = redis::cmd("DEL")
            .arg(&key)
            .query_async(&mut conn)
            .await
            .map_err(RedisError::from)?;
        Ok(())
    }
}
