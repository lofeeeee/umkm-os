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
//! - Sesi `needs_relogin` tidak pernah di-refresh lagi (refresh token-nya sudah ditolak); "Cek sekarang"
//!   hanya menguji access token. Lihat [`force_step`].
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

/// Langkah "Cek sekarang" setelah access token tersimpan diuji.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ForceStep {
    /// Token diterima dan belum jatuh tempo: biarkan.
    KeepFresh,
    /// Sesi berstatus perlu login ulang, tetapi access token ternyata diterima: aktifkan lagi
    /// tanpa refresh.
    Reactivate,
    /// Sesi berstatus perlu login ulang dan GoBiz tetap menolak: pemilik harus menghubungkan ulang.
    Relogin,
    /// GoBiz tidak dapat dihubungi (sesi aktif): catat, coba lagi nanti.
    Transient,
    /// Perbarui token.
    Refresh,
}

/// Sesi `needs_relogin` hanya terjadi setelah GoBiz menolak refresh token-nya. Memakai ulang refresh
/// token yang sudah ditolak tidak akan berhasil dan dapat dianggap GoBiz sebagai pemakaian ulang token
/// (*refresh token reuse*), yang mencabut seluruh sesi akun — termasuk login pemilik di browser.
/// Jadi pada sesi itu "Cek sekarang" hanya menguji access token, tidak pernah me-refresh.
fn force_step(active: bool, access: Access, due: bool) -> ForceStep {
    match (active, access) {
        (true, Access::Valid) if !due => ForceStep::KeepFresh,
        (true, Access::Valid | Access::Rejected) => ForceStep::Refresh,
        (true, Access::Unknown) => ForceStep::Transient,
        (false, Access::Valid) => ForceStep::Reactivate,
        (false, Access::Rejected | Access::Unknown) => ForceStep::Relogin,
    }
}

/// Kode `last_error` untuk sesi yang refresh token-nya pernah ditolak (selalu berawalan `refresh_`).
fn soft_refresh_code(last_error: Option<&str>) -> String {
    match last_error {
        Some(c) if c.starts_with("refresh_") => c.to_string(),
        Some(c) => format!("refresh_{c}"),
        None => "refresh_rejected".into(),
    }
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
        let (a, code) = probe(client, &old, now).await;
        match force_step(stored.status == "active", a, due) {
            ForceStep::KeepFresh => {
                lock.keep_active(None).await?;
                return Ok(RefreshOutcome::Fresh(old));
            }
            ForceStep::Reactivate => {
                // Access token ternyata masih diterima, tetapi refresh token tersimpan sudah pernah
                // ditolak: catat sebagai penolakan perpanjangan agar worker tidak langsung mengulangnya.
                let soft = soft_refresh_code(stored.last_error.as_deref());
                lock.keep_active(Some(&soft)).await?;
                return Ok(RefreshOutcome::Transient {
                    session: old,
                    code: soft,
                });
            }
            ForceStep::Relogin => {
                let code = code
                    .or_else(|| stored.last_error.clone())
                    .unwrap_or_else(|| "needs_relogin".into());
                lock.release().await?;
                return Ok(RefreshOutcome::NeedsRelogin { code });
            }
            ForceStep::Transient => {
                let code = code.unwrap_or_else(|| "network".into());
                lock.fail(&code, false).await?;
                return Ok(RefreshOutcome::Transient { session: old, code });
            }
            ForceStep::Refresh => access = Some(a),
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
    fn force_never_refreshes_a_relogin_session() {
        use Access::*;
        // Sesi aktif: perilaku lama.
        assert_eq!(force_step(true, Valid, false), ForceStep::KeepFresh);
        assert_eq!(force_step(true, Valid, true), ForceStep::Refresh);
        assert_eq!(force_step(true, Rejected, false), ForceStep::Refresh);
        assert_eq!(force_step(true, Unknown, true), ForceStep::Transient);
        // Sesi perlu login ulang: refresh token sudah ditolak → jangan pernah dipakai ulang.
        for due in [false, true] {
            assert_eq!(force_step(false, Valid, due), ForceStep::Reactivate);
            assert_eq!(force_step(false, Rejected, due), ForceStep::Relogin);
            assert_eq!(force_step(false, Unknown, due), ForceStep::Relogin);
        }
    }

    #[test]
    fn soft_code_is_always_refresh_prefixed() {
        assert_eq!(soft_refresh_code(Some("status_400")), "refresh_status_400");
        assert_eq!(
            soft_refresh_code(Some("refresh_status_400")),
            "refresh_status_400"
        );
        assert_eq!(soft_refresh_code(None), "refresh_rejected");
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
