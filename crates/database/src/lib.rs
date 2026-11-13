//! Akses PostgreSQL via SQLx: pool, migration, dan tenant context (P0-4).
//!
//! Aturan (`PRD.md §5.7`):
//! - Tenant context di-set **transaction-local** (`set_config(..., true)`), sehingga
//!   context tidak tertinggal di koneksi pool setelah commit/rollback.
//! - Query repository tetap wajib menyertakan `tenant_id` sebagai defense-in-depth.

use std::time::Duration;

use sqlx::{
    PgPool, Postgres, Transaction,
    migrate::Migrator,
    postgres::{PgConnectOptions, PgPoolOptions},
};
use umkm_shared::{
    config::{DatabaseSettings, SecretString},
    ids::TenantId,
};

pub mod app_role;

/// Nama crate, dipakai untuk smoke test wiring workspace.
pub const CRATE: &str = "umkm-database";

/// Migration ter-embed dari `migrations/` (versioned, reversible).
pub static MIGRATOR: Migrator = sqlx::migrate!("../../migrations");

/// Migrator untuk perintah `migrate` di deploy (runbook deploy & rollback, P8-5).
/// Bedanya dengan [`MIGRATOR`]: migrasi yang SUDAH diterapkan tetapi tidak dikenal binary ini (milik rilis yang
/// lebih baru) diabaikan, sehingga rollback ke image sebelumnya tidak macet di langkah migrate. Aman karena
/// kebijakan expand/contract: skema rilis N tetap kompatibel dengan binary N-1. Checksum migrasi yang dikenal
/// tetap diverifikasi (file yang sudah diterapkan tidak boleh diedit).
pub fn deploy_migrator() -> Migrator {
    let mut m = sqlx::migrate!("../../migrations");
    m.set_ignore_missing(true);
    m
}

pub const TENANT_SETTING: &str = "app.tenant_id";

#[derive(Debug, thiserror::Error)]
pub enum DbError {
    #[error("database url tidak valid")]
    InvalidUrl,
    #[error(transparent)]
    Sqlx(#[from] sqlx::Error),
}

/// Buat pool terbatas sesuai config (`PRD.md §9.4`: SQLx pool dibatasi).
/// Koneksi dibuka lazily agar proses dapat start dan melaporkan `/readyz` 503.
pub fn connect_lazy(url: &SecretString, settings: &DatabaseSettings) -> Result<PgPool, DbError> {
    // Pesan error parse URL bisa memuat URL (dan password); jangan diteruskan.
    let opts: PgConnectOptions = url.expose().parse().map_err(|_| DbError::InvalidUrl)?;
    Ok(PgPoolOptions::new()
        .max_connections(settings.max_connections)
        .acquire_timeout(Duration::from_secs(settings.acquire_timeout_secs))
        .connect_lazy_with(opts))
}

/// Mulai transaksi dengan tenant context transaction-local.
pub async fn begin_tenant_tx(
    pool: &PgPool,
    tenant: TenantId,
) -> Result<Transaction<'static, Postgres>, DbError> {
    let mut tx = pool.begin().await?;
    set_tenant(&mut tx, tenant).await?;
    Ok(tx)
}

/// Set tenant context pada transaksi yang sudah berjalan.
pub async fn set_tenant(
    tx: &mut Transaction<'_, Postgres>,
    tenant: TenantId,
) -> Result<(), DbError> {
    sqlx::query("SELECT set_config($1, $2, true)")
        .bind(TENANT_SETTING)
        .bind(tenant.to_string())
        .execute(&mut **tx)
        .await?;
    Ok(())
}

/// Readiness ringan: `SELECT 1`.
pub async fn ping(pool: &PgPool) -> Result<(), DbError> {
    sqlx::query("SELECT 1").execute(pool).await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    #[test]
    fn crate_name() {
        assert_eq!(super::CRATE, "umkm-database");
    }

    /// Penjaga MIGRATOR basi: setiap `*.up.sql` di `migrations/` harus ter-embed.
    #[test]
    fn embedded_migrations_match_directory() {
        let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/../../migrations");
        let mut on_disk: Vec<i64> = std::fs::read_dir(dir)
            .unwrap()
            .filter_map(|e| {
                let name = e.unwrap().file_name().into_string().unwrap();
                name.strip_suffix(".up.sql")
                    .map(|stem| stem.split('_').next().unwrap().parse().unwrap())
            })
            .collect();
        on_disk.sort_unstable();
        let embedded: Vec<i64> = super::MIGRATOR
            .iter()
            .filter(|m| !m.migration_type.is_down_migration())
            .map(|m| m.version)
            .collect();
        assert_eq!(
            embedded, on_disk,
            "MIGRATOR basi: rebuild crate database (build.rs harus memicu ini)"
        );
    }

    #[test]
    fn migrations_are_embedded_and_reversible() {
        let m: Vec<_> = super::MIGRATOR.iter().collect();
        assert!(!m.is_empty());
        assert!(
            super::MIGRATOR
                .iter()
                .all(|m| m.migration_type.is_reversible())
        );
    }
}
