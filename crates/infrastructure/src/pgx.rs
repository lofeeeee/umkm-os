//! Helper kecil bersama untuk repo ber-tenant (P3+).

use sqlx::{PgPool, Postgres, Transaction};
use umkm_shared::ids::TenantId;

pub type Tx = Transaction<'static, Postgres>;

pub fn db_err(e: umkm_database::DbError) -> sqlx::Error {
    match e {
        umkm_database::DbError::Sqlx(e) => e,
        umkm_database::DbError::InvalidUrl => sqlx::Error::Configuration("invalid url".into()),
    }
}

/// Transaksi dengan tenant context RLS (`app.tenant_id`).
pub async fn tenant_tx(pool: &PgPool, t: TenantId) -> Result<Tx, sqlx::Error> {
    umkm_database::begin_tenant_tx(pool, t)
        .await
        .map_err(db_err)
}

/// SQLSTATE error database, mis. `23505`.
pub fn sqlstate(e: &sqlx::Error) -> Option<String> {
    e.as_database_error()
        .and_then(|d| d.code())
        .map(|c| c.into_owned())
}

/// Nama constraint yang dilanggar (bila ada).
pub fn constraint(e: &sqlx::Error) -> Option<&str> {
    e.as_database_error().and_then(|d| d.constraint())
}

/// `(sqlstate, constraint)` untuk pencocokan error.
pub fn violation(e: &sqlx::Error) -> Option<(String, String)> {
    Some((sqlstate(e)?, constraint(e).unwrap_or_default().to_owned()))
}
