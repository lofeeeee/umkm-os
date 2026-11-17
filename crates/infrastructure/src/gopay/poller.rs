//! Poller pembayaran GoPay (P10, ADR-0020): untuk tiap toko dengan charge pending, ambil transaksi
//! GoBiz, cocokkan nominal unik, lalu auto-konfirmasi pesanan (→ Dibayar). Dipanggil berkala oleh
//! worker (~15 dtk). GoBiz tak punya webhook, jadi polling adalah satu-satunya jalur.

use std::time::{SystemTime, UNIX_EPOCH};

use sqlx::PgPool;
use umkm_domain::{catalog::Money, commerce::gopay::match_payment};
use umkm_shared::ids::TenantId;

use super::client::{GobizClient, GobizError};
use super::keepalive::{RefreshMode, RefreshOutcome, ensure_fresh};
use crate::{crypto::Keyring, gopay_repo, payment_repo};

/// Berapa transaksi terbaru yang diambil per toko tiap siklus.
const FETCH_SIZE: u32 = 50;
/// Toleransi mundur jendela waktu (jam GoBiz vs server bisa sedikit beda).
const WINDOW_SKEW_MS: i64 = 60_000;

#[derive(Debug, Default, Clone, Copy)]
pub struct PollStats {
    pub tenants: usize,
    pub confirmed: usize,
    pub errors: usize,
}

pub fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

/// Poll semua toko yang punya charge pending. Error per-toko tidak menghentikan toko lain.
pub async fn poll_due(pool: &PgPool, k: &Keyring, client: &GobizClient, limit: i32) -> PollStats {
    let mut stats = PollStats::default();
    let tenants = match gopay_repo::due_tenants(pool, limit).await {
        Ok(t) => t,
        Err(e) => {
            tracing::warn!(error = %e, "gopay: gagal ambil daftar tenant due");
            return stats;
        }
    };
    for t in tenants {
        stats.tenants += 1;
        match poll_tenant(pool, t, k, client).await {
            Ok(n) => stats.confirmed += n,
            Err(e) => {
                stats.errors += 1;
                tracing::warn!(tenant = %t.as_uuid(), error = %e, "gopay: poll toko gagal");
            }
        }
    }
    stats
}

/// Poll satu toko. Kembalikan jumlah pesanan yang terkonfirmasi otomatis pada siklus ini.
async fn poll_tenant(
    pool: &PgPool,
    t: TenantId,
    k: &Keyring,
    client: &GobizClient,
) -> Result<usize, GobizError> {
    let Some(stored) = gopay_repo::get_session(pool, t, k)
        .await
        .map_err(|_| GobizError::Parse("sesi"))?
    else {
        return Ok(0);
    };
    if stored.status != "active" {
        return Ok(0);
    }

    let charges = gopay_repo::pending_charges(pool, t)
        .await
        .map_err(|_| GobizError::Parse("charges"))?;
    if charges.is_empty() {
        return Ok(0);
    }

    let now = now_ms();
    // Token disegarkan lewat jalur keep-alive bersama (terkunci, tahan gangguan sementara).
    let session = match ensure_fresh(pool, k, client, t, RefreshMode::WhenDue)
        .await
        .map_err(|_| GobizError::Parse("sesi"))?
    {
        RefreshOutcome::Refreshed(s) | RefreshOutcome::Fresh(s) => s,
        // Gagal sementara: coba poll dengan token lama (bisa jadi masih berlaku).
        RefreshOutcome::Transient { session, .. } => session,
        // Worker lain sedang memperbarui; poll siklus berikutnya.
        RefreshOutcome::Busy => return Ok(0),
        RefreshOutcome::NeedsRelogin { .. } | RefreshOutcome::NotConnected => return Ok(0),
    };

    let start = charges.iter().map(|c| c.issued_at_ms).min().unwrap_or(now) - WINDOW_SKEW_MS;

    let txns = match client
        .fetch_transactions(&session, start, now, FETCH_SIZE)
        .await
    {
        Ok(tx) => tx,
        Err(GobizError::Unauthorized) => {
            // Token ditolak lebih awal dari perkiraan: paksa refresh sekali lalu ulang. Hanya penolakan
            // definitif saat refresh yang menandai sesi perlu login ulang (di dalam ensure_fresh).
            match ensure_fresh(pool, k, client, t, RefreshMode::Force)
                .await
                .map_err(|_| GobizError::Parse("sesi"))?
            {
                RefreshOutcome::Refreshed(fresh) => {
                    client
                        .fetch_transactions(&fresh, start, now, FETCH_SIZE)
                        .await?
                }
                _ => return Ok(0),
            }
        }
        Err(e) => return Err(e),
    };

    let mut confirmed = 0;
    for c in &charges {
        let total = c.total_rupiah as u64;
        // Dedup lintas-siklus ditegakkan DB (indeks unik txn); di sini cukup |_| false.
        let Some(txn) = match_payment(total, c.issued_at_ms, &txns, |_| false) else {
            continue;
        };
        // Klaim transaksi untuk charge ini (anti double-claim di DB).
        match gopay_repo::mark_matched(pool, t, c.order_id, &txn.id).await {
            Ok(true) => {}
            _ => continue,
        }
        let Some(received) = Money::from_sen(c.total_rupiah.saturating_mul(100)) else {
            continue;
        };
        match payment_repo::confirm_auto(pool, t, c.order_id, received, &txn.id, (None, None)).await
        {
            Ok(_) => confirmed += 1,
            Err(e) => {
                // Mis. order sudah dikonfirmasi manual: charge tetap 'matched', tidak dihitung.
                tracing::debug!(order = %c.order_id, error = %e, "gopay: auto-confirm dilewati");
            }
        }
    }
    Ok(confirmed)
}
