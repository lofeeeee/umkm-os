//! Paket & langganan (P7-1, `PRD.md §4.15`, FR-SUB-001, ADR-0016).
//! Batas paket dibaca dari data (`plans.limits_json`) setiap permintaan — perubahan berlaku tanpa deploy.

use axum::{Json, Router, extract::State, routing::get};
use serde::Serialize;
use umkm_domain::billing::PlanLimits;
use umkm_infrastructure::billing_repo::{self as repo, BillingError, Plan};
use umkm_shared::http::SuccessEnvelope;
use utoipa::ToSchema;

use crate::{
    ApiError, AppState,
    http_util::{internal, pool},
    middleware::ReqId,
    session::Member,
};

#[derive(Debug, Serialize, ToSchema)]
pub struct LimitsView {
    /// `null` = tak terbatas (fair-use)
    pub max_members: Option<u64>,
    pub max_products: Option<u64>,
    /// 0 = tidak termasuk token AI platform
    pub platform_ai_tokens_monthly: Option<u64>,
    /// Boleh memakai kunci provider AI sendiri (ADR-0011)
    pub byok_allowed: bool,
    pub storage_bytes: Option<u64>,
    pub conversations_monthly: Option<u64>,
    pub custom_domain: bool,
}

impl From<PlanLimits> for LimitsView {
    fn from(l: PlanLimits) -> Self {
        Self {
            max_members: l.max_members,
            max_products: l.max_products,
            platform_ai_tokens_monthly: l.platform_ai_tokens_monthly,
            byok_allowed: l.byok_allowed,
            storage_bytes: l.storage_bytes,
            conversations_monthly: l.conversations_monthly,
            custom_domain: l.custom_domain,
        }
    }
}

#[derive(Debug, Serialize, ToSchema)]
pub struct PlanView {
    pub code: String,
    pub name: String,
    /// Rupiah per periode
    pub price_idr: i32,
    pub period_days: i32,
    /// Terisi untuk paket yang dipakai saat uji coba toko baru
    pub trial_days: Option<i32>,
    pub limits: LimitsView,
}

impl From<Plan> for PlanView {
    fn from(p: Plan) -> Self {
        Self {
            code: p.code,
            name: p.name,
            price_idr: p.price_idr,
            period_days: p.period_days,
            trial_days: p.trial_days,
            limits: p.limits.into(),
        }
    }
}

#[derive(Debug, Serialize, ToSchema)]
pub struct UsageView {
    pub members: i64,
    pub pending_invitations: i64,
    pub products: i64,
    pub storage_bytes: i64,
    pub conversations_month: i64,
    pub platform_tokens_month: i64,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct SubscriptionView {
    pub plan: PlanView,
    /// Pemakaian saat ini terhadap batas paket
    pub usage: UsageView,
    /// Status efektif: `trialing` | `active` | `expired`
    pub status: String,
    pub trial_ends_at: Option<String>,
    pub current_period_end: Option<String>,
}

pub(crate) fn billing_err(e: BillingError) -> ApiError {
    // Paket rusak / langganan hilang = kesalahan operator: fail-closed sebagai 500, dicatat.
    internal(e)
}

/// Paket yang ditawarkan (publik, untuk halaman harga).
#[utoipa::path(
    get, path = "/api/v1/plans", tag = "billing",
    responses((status = 200, description = "Paket aktif", body = SuccessEnvelope<Vec<PlanView>>))
)]
pub async fn list_plans(
    State(state): State<AppState>,
    ReqId(id): ReqId,
) -> Result<Json<SuccessEnvelope<Vec<PlanView>>>, ApiError> {
    let plans = repo::plans(pool(&state)?).await.map_err(billing_err)?;
    Ok(Json(SuccessEnvelope::new(
        plans.into_iter().map(PlanView::from).collect(),
        &id,
    )))
}

/// Paket & status langganan toko. Butuh `tenant.read`.
#[utoipa::path(
    get, path = "/api/v1/tenants/{tenant_id}/subscription", tag = "billing",
    params(("tenant_id" = uuid::Uuid, Path)),
    responses(
        (status = 200, description = "Langganan", body = SuccessEnvelope<SubscriptionView>),
        (status = 404, description = "Bukan anggota", body = umkm_shared::http::ErrorEnvelope)
    )
)]
pub async fn subscription(
    State(state): State<AppState>,
    ReqId(id): ReqId,
    Member(actor): Member,
) -> Result<Json<SuccessEnvelope<SubscriptionView>>, ApiError> {
    actor.require("tenant.read")?;
    let s = repo::current(pool(&state)?, actor.tenant_id)
        .await
        .map_err(billing_err)?;
    let u = repo::usage(pool(&state)?, actor.tenant_id)
        .await
        .map_err(billing_err)?;
    Ok(Json(SuccessEnvelope::new(
        SubscriptionView {
            plan: s.plan.into(),
            usage: UsageView {
                members: u.active_members,
                pending_invitations: u.pending_invitations,
                products: u.products,
                storage_bytes: u.storage_bytes,
                conversations_month: u.conversations_month,
                platform_tokens_month: u.platform_tokens_month,
            },
            status: s.status.as_str().into(),
            trial_ends_at: s.trial_ends_at,
            current_period_end: s.current_period_end,
        },
        &id,
    )))
}

// ------------------------------------------------------------------ penegakan (P7-2)

/// Batas paket yang terlampaui → 403 `details.reason = "plan_limit"` (+ batas & nilainya).
pub(crate) fn over_limit(limit: &'static str, max: u64) -> ApiError {
    ApiError(
        umkm_shared::http::ErrorBody::new(umkm_shared::http::ErrorCode::Forbidden)
            .with_message("Batas paket tercapai. Tingkatkan paket untuk menambah.")
            .with_details(
                serde_json::json!({ "reason": "plan_limit", "limit": limit, "max": max }),
            ),
    )
}

/// Langganan habis → fitur publik toko (website, katalog, checkout, chat) nonaktif (ADR-0016).
pub(crate) fn expired() -> ApiError {
    ApiError(
        umkm_shared::http::ErrorBody::new(umkm_shared::http::ErrorCode::Forbidden)
            .with_message("Toko sedang tidak aktif.")
            .with_details(serde_json::json!({ "reason": "subscription_expired" })),
    )
}

/// Paket + status efektif tenant (dibaca langsung dari data setiap permintaan).
pub(crate) async fn plan_of(
    state: &AppState,
    t: umkm_shared::ids::TenantId,
) -> Result<repo::TenantPlan, ApiError> {
    repo::current(pool(state)?, t).await.map_err(billing_err)
}

/// Untuk endpoint publik toko: tolak bila langganan habis.
pub(crate) async fn require_operational(
    state: &AppState,
    t: umkm_shared::ids::TenantId,
) -> Result<repo::TenantPlan, ApiError> {
    let p = plan_of(state, t).await?;
    if p.status == umkm_domain::billing::SubscriptionStatus::Expired {
        return Err(expired());
    }
    Ok(p)
}

/// Batas jumlah: `current` = pemakaian saat ini (sebelum menambah satu).
pub(crate) fn check_count(
    limit_name: &'static str,
    limit: Option<u64>,
    current: i64,
) -> Result<(), ApiError> {
    let cur = u64::try_from(current.max(0)).unwrap_or(u64::MAX);
    match limit {
        Some(max) if !PlanLimits::allows(Some(max), cur) => Err(over_limit(limit_name, max)),
        _ => Ok(()),
    }
}

pub(crate) async fn usage(
    state: &AppState,
    t: umkm_shared::ids::TenantId,
) -> Result<repo::Usage, ApiError> {
    repo::usage(pool(state)?, t).await.map_err(billing_err)
}

pub fn routes() -> Router<AppState> {
    Router::new().route("/api/v1/plans", get(list_plans)).route(
        "/api/v1/tenants/{tenant_id}/subscription",
        get(subscription),
    )
}
