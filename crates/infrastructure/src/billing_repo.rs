//! Paket & langganan (P7-1, ADR-0016). `plans` = katalog platform (hanya-baca untuk aplikasi);
//! `subscriptions` = per tenant (RLS), ditulis hanya oleh fungsi/peran platform.

use sqlx::{PgPool, Row};
use umkm_domain::billing::{LimitsError, PlanLimits, SubscriptionStatus, effective, parse_limits};
use umkm_shared::ids::TenantId;

use crate::pgx::tenant_tx;

#[derive(Debug, thiserror::Error)]
pub enum BillingError {
    #[error("langganan tenant tidak ada")]
    NoSubscription,
    #[error("konfigurasi paket rusak: {0}")]
    BadPlan(#[from] LimitsError),
    #[error(transparent)]
    Db(#[from] sqlx::Error),
}

#[derive(Debug, Clone)]
pub struct Plan {
    pub code: String,
    pub name: String,
    pub price_idr: i32,
    pub period_days: i32,
    pub trial_days: Option<i32>,
    pub limits: PlanLimits,
}

fn plan_of(r: &sqlx::postgres::PgRow) -> Result<Plan, BillingError> {
    Ok(Plan {
        code: r.get("code"),
        name: r.get("name"),
        price_idr: r.get("price_idr"),
        period_days: r.get("period_days"),
        trial_days: r.get("trial_days"),
        limits: parse_limits(&r.get::<serde_json::Value, _>("limits_json"))?,
    })
}

/// Paket aktif (urutan tampil). Dibaca langsung tiap permintaan — perubahan data berlaku tanpa deploy.
pub async fn plans(pool: &PgPool) -> Result<Vec<Plan>, BillingError> {
    sqlx::query(
        "SELECT code, name, price_idr, period_days, trial_days, limits_json FROM plans WHERE active ORDER BY position, code",
    )
    .fetch_all(pool)
    .await?
    .iter()
    .map(plan_of)
    .collect()
}

#[derive(Debug, Clone)]
pub struct TenantPlan {
    pub plan: Plan,
    /// Status tersimpan.
    pub stored_status: SubscriptionStatus,
    /// Status efektif saat ini (uji coba/periode lewat = expired).
    pub status: SubscriptionStatus,
    pub trial_ends_at: Option<String>,
    pub current_period_end: Option<String>,
}

const TS: &str = "'YYYY-MM-DD\"T\"HH24:MI:SS.US\"Z\"'";

/// Paket + status langganan tenant.
pub async fn current(pool: &PgPool, t: TenantId) -> Result<TenantPlan, BillingError> {
    let mut tx = tenant_tx(pool, t).await?;
    let r = sqlx::query(sqlx::AssertSqlSafe(format!(
        "SELECT p.code, p.name, p.price_idr, p.period_days, p.trial_days, p.limits_json, s.status,
                extract(epoch FROM s.trial_ends_at)::bigint AS trial_epoch,
                extract(epoch FROM s.current_period_end)::bigint AS period_epoch,
                extract(epoch FROM now())::bigint AS now_epoch,
                to_char(s.trial_ends_at AT TIME ZONE 'UTC', {TS}) AS trial_ends_at,
                to_char(s.current_period_end AT TIME ZONE 'UTC', {TS}) AS current_period_end
           FROM subscriptions s JOIN plans p ON p.code = s.plan_code WHERE s.tenant_id = $1"
    )))
    .bind(t.as_uuid())
    .fetch_optional(&mut *tx)
    .await?
    .ok_or(BillingError::NoSubscription)?;
    tx.commit().await?;
    let stored = SubscriptionStatus::parse(r.get::<&str, _>("status"))
        .unwrap_or(SubscriptionStatus::Expired);
    Ok(TenantPlan {
        plan: plan_of(&r)?,
        stored_status: stored,
        status: effective(
            stored,
            r.get("trial_epoch"),
            r.get("period_epoch"),
            r.get("now_epoch"),
        ),
        trial_ends_at: r.get("trial_ends_at"),
        current_period_end: r.get("current_period_end"),
    })
}

/// Pemakaian tenant untuk batas paket (P7-2). Bulan = bulan kalender Asia/Jakarta.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Usage {
    pub active_members: i64,
    pub pending_invitations: i64,
    /// Produk selain `archived`.
    pub products: i64,
    pub storage_bytes: i64,
    /// Percakapan baru bulan ini (dari agregat P6-6, tidak berkurang saat purge).
    pub conversations_month: i64,
    /// Token AI platform bulan ini (giliran chat & uji coba).
    pub platform_tokens_month: i64,
}

pub async fn usage(pool: &PgPool, t: TenantId) -> Result<Usage, BillingError> {
    let mut tx = tenant_tx(pool, t).await?;
    let r = sqlx::query(
        "SELECT
           (SELECT count(*) FROM tenant_memberships WHERE tenant_id = $1 AND status = 'active') AS members,
           (SELECT count(*) FROM invitations WHERE tenant_id = $1 AND accepted_at IS NULL AND revoked_at IS NULL
                AND expires_at > now()) AS invites,
           (SELECT count(*) FROM products WHERE tenant_id = $1 AND status <> 'archived') AS products,
           (SELECT coalesce(sum(byte_size), 0)::bigint FROM stored_objects WHERE tenant_id = $1) AS storage,
           (SELECT coalesce(sum(started), 0)::bigint FROM conversation_daily_stats WHERE tenant_id = $1
                AND date_trunc('month', day) = date_trunc('month', app.jkt_day(now()))) AS conv,
           (SELECT coalesce(sum(prompt_tokens + completion_tokens), 0)::bigint FROM ai_runs WHERE tenant_id = $1
                AND source = 'platform'
                AND date_trunc('month', app.jkt_day(created_at)) = date_trunc('month', app.jkt_day(now()))) AS tokens",
    )
    .bind(t.as_uuid())
    .fetch_one(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(Usage {
        active_members: r.get("members"),
        pending_invitations: r.get("invites"),
        products: r.get("products"),
        storage_bytes: r.get("storage"),
        conversations_month: r.get("conv"),
        platform_tokens_month: r.get("tokens"),
    })
}
