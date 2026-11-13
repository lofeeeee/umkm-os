//! Area platform admin (P7-3, ADR-0016). Pintu pertama lintas tenant di API:
//! - hanya pengguna `platform_role = super_admin` yang aktif;
//! - sesi wajib lolos MFA (gerbang AUTH-011 di `SessionUser`: super_admin tanpa MFA sudah ditolak di sana);
//! - akses data lintas tenant hanya lewat fungsi DB `app.platform_*` yang sempit; setiap keputusan & akses bukti
//!   bayar diaudit dengan aktor `platform` (kategori PRD §6.9 "Platform admin access").

use axum::{
    Json, Router,
    extract::{FromRequestParts, Path, Query, State},
    http::request::Parts,
    response::Response,
    routing::{get, post},
};
use serde::{Deserialize, Serialize};
use umkm_infrastructure::billing_repo as repo;
use umkm_shared::http::{ErrorBody, ErrorCode, SuccessEnvelope};
use utoipa::{IntoParams, ToSchema};
use uuid::Uuid;

use crate::{
    ApiError, AppState,
    auth::ClientIp,
    billing::{InvoiceView, image, invoice_err, invoice_view},
    http_util::{internal, not_found, pool},
    middleware::ReqId,
    session::{SessionUser, mfa_required},
    validation::one,
};

/// Super admin platform dengan sesi lolos MFA.
#[derive(Debug, Clone, Copy)]
pub struct PlatformAdmin {
    pub user_id: Uuid,
}

impl FromRequestParts<AppState> for PlatformAdmin {
    type Rejection = ApiError;

    async fn from_request_parts(parts: &mut Parts, state: &AppState) -> Result<Self, ApiError> {
        let s = SessionUser::from_request_parts(parts, state).await?;
        let role: Option<String> = sqlx::query_scalar(
            "SELECT platform_role FROM users WHERE id = $1 AND status = 'active'",
        )
        .bind(s.user_id)
        .fetch_optional(pool(state)?)
        .await
        .map_err(internal)?
        .flatten();
        if role.as_deref() != Some("super_admin") {
            return Err(ApiError(
                ErrorBody::new(ErrorCode::Forbidden).with_message("Khusus admin platform"),
            ));
        }
        // Pertahanan kedua di samping gerbang SessionUser (AUTH-011).
        if !s.mfa_verified {
            return Err(mfa_required("Admin platform wajib masuk dengan MFA"));
        }
        Ok(Self { user_id: s.user_id })
    }
}

#[derive(Debug, Serialize, ToSchema)]
pub struct PlatformMe {
    pub user_id: Uuid,
    pub platform_role: String,
}

/// Cek akses admin platform (dipakai dashboard untuk menampilkan menu).
#[utoipa::path(
    get, path = "/api/v1/platform/me", tag = "platform",
    responses(
        (status = 200, description = "Super admin dengan sesi MFA", body = SuccessEnvelope<PlatformMe>),
        (status = 403, description = "Bukan admin platform / belum MFA", body = umkm_shared::http::ErrorEnvelope)
    )
)]
pub async fn me(ReqId(id): ReqId, admin: PlatformAdmin) -> Json<SuccessEnvelope<PlatformMe>> {
    Json(SuccessEnvelope::new(
        PlatformMe {
            user_id: admin.user_id,
            platform_role: "super_admin".into(),
        },
        &id,
    ))
}

#[derive(Debug, Deserialize, IntoParams)]
pub struct InvoiceQuery {
    /// default `submitted` (menunggu verifikasi); `all` = semua
    pub status: Option<String>,
}

/// Antrean tagihan langganan lintas toko (maks 200, terlama dulu).
#[utoipa::path(
    get, path = "/api/v1/platform/invoices", tag = "platform", params(InvoiceQuery),
    responses(
        (status = 200, description = "Tagihan", body = SuccessEnvelope<Vec<InvoiceView>>),
        (status = 403, description = "Bukan admin platform", body = umkm_shared::http::ErrorEnvelope)
    )
)]
pub async fn invoices(
    State(state): State<AppState>,
    ReqId(id): ReqId,
    _admin: PlatformAdmin,
    Query(q): Query<InvoiceQuery>,
) -> Result<Json<SuccessEnvelope<Vec<InvoiceView>>>, ApiError> {
    let status = match q.status.as_deref() {
        None | Some("") => Some("submitted"),
        Some("all") => None,
        Some(s @ ("awaiting_payment" | "submitted" | "paid" | "rejected" | "cancelled")) => Some(s),
        Some(_) => return Err(one("status", "invalid_status")),
    };
    let list = repo::platform_invoices(pool(&state)?, status)
        .await
        .map_err(invoice_err)?;
    Ok(Json(SuccessEnvelope::new(
        list.into_iter().map(|i| invoice_view(i, true)).collect(),
        &id,
    )))
}

/// Bukti bayar untuk verifikasi (akses diaudit).
#[utoipa::path(
    get, path = "/api/v1/platform/invoices/{invoice_id}/proof", tag = "platform",
    params(("invoice_id" = Uuid, Path)),
    responses((status = 200, description = "Gambar", content_type = "image/png"), (status = 404, description = "Tidak ada"))
)]
pub async fn proof(
    State(state): State<AppState>,
    ReqId(id): ReqId,
    ClientIp(ip): ClientIp,
    admin: PlatformAdmin,
    Path(iid): Path<Uuid>,
) -> Result<Response, ApiError> {
    let (ct, bytes) = repo::platform_proof(
        pool(&state)?,
        admin.user_id,
        iid,
        (Some(id.as_str()), Some(&ip)),
    )
    .await
    .map_err(invoice_err)?
    .ok_or_else(not_found)?;
    image(&ct, bytes)
}

#[derive(Debug, Serialize, ToSchema)]
pub struct DecisionView {
    pub invoice_id: Uuid,
    pub tenant_id: Uuid,
    /// `paid` | `rejected`
    pub status: String,
    pub period_end: Option<String>,
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct RejectInput {
    pub reason: String,
}

/// Setujui pembayaran → langganan aktif/diperpanjang + notifikasi toko (atomik).
#[utoipa::path(
    post, path = "/api/v1/platform/invoices/{invoice_id}/approve", tag = "platform",
    params(("invoice_id" = Uuid, Path)),
    responses(
        (status = 200, description = "Langganan aktif", body = SuccessEnvelope<DecisionView>),
        (status = 409, description = "Tagihan tidak menunggu verifikasi", body = umkm_shared::http::ErrorEnvelope)
    )
)]
pub async fn approve(
    State(state): State<AppState>,
    ReqId(id): ReqId,
    ClientIp(ip): ClientIp,
    admin: PlatformAdmin,
    Path(iid): Path<Uuid>,
) -> Result<Json<SuccessEnvelope<DecisionView>>, ApiError> {
    decide(&state, &id, &ip, admin, iid, true, None).await
}

/// Tolak pembayaran (alasan wajib, terlihat oleh toko).
#[utoipa::path(
    post, path = "/api/v1/platform/invoices/{invoice_id}/reject", tag = "platform",
    params(("invoice_id" = Uuid, Path)), request_body = RejectInput,
    responses(
        (status = 200, description = "Ditolak", body = SuccessEnvelope<DecisionView>),
        (status = 409, description = "Tagihan tidak menunggu verifikasi", body = umkm_shared::http::ErrorEnvelope),
        (status = 422, description = "Alasan kosong / terlalu panjang", body = umkm_shared::http::ErrorEnvelope)
    )
)]
pub async fn reject(
    State(state): State<AppState>,
    ReqId(id): ReqId,
    ClientIp(ip): ClientIp,
    admin: PlatformAdmin,
    Path(iid): Path<Uuid>,
    Json(req): Json<RejectInput>,
) -> Result<Json<SuccessEnvelope<DecisionView>>, ApiError> {
    let reason = req.reason.trim();
    if reason.is_empty() || reason.chars().count() > 300 {
        return Err(one(
            "reason",
            if reason.is_empty() {
                "required"
            } else {
                "too_long"
            },
        ));
    }
    decide(&state, &id, &ip, admin, iid, false, Some(reason)).await
}

async fn decide(
    state: &AppState,
    rid: &umkm_shared::http::RequestId,
    ip: &str,
    admin: PlatformAdmin,
    iid: Uuid,
    approve: bool,
    reason: Option<&str>,
) -> Result<Json<SuccessEnvelope<DecisionView>>, ApiError> {
    let (tenant, period_end) = repo::platform_decide(
        pool(state)?,
        admin.user_id,
        iid,
        approve,
        reason,
        (Some(rid.as_str()), Some(ip)),
    )
    .await
    .map_err(invoice_err)?;
    Ok(Json(SuccessEnvelope::new(
        DecisionView {
            invoice_id: iid,
            tenant_id: tenant,
            status: if approve { "paid" } else { "rejected" }.into(),
            period_end,
        },
        rid,
    )))
}

pub fn routes() -> Router<AppState> {
    let p = "/api/v1/platform";
    Router::new()
        .route(&format!("{p}/me"), get(me))
        .route(&format!("{p}/invoices"), get(invoices))
        .route(&format!("{p}/invoices/{{invoice_id}}/proof"), get(proof))
        .route(
            &format!("{p}/invoices/{{invoice_id}}/approve"),
            post(approve),
        )
        .route(&format!("{p}/invoices/{{invoice_id}}/reject"), post(reject))
}
