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
pub struct SubscriptionView {
    pub plan: PlanView,
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
    Ok(Json(SuccessEnvelope::new(
        SubscriptionView {
            plan: s.plan.into(),
            status: s.status.as_str().into(),
            trial_ends_at: s.trial_ends_at,
            current_period_end: s.current_period_end,
        },
        &id,
    )))
}

pub fn routes() -> Router<AppState> {
    Router::new().route("/api/v1/plans", get(list_plans)).route(
        "/api/v1/tenants/{tenant_id}/subscription",
        get(subscription),
    )
}
