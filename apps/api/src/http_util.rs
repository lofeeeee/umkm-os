//! Helper handler bersama untuk modul P3+ (catalog, inventory, CRM, commerce).

use axum::{
    Json,
    http::{HeaderValue, header},
    response::{IntoResponse, Response},
};
use serde::Serialize;
use umkm_shared::http::{ErrorBody, ErrorCode, SuccessEnvelope};

use crate::{ApiError, AppState};

pub fn pool(state: &AppState) -> Result<&sqlx::PgPool, ApiError> {
    state
        .auth
        .as_ref()
        .map(|a| &a.pool)
        .or(state.db.as_ref())
        .ok_or_else(|| ApiError::new(ErrorCode::ServiceUnavailable))
}

pub fn internal(e: impl std::fmt::Display) -> ApiError {
    tracing::error!(error = %e, "query failed");
    ApiError::new(ErrorCode::InternalError)
}

pub fn not_found() -> ApiError {
    ApiError::new(ErrorCode::ResourceNotFound)
}

/// `409` dengan `details.reason` yang stabil untuk klien.
pub fn conflict(reason: &str, message: &str) -> ApiError {
    ApiError(
        ErrorBody::new(ErrorCode::Conflict)
            .with_message(message)
            .with_details(serde_json::json!({ "reason": reason })),
    )
}

/// Response publik yang boleh di-cache CDN/browser 60 detik per Host (ADR-0014).
pub fn public_json<T: Serialize>(body: T, request_id: &umkm_shared::http::RequestId) -> Response {
    let mut res = Json(SuccessEnvelope::new(body, request_id)).into_response();
    res.headers_mut().insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static("public, max-age=60"),
    );
    res.headers_mut()
        .insert(header::VARY, HeaderValue::from_static("Host"));
    res
}

pub fn asset_ref(id: uuid::Uuid) -> String {
    format!("asset:{id}")
}

pub fn parse_asset_ref(s: &str) -> Option<uuid::Uuid> {
    s.strip_prefix("asset:")
        .and_then(|x| uuid::Uuid::parse_str(x).ok())
}

#[derive(Debug, Clone, Copy, serde::Deserialize, utoipa::IntoParams)]
pub struct Page {
    /// 1..=100, default 20
    pub limit: Option<i64>,
    /// default 0
    pub offset: Option<i64>,
}

impl Page {
    pub fn bounds(self) -> (i64, i64) {
        (
            self.limit.unwrap_or(20).clamp(1, 100),
            self.offset.unwrap_or(0).clamp(0, 100_000),
        )
    }
}
