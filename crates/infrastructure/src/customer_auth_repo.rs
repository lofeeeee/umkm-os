//! Akun customer toko (P3-3, ADR-0005). Semua query di tenant context dari Host (RLS):
//! akun/sesi/token toko A tidak pernah terlihat saat request datang ke host toko B.

use std::time::Duration;

use sqlx::{PgPool, Row};
use umkm_application::notify::OutboxEvent;
use umkm_domain::{audit::AuditEvent, auth::TokenHash};
use umkm_shared::ids::TenantId;
use uuid::Uuid;

use crate::{
    crm_repo::CUSTOMER_LINKED,
    pgx::{Tx, tenant_tx, violation},
};

#[derive(Debug, thiserror::Error)]
pub enum CustomerAuthError {
    #[error("email sudah terdaftar di toko ini")]
    EmailTaken,
    #[error(transparent)]
    Db(#[from] sqlx::Error),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoginRecord {
    pub account_id: Uuid,
    pub password_hash: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CustomerSession {
    pub tenant_id: TenantId,
    pub account_id: Uuid,
    pub session_id: Uuid,
    pub email: String,
    pub verified: bool,
}

fn secs(d: Duration) -> f64 {
    d.as_secs_f64()
}

/// Daftar akun + event verifikasi di transaksi yang sama (outbox, BR-017).
pub async fn register(
    pool: &PgPool,
    t: TenantId,
    email: &str,
    password_hash: &str,
    (privacy, terms): (&str, &str),
) -> Result<Uuid, CustomerAuthError> {
    let mut tx = tenant_tx(pool, t).await?;
    let id: Uuid = sqlx::query_scalar(
        "INSERT INTO customer_accounts (tenant_id, email, password_hash, privacy_version, terms_version)
         VALUES ($1, $2, $3, $4, $5) RETURNING id",
    )
    .bind(t.as_uuid())
    .bind(email)
    .bind(password_hash)
    .bind(privacy)
    .bind(terms)
    .fetch_one(&mut *tx)
    .await
    .map_err(|e| match violation(&e) {
        Some((s, c)) if s == "23505" && c == "uq_customer_accounts_email" => {
            CustomerAuthError::EmailTaken
        }
        _ => CustomerAuthError::Db(e),
    })?;
    crate::outbox::enqueue(
        &mut *tx,
        &OutboxEvent::customer_email_verification_requested(*t.as_uuid(), id),
    )
    .await?;
    tx.commit().await?;
    Ok(id)
}

pub async fn find_login(
    pool: &PgPool,
    t: TenantId,
    email: &str,
) -> Result<Option<LoginRecord>, sqlx::Error> {
    let mut tx = tenant_tx(pool, t).await?;
    let r = sqlx::query(
        "SELECT id, password_hash FROM customer_accounts WHERE tenant_id = $1 AND email = $2",
    )
    .bind(t.as_uuid())
    .bind(email)
    .fetch_optional(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(r.map(|r| LoginRecord {
        account_id: r.get("id"),
        password_hash: r.get("password_hash"),
    }))
}

pub async fn create_session(
    pool: &PgPool,
    t: TenantId,
    account: Uuid,
    token: TokenHash,
    ttl: Duration,
) -> Result<Uuid, sqlx::Error> {
    let mut tx = tenant_tx(pool, t).await?;
    let id = sqlx::query_scalar(
        "INSERT INTO customer_sessions (tenant_id, account_id, token_hash, expires_at)
         VALUES ($1, $2, $3, now() + make_interval(secs => $4)) RETURNING id",
    )
    .bind(t.as_uuid())
    .bind(account)
    .bind(&token.0[..])
    .bind(secs(ttl))
    .fetch_one(&mut *tx)
    .await?;
    // Akun terverifikasi: tautkan customer guest baru dengan email sama (ADR-0005).
    link_in(&mut tx, t, account).await?;
    tx.commit().await?;
    Ok(id)
}

/// Sesi valid di tenant ini (bukan dicabut/kedaluwarsa).
pub async fn authenticate(
    pool: &PgPool,
    t: TenantId,
    token: TokenHash,
) -> Result<Option<CustomerSession>, sqlx::Error> {
    let mut tx = tenant_tx(pool, t).await?;
    let r = sqlx::query(
        "SELECT s.id, s.account_id, a.email, a.email_verified_at IS NOT NULL AS verified
         FROM customer_sessions s
         JOIN customer_accounts a ON a.tenant_id = s.tenant_id AND a.id = s.account_id
         WHERE s.tenant_id = $1 AND s.token_hash = $2 AND s.revoked_at IS NULL AND s.expires_at > now()",
    )
    .bind(t.as_uuid())
    .bind(&token.0[..])
    .fetch_optional(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(r.map(|r| CustomerSession {
        tenant_id: t,
        account_id: r.get("account_id"),
        session_id: r.get("id"),
        email: r.get("email"),
        verified: r.get("verified"),
    }))
}

pub async fn revoke(pool: &PgPool, t: TenantId, session: Uuid) -> Result<(), sqlx::Error> {
    let mut tx = tenant_tx(pool, t).await?;
    sqlx::query(
        "UPDATE customer_sessions SET revoked_at = now()
         WHERE tenant_id = $1 AND id = $2 AND revoked_at IS NULL",
    )
    .bind(t.as_uuid())
    .bind(session)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(())
}

/// AC-CRM-005: hanya akun terverifikasi; hanya customer tenant ini, belum bertaut, tidak
/// dalam proses hapus, dengan email sama. Mengembalikan jumlah yang ditautkan.
async fn link_in(tx: &mut Tx, t: TenantId, account: Uuid) -> Result<u64, sqlx::Error> {
    let n = sqlx::query(
        "UPDATE customers c SET account_id = a.id, updated_at = now()
         FROM customer_accounts a
         WHERE a.tenant_id = $1 AND a.id = $2 AND a.email_verified_at IS NOT NULL
           AND c.tenant_id = a.tenant_id AND c.email = a.email AND c.account_id IS NULL
           AND c.deletion_requested_at IS NULL AND c.anonymized_at IS NULL",
    )
    .bind(t.as_uuid())
    .bind(account)
    .execute(&mut **tx)
    .await?
    .rows_affected();
    if n > 0 {
        crate::audit_repo::record(
            &mut **tx,
            &AuditEvent::new(CUSTOMER_LINKED, "customer_account")
                .in_tenant(t)
                .resource(account)
                .with_summary(serde_json::json!({ "linked": n })),
        )
        .await?;
    }
    Ok(n)
}

/// Konsumsi token verifikasi di tenant ini lalu tautkan customer guest.
/// `Some((account, linked))` bila token valid.
pub async fn verify_email(
    pool: &PgPool,
    t: TenantId,
    token: TokenHash,
) -> Result<Option<(Uuid, u64)>, sqlx::Error> {
    let mut tx = tenant_tx(pool, t).await?;
    let account: Option<Uuid> = sqlx::query_scalar(
        "UPDATE customer_tokens SET consumed_at = now()
         WHERE tenant_id = $1 AND token_hash = $2 AND purpose = 'email_verification'
           AND consumed_at IS NULL AND expires_at > now()
         RETURNING account_id",
    )
    .bind(t.as_uuid())
    .bind(&token.0[..])
    .fetch_optional(&mut *tx)
    .await?;
    let Some(account) = account else {
        tx.commit().await?;
        return Ok(None);
    };
    sqlx::query(
        "UPDATE customer_accounts SET email_verified_at = COALESCE(email_verified_at, now())
         WHERE tenant_id = $1 AND id = $2",
    )
    .bind(t.as_uuid())
    .bind(account)
    .execute(&mut *tx)
    .await?;
    let linked = link_in(&mut tx, t, account).await?;
    tx.commit().await?;
    Ok(Some((account, linked)))
}

/// Dipakai worker: buat token baru (yang lama dibatalkan). `None` bila akun sudah
/// terverifikasi / tidak ada. Mengembalikan (email, host toko).
pub async fn issue_verification_token(
    pool: &PgPool,
    t: TenantId,
    account: Uuid,
    token: TokenHash,
    ttl: Duration,
) -> Result<Option<(String, String)>, sqlx::Error> {
    let mut tx = tenant_tx(pool, t).await?;
    let email: Option<String> = sqlx::query_scalar(
        "SELECT email FROM customer_accounts
         WHERE tenant_id = $1 AND id = $2 AND email_verified_at IS NULL",
    )
    .bind(t.as_uuid())
    .bind(account)
    .fetch_optional(&mut *tx)
    .await?;
    let host: Option<String> = sqlx::query_scalar(
        "SELECT hostname FROM tenant_domains
         WHERE tenant_id = $1 AND status = 'active' ORDER BY (type = 'subdomain') DESC, created_at LIMIT 1",
    )
    .bind(t.as_uuid())
    .fetch_optional(&mut *tx)
    .await?;
    let (Some(email), Some(host)) = (email, host) else {
        tx.commit().await?;
        return Ok(None);
    };
    sqlx::query(
        "UPDATE customer_tokens SET consumed_at = now()
         WHERE tenant_id = $1 AND account_id = $2 AND consumed_at IS NULL",
    )
    .bind(t.as_uuid())
    .bind(account)
    .execute(&mut *tx)
    .await?;
    sqlx::query(
        "INSERT INTO customer_tokens (tenant_id, account_id, purpose, token_hash, expires_at)
         VALUES ($1, $2, 'email_verification', $3, now() + make_interval(secs => $4))",
    )
    .bind(t.as_uuid())
    .bind(account)
    .bind(&token.0[..])
    .bind(secs(ttl))
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(Some((email, host)))
}

/// Customer milik akun (terlihat oleh customer itu sendiri). Tanpa tag/catatan internal.
pub async fn customer_ids(
    pool: &PgPool,
    t: TenantId,
    account: Uuid,
) -> Result<Vec<Uuid>, sqlx::Error> {
    let mut tx = tenant_tx(pool, t).await?;
    let ids = sqlx::query_scalar(
        "SELECT id FROM customers WHERE tenant_id = $1 AND account_id = $2 AND anonymized_at IS NULL
         ORDER BY created_at",
    )
    .bind(t.as_uuid())
    .bind(account)
    .fetch_all(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(ids)
}
