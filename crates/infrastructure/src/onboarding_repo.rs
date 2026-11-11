//! Adapter Postgres untuk registrasi (P1-3) + adapter hashing/token untuk port aplikasi.

use std::sync::Arc;

use sqlx::{PgPool, Postgres, Transaction};
use umkm_application::onboarding::{
    NewRegistration, PasswordHashing, Registered, RegistrationStore, StoreError, TokenIssuer,
};
use umkm_domain::auth::{AuthError, PasswordHasher, TokenHash};
use uuid::Uuid;

use crate::auth::{Argon2idHasher, new_token};

#[derive(Clone)]
pub struct PgRegistrationStore {
    pool: PgPool,
}

impl PgRegistrationStore {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

fn other(e: impl std::fmt::Display) -> StoreError {
    StoreError::Other(e.to_string())
}

/// Konflik slug/hostname atau slug cadangan (trigger `reject_reserved_slug`).
fn is_slug_conflict(e: &sqlx::Error) -> bool {
    let Some(db) = e.as_database_error() else {
        return false;
    };
    match db.code().as_deref() {
        Some("23505") => matches!(
            db.constraint(),
            Some("uq_tenants_slug" | "uq_tenant_domains_hostname")
        ),
        Some("23514") => db.message().contains("is reserved"),
        _ => false,
    }
}

async fn insert_tenant_with_domain(
    tx: &mut Transaction<'_, Postgres>,
    reg: &NewRegistration,
    slug: &umkm_domain::tenant::Slug,
) -> Result<String, sqlx::Error> {
    let tenant_id = *reg.tenant_id.as_uuid();
    sqlx::query("INSERT INTO tenants (id, name, slug) VALUES ($1, $2, $3)")
        .bind(tenant_id)
        .bind(&reg.business_name)
        .bind(slug.as_str())
        .execute(&mut **tx)
        .await?;
    let hostname = slug.hostname(&reg.root_domain);
    sqlx::query(
        "INSERT INTO tenant_domains (tenant_id, hostname, type, status, verified_at)
         VALUES ($1, $2, 'subdomain', 'active', now())",
    )
    .bind(tenant_id)
    .bind(&hostname)
    .execute(&mut **tx)
    .await?;
    Ok(hostname)
}

impl RegistrationStore for PgRegistrationStore {
    async fn create(&self, reg: NewRegistration) -> Result<Registered, StoreError> {
        let mut tx = self.pool.begin().await.map_err(other)?;

        let user_id: Uuid = sqlx::query_scalar(
            "INSERT INTO users (email, password_hash) VALUES ($1, $2) RETURNING id",
        )
        .bind(reg.email.as_str())
        .bind(&reg.password_hash)
        .fetch_one(&mut *tx)
        .await
        .map_err(
            |e| match e.as_database_error().and_then(|d| d.constraint()) {
                Some("uq_users_email") => StoreError::EmailTaken,
                _ => other(e),
            },
        )?;

        // Tenant context transaction-local sebelum menyentuh tabel milik tenant (RLS P1-7).
        umkm_database::set_tenant(&mut tx, reg.tenant_id)
            .await
            .map_err(other)?;

        let mut chosen = None;
        for slug in &reg.slug_candidates {
            // Transaksi bersarang = SAVEPOINT.
            let mut sp = sqlx::Acquire::begin(&mut *tx).await.map_err(other)?;
            match insert_tenant_with_domain(&mut sp, &reg, slug).await {
                Ok(hostname) => {
                    sp.commit().await.map_err(other)?;
                    chosen = Some((slug.clone(), hostname));
                    break;
                }
                Err(e) if is_slug_conflict(&e) => {
                    sp.rollback().await.map_err(other)?;
                }
                Err(e) => return Err(other(e)),
            }
        }
        let (slug, hostname) = chosen.ok_or(StoreError::SlugUnavailable)?;
        let tenant_id = *reg.tenant_id.as_uuid();

        sqlx::query(
            "INSERT INTO tenant_memberships (tenant_id, user_id, role_id)
             SELECT $1, $2, id FROM roles WHERE tenant_id IS NULL AND is_system AND name = 'Owner'",
        )
        .bind(tenant_id)
        .bind(user_id)
        .execute(&mut *tx)
        .await
        .map_err(other)
        .and_then(|r| {
            if r.rows_affected() == 1 {
                Ok(())
            } else {
                Err(other("preset role Owner tidak ada"))
            }
        })?;

        sqlx::query("INSERT INTO business_profiles (tenant_id, name) VALUES ($1, $2)")
            .bind(tenant_id)
            .bind(&reg.business_name)
            .execute(&mut *tx)
            .await
            .map_err(other)?;

        sqlx::query(
            "INSERT INTO user_consents (user_id, document, version)
             VALUES ($1, 'privacy_policy', $2), ($1, 'terms_of_service', $3)",
        )
        .bind(user_id)
        .bind(&reg.legal.privacy_policy)
        .bind(&reg.legal.terms_of_service)
        .execute(&mut *tx)
        .await
        .map_err(other)?;

        let session_id = crate::auth_repo::create_session(
            &mut *tx,
            user_id,
            reg.session_token_hash,
            reg.session_ttl,
            reg.user_agent.as_deref(),
        )
        .await
        .map_err(other)?;

        // Audit atomik dengan registrasi (P1-8).
        crate::audit_repo::record(
            &mut *tx,
            &umkm_domain::audit::AuditEvent::new(
                umkm_domain::audit::actions::TENANT_REGISTERED,
                "tenant",
            )
            .by_user(user_id)
            .in_tenant(reg.tenant_id)
            .resource(tenant_id)
            .request(reg.request_id.as_deref(), reg.client_ip.as_deref())
            .with_summary(serde_json::json!({ "slug": slug.as_str(), "hostname": hostname })),
        )
        .await
        .map_err(other)?;

        // Transactional outbox: verifikasi email dikirim worker (P1-6).
        crate::outbox::enqueue(
            &mut *tx,
            &umkm_application::notify::OutboxEvent::email_verification_requested(user_id),
        )
        .await
        .map_err(other)?;

        tx.commit().await.map_err(other)?;
        Ok(Registered {
            user_id,
            tenant_id: reg.tenant_id,
            slug,
            hostname,
            session_id,
        })
    }
}

/// Argon2id di thread blocking agar tidak menahan runtime async.
#[derive(Clone)]
pub struct BlockingArgon2(pub Arc<Argon2idHasher>);

impl PasswordHashing for BlockingArgon2 {
    async fn hash(&self, password: String) -> Result<String, AuthError> {
        let h = self.0.clone();
        tokio::task::spawn_blocking(move || h.hash(&password))
            .await
            .map_err(|_| AuthError::Internal)?
    }
}

#[derive(Clone, Copy, Default)]
pub struct OsTokenIssuer;

impl TokenIssuer for OsTokenIssuer {
    fn issue(&self) -> Result<(String, TokenHash), AuthError> {
        new_token()
    }
}
