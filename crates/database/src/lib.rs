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

/// Nama crate, dipakai untuk smoke test wiring workspace.
pub const CRATE: &str = "umkm-database";

/// Migration ter-embed dari `migrations/` (versioned, reversible).
pub static MIGRATOR: Migrator = sqlx::migrate!("../../migrations");

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
