//! Penjaga sesi GoPay Merchant (keep-alive): token diperbarui SEBELUM kedaluwarsa dan minimal tiap
//! 6 jam, tanpa menunggu ada pesanan. Dulu refresh hanya terjadi saat ada tagihan menunggu bayar,
//! sehingga toko yang sepi beberapa jam kehilangan sesinya dan pemilik harus login ulang.
//!
//! Aturan:
//! - Satu jalur refresh untuk worker, poller, dan tombol "Cek sekarang" ([`ensure_fresh`]).
//! - Baris sesi dikunci (`FOR UPDATE SKIP LOCKED`) selama refresh: refresh token GoBiz dirotasi,
//!   jadi dua refresh bersamaan akan membuat salah satunya tidak sah.
//! - Status `needs_relogin` hanya bila GoBiz menolak refresh DAN access token tersimpan juga ditolak.
//!   Refresh ditolak tetapi access token masih berlaku → tetap `active` (`last_error = refresh_…`).
//! - Penolakan definitif GoBiz = 401/400/403. Gangguan
//!   jaringan/5xx/429 dicatat (`last_error`, `refresh_failures`) lalu dicoba lagi siklus berikutnya.

use sqlx::PgPool;
use umkm_shared::ids::TenantId;

use super::client::GobizClient;
use super::poller::now_ms;
use super::session::{GopaySession, KEEPALIVE_MS};
use crate::{
    crypto::Keyring,
    gopay_repo::{self, GopayRepoError, LockResult},
};

/// Kapan sesi diperbarui.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RefreshMode {
    /// Hanya bila token hampir kedaluwarsa atau sudah ≥ 6 jam tidak diperbarui.
    WhenDue,
    /// Pemilik menekan "Cek sekarang": uji access token yang tersimpan dulu; perbarui hanya bila
    /// token ditolak atau memang jatuh tempo. Sesi berstatus perlu login ulang juga diuji ulang.
    Force,
    /// GoBiz baru saja menolak access token (401 saat polling): wajib perbarui.
    AccessRejected,
}

/// Hasil uji access token tersimpan.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Access {
    Valid,
    Rejected,
    Unknown,
}

async fn probe(client: &GobizClient, session: &GopaySession, now: i64) -> (Access, Option<String>) {
    match client.probe(session, now).await {
        Ok(()) => (Access::Valid, None),
        Err(e) if e.is_auth_rejected() => (Access::Rejected, Some(e.code())),
        Err(e) => (Access::Unknown, Some(e.code())),
    }
}

#[derive(Debug)]
pub enum RefreshOutcome {
    /// Token diperbarui dan disimpan.
    Refreshed(GopaySession),
    /// Token masih segar; tidak perlu diperbarui.
    Fresh(GopaySession),
    /// GoBiz menolak refresh token — pemilik perlu menghubungkan ulang.
    NeedsRelogin {
        code: String,
    },
    /// Gagal sementara; sesi lama dipertahankan dan dicoba lagi nanti.
    Transient {
        session: GopaySession,
        code: String,
    },
    /// Proses lain sedang memperbarui sesi toko ini.
    Busy,
    NotConnected,
}

/// Apakah sesi perlu diperbarui sekarang (token hampir habis, atau refresh penjaga 6 jam).
pub fn is_due(session: &GopaySession, refreshed_at_ms: Option<i64>, now: i64) -> bool {
    session.needs_refresh(now) || refreshed_at_ms.is_none_or(|r| now - r >= KEEPALIVE_MS)
}

/// Pastikan sesi toko segar; perbarui bila perlu (lihat [`RefreshMode`]).
pub async fn ensure_fresh(
    pool: &PgPool,
    k: &Keyring,
    client: &GobizClient,
    t: TenantId,
    mode: RefreshMode,
) -> Result<RefreshOutcome, GopayRepoError> {
    let lock = match gopay_repo::lock_for_refresh(pool, t, k).await? {
        LockResult::Locked(l) => l,
        LockResult::Busy => return Ok(RefreshOutcome::Busy),
        LockResult::NotConnected => return Ok(RefreshOutcome::NotConnected),
    };
    let now = now_ms();
    let stored = &lock.stored;
    // Sesi yang sudah ditandai perlu login ulang hanya dicoba lagi atas permintaan pemilik.
    if stored.status != "active" && mode != RefreshMode::Force {
        let code = stored
            .last_error
            .clone()
            .unwrap_or_else(|| "needs_relogin".into());
        lock.release().await?;
        return Ok(RefreshOutcome::NeedsRelogin { code });
    }
    if mode == RefreshMode::WhenDue && !is_due(&stored.session, stored.refreshed_at_ms, now) {
        let session = stored.session.clone();
        lock.release().await?;
        return Ok(RefreshOutcome::Fresh(session));
    }

    let old = stored.session.clone();
    let due = is_due(&old, stored.refreshed_at_ms, now);

    // "Cek sekarang": token yang masih diterima GoBiz tidak perlu (dan sebaiknya tidak) diperbarui —
    // refresh token GoBiz dirotasi, jadi setiap refresh yang tak perlu adalah risiko.
    let mut access = None;
    if mode == RefreshMode::Force {
        match probe(client, &old, now).await {
            (Access::Valid, _) if !due => {
                lock.keep_active(None).await?;
                return Ok(RefreshOutcome::Fresh(old));
            }
            (Access::Unknown, code) => {
                let code = code.unwrap_or_else(|| "network".into());
                if stored.status != "active" {
                    lock.release().await?;
                    return Ok(RefreshOutcome::NeedsRelogin { code });
                }
                lock.fail(&code, false).await?;
                return Ok(RefreshOutcome::Transient { session: old, code });
            }
            (a, _) => access = Some(a),
        }
    }
    if mode == RefreshMode::AccessRejected {
        access = Some(Access::Rejected);
    }

    match client.refresh(&old, now).await {
        Ok(fresh) => {
            // Token lama sudah dirotasi GoBiz: kegagalan menyimpan di sini dilaporkan (bukan diabaikan).
            lock.save(&fresh, k).await?;
            tracing::info!(tenant = %t.as_uuid(), expires_in_min = (fresh.expires_at_ms - now) / 60_000, "gopay: sesi diperbarui");
            Ok(RefreshOutcome::Refreshed(fresh))
        }
        Err(e) if e.is_auth_rejected() => {
            let code = e.code();
            // GoBiz menolak PERPANJANGAN. Itu belum berarti sesi mati: access token (±24 jam) bisa masih
            // diterima. Hanya bila access token juga ditolak, pemilik perlu menghubungkan ulang.
            let access = match access {
                Some(a) => a,
                None => probe(client, &old, now).await.0,
            };
            match access {
                Access::Valid => {
                    let soft = format!("refresh_{code}");
                    lock.keep_active(Some(&soft)).await?;
                    tracing::warn!(tenant = %t.as_uuid(), code, "gopay: refresh ditolak, access token masih berlaku");
                    Ok(RefreshOutcome::Transient {
                        session: old,
                        code: soft,
                    })
                }
                Access::Rejected => {
                    lock.fail(&code, true).await?;
                    tracing::warn!(tenant = %t.as_uuid(), code, "gopay: sesi ditolak GoBiz; perlu login ulang");
                    Ok(RefreshOutcome::NeedsRelogin { code })
                }
                Access::Unknown => {
                    lock.fail(&code, false).await?;
                    Ok(RefreshOutcome::Transient { session: old, code })
                }
            }
        }
        Err(e) => {
            let code = e.code();
            lock.fail(&code, false).await?;
            tracing::warn!(tenant = %t.as_uuid(), code, "gopay: refresh gagal sementara; dicoba lagi");
            Ok(RefreshOutcome::Transient { session: old, code })
        }
    }
}

#[derive(Debug, Default, Clone, Copy)]
pub struct KeepaliveStats {
    pub due: usize,
    pub refreshed: usize,
    pub relogin: usize,
    pub transient: usize,
}

/// Perbarui semua sesi aktif yang jatuh tempo (dipanggil worker tiap siklus).
pub async fn keepalive_due(
    pool: &PgPool,
    k: &Keyring,
    client: &GobizClient,
    limit: i32,
) -> KeepaliveStats {
    let mut st = KeepaliveStats::default();
    let tenants = match gopay_repo::refresh_due_tenants(pool, limit).await {
        Ok(t) => t,
        Err(e) => {
            tracing::warn!(error = %e, "gopay: gagal ambil sesi jatuh tempo");
            return st;
        }
    };
    for t in tenants {
        st.due += 1;
        match ensure_fresh(pool, k, client, t, RefreshMode::WhenDue).await {
            Ok(RefreshOutcome::Refreshed(_)) => st.refreshed += 1,
            Ok(RefreshOutcome::NeedsRelogin { .. }) => st.relogin += 1,
            Ok(RefreshOutcome::Transient { .. }) => st.transient += 1,
            Ok(_) => {}
            Err(e) => {
                st.transient += 1;
                tracing::warn!(tenant = %t.as_uuid(), error = %e, "gopay: keep-alive gagal");
            }
        }
    }
    st
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gopay::session::REFRESH_SKEW_MS;

    fn s(expires_at_ms: i64) -> GopaySession {
        GopaySession {
            phone_e164: "+628123456789".into(),
            merchant_id: None,
            outlet_name: None,
            access_token: "a".into(),
            refresh_token: "r".into(),
            expires_at_ms,
        }
    }

    #[test]
    fn due_when_expiring_or_stale() {
        let now = 100 * 60 * 60 * 1000;
        let far = now + 20 * 60 * 60 * 1000;
        // Segar & baru diperbarui → tidak perlu.
        assert!(!is_due(&s(far), Some(now - 60_000), now));
        // Hampir kedaluwarsa → perlu.
        assert!(is_due(
            &s(now + REFRESH_SKEW_MS - 1),
            Some(now - 60_000),
            now
        ));
        // Sudah 6 jam tidak diperbarui walau token masih lama → perlu (refresh penjaga).
        assert!(is_due(&s(far), Some(now - KEEPALIVE_MS), now));
        // Belum pernah diperbarui sejak terhubung (sesi lama sebelum migrasi) → perlu.
        assert!(is_due(&s(far), None, now));
    }
}
