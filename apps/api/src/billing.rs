//! Paket & langganan (P7-1, `PRD.md §4.15`, FR-SUB-001, ADR-0016).
//! Batas paket dibaca dari data (`plans.limits_json`) setiap permintaan — perubahan berlaku tanpa deploy.

use axum::{
    Json, Router,
    body::Bytes,
    extract::{Path, State},
    http::{HeaderValue, StatusCode, header},
    response::{IntoResponse, Response},
    routing::{get, post},
};
use serde::Serialize;
use umkm_domain::billing::PlanLimits;
use umkm_infrastructure::billing_repo::{self as repo, BillingError, Invoice, InvoiceError, Plan};
use umkm_shared::http::SuccessEnvelope;
use utoipa::ToSchema;
use uuid::Uuid;

use crate::{
    ApiError, AppState,
    auth::ClientIp,
    http_util::{conflict, internal, not_found, pool},
    middleware::ReqId,
    session::Member,
    validation::one,
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

// ------------------------------------------------------------------ tagihan & pembayaran manual (P7-3)

/// Ukuran maksimum bukti bayar (sama dengan bukti bayar pesanan).
pub const MAX_PROOF_BYTES: usize = 2 * 1024 * 1024;

#[derive(Debug, Serialize, ToSchema)]
pub struct InvoiceView {
    pub id: Uuid,
    pub code: String,
    pub plan_code: String,
    pub amount_idr: i32,
    pub period_days: i32,
    /// `awaiting_payment` | `submitted` | `paid` | `rejected` | `cancelled`
    pub status: String,
    pub has_proof: bool,
    pub rejection_reason: Option<String>,
    pub period_end: Option<String>,
    pub created_at: String,
    /// Hanya pada daftar platform
    pub tenant_id: Option<Uuid>,
    pub tenant_name: Option<String>,
    pub tenant_slug: Option<String>,
}

pub(crate) fn invoice_view(i: Invoice, platform: bool) -> InvoiceView {
    InvoiceView {
        id: i.id,
        code: i.code,
        plan_code: i.plan_code,
        amount_idr: i.amount_idr,
        period_days: i.period_days,
        status: i.status,
        has_proof: i.has_proof,
        rejection_reason: i.rejection_reason,
        period_end: i.period_end,
        created_at: i.created_at,
        tenant_id: platform.then_some(i.tenant_id),
        tenant_name: i.tenant_name,
        tenant_slug: i.tenant_slug,
    }
}

pub(crate) fn invoice_err(e: InvoiceError) -> ApiError {
    match e {
        InvoiceError::NotFound => not_found(),
        InvoiceError::PlanUnavailable => one("plan_code", "unknown_value"),
        InvoiceError::PendingExists => conflict(
            "invoice_pending",
            "Masih ada pembayaran yang menunggu verifikasi",
        ),
        InvoiceError::InvalidState => {
            conflict("invalid_state", "Status tagihan tidak mengizinkan aksi ini")
        }
        e => internal(e),
    }
}

#[derive(Debug, Serialize, ToSchema)]
pub struct InvoiceList {
    pub items: Vec<InvoiceView>,
    /// Instruksi pembayaran platform (rekening/QRIS); kosong = belum diatur operator
    pub payment_instructions: String,
}

/// Tagihan langganan toko + instruksi pembayaran. Butuh `tenant.read`.
#[utoipa::path(
    get, path = "/api/v1/tenants/{tenant_id}/subscription/invoices", tag = "billing",
    params(("tenant_id" = Uuid, Path)),
    responses((status = 200, description = "Tagihan (terbaru dulu)", body = SuccessEnvelope<InvoiceList>))
)]
pub async fn list_invoices(
    State(state): State<AppState>,
    ReqId(id): ReqId,
    Member(actor): Member,
) -> Result<Json<SuccessEnvelope<InvoiceList>>, ApiError> {
    actor.require("tenant.read")?;
    let items = repo::invoices(pool(&state)?, actor.tenant_id)
        .await
        .map_err(invoice_err)?;
    Ok(Json(SuccessEnvelope::new(
        InvoiceList {
            items: items.into_iter().map(|i| invoice_view(i, false)).collect(),
            payment_instructions: state.billing_instructions.to_string(),
        },
        &id,
    )))
}

#[derive(Debug, serde::Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct InvoiceInput {
    /// `starter` | `pro`
    pub plan_code: String,
}

/// Buat tagihan paket (Owner, `tenant.update`). Nominal dari katalog; tagihan lama yang belum dibayar dibatalkan.
#[utoipa::path(
    post, path = "/api/v1/tenants/{tenant_id}/subscription/invoices", tag = "billing",
    params(("tenant_id" = Uuid, Path)), request_body = InvoiceInput,
    responses(
        (status = 201, description = "Tagihan dibuat", body = SuccessEnvelope<InvoiceView>),
        (status = 409, description = "Masih ada pembayaran menunggu verifikasi", body = umkm_shared::http::ErrorEnvelope),
        (status = 422, description = "Paket tidak tersedia", body = umkm_shared::http::ErrorEnvelope)
    )
)]
pub async fn create_invoice(
    State(state): State<AppState>,
    ReqId(id): ReqId,
    ClientIp(ip): ClientIp,
    Member(actor): Member,
    Json(req): Json<InvoiceInput>,
) -> Result<(StatusCode, Json<SuccessEnvelope<InvoiceView>>), ApiError> {
    actor.require("tenant.update")?;
    let i = repo::create_invoice(
        pool(&state)?,
        &actor,
        req.plan_code.trim(),
        (Some(id.as_str()), Some(&ip)),
    )
    .await
    .map_err(invoice_err)?;
    Ok((
        StatusCode::CREATED,
        Json(SuccessEnvelope::new(invoice_view(i, false), &id)),
    ))
}

/// Unggah bukti bayar (gambar PNG/JPEG/WebP ≤ 2 MB, body mentah). Butuh `tenant.update`.
#[utoipa::path(
    post, path = "/api/v1/tenants/{tenant_id}/subscription/invoices/{invoice_id}/proof", tag = "billing",
    params(("tenant_id" = Uuid, Path), ("invoice_id" = Uuid, Path)),
    request_body(content = Vec<u8>, content_type = "application/octet-stream"),
    responses(
        (status = 200, description = "Menunggu verifikasi platform", body = SuccessEnvelope<InvoiceView>),
        (status = 409, description = "Tagihan sudah diputus/dibatalkan", body = umkm_shared::http::ErrorEnvelope),
        (status = 422, description = "Bukan gambar yang diizinkan / terlalu besar", body = umkm_shared::http::ErrorEnvelope)
    )
)]
pub async fn upload_proof(
    State(state): State<AppState>,
    ReqId(id): ReqId,
    ClientIp(ip): ClientIp,
    Member(actor): Member,
    Path((_, iid)): Path<(Uuid, Uuid)>,
    body: Bytes,
) -> Result<Json<SuccessEnvelope<InvoiceView>>, ApiError> {
    actor.require("tenant.update")?;
    // P9-9: bukti bayar langganan disimpan tanpa metadata.
    let (ty, body) = umkm_domain::storage::sanitize_image(&body, MAX_PROOF_BYTES)
        .map_err(|e| one("file", e.code()))?;
    let i = repo::submit_proof(
        pool(&state)?,
        &actor,
        iid,
        ty.content_type(),
        &body,
        (Some(id.as_str()), Some(&ip)),
    )
    .await
    .map_err(invoice_err)?;
    Ok(Json(SuccessEnvelope::new(invoice_view(i, false), &id)))
}

pub(crate) fn image(ct: &str, bytes: Vec<u8>) -> Result<Response, ApiError> {
    let mut res = bytes.into_response();
    let h = res.headers_mut();
    h.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_str(ct).map_err(internal)?,
    );
    h.insert(
        "x-content-type-options",
        HeaderValue::from_static("nosniff"),
    );
    h.insert(
        header::CONTENT_SECURITY_POLICY,
        HeaderValue::from_static("default-src 'none'; sandbox"),
    );
    h.insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static("private, no-store"),
    );
    Ok(res)
}

/// Bukti bayar yang diunggah toko ini. Butuh `tenant.read`.
#[utoipa::path(
    get, path = "/api/v1/tenants/{tenant_id}/subscription/invoices/{invoice_id}/proof", tag = "billing",
    params(("tenant_id" = Uuid, Path), ("invoice_id" = Uuid, Path)),
    responses((status = 200, description = "Gambar", content_type = "image/png"), (status = 404, description = "Tidak ada"))
)]
pub async fn get_proof(
    State(state): State<AppState>,
    Member(actor): Member,
    Path((_, iid)): Path<(Uuid, Uuid)>,
) -> Result<Response, ApiError> {
    actor.require("tenant.read")?;
    let (ct, bytes) = repo::proof(pool(&state)?, actor.tenant_id, iid)
        .await
        .map_err(invoice_err)?
        .ok_or_else(not_found)?;
    image(&ct, bytes)
}

/// Batalkan tagihan yang belum diputus. Butuh `tenant.update`.
#[utoipa::path(
    post, path = "/api/v1/tenants/{tenant_id}/subscription/invoices/{invoice_id}/cancel", tag = "billing",
    params(("tenant_id" = Uuid, Path), ("invoice_id" = Uuid, Path)),
    responses((status = 204, description = "Dibatalkan"), (status = 409, description = "Sudah diputus", body = umkm_shared::http::ErrorEnvelope))
)]
pub async fn cancel_invoice(
    State(state): State<AppState>,
    Member(actor): Member,
    Path((_, iid)): Path<(Uuid, Uuid)>,
) -> Result<StatusCode, ApiError> {
    actor.require("tenant.update")?;
    repo::cancel_invoice(pool(&state)?, actor.tenant_id, iid)
        .await
        .map_err(invoice_err)?;
    Ok(StatusCode::NO_CONTENT)
}

pub fn routes() -> Router<AppState> {
    let s = "/api/v1/tenants/{tenant_id}/subscription";
    Router::new()
        .route("/api/v1/plans", get(list_plans))
        .route(s, get(subscription))
        .route(
            &format!("{s}/invoices"),
            get(list_invoices).post(create_invoice),
        )
        .route(
            &format!("{s}/invoices/{{invoice_id}}/proof"),
            get(get_proof).post(upload_proof),
        )
        .route(
            &format!("{s}/invoices/{{invoice_id}}/cancel"),
            post(cancel_invoice),
        )
}
