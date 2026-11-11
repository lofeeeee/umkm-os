//! Extractor tenant dari header `Host` (P1-4, `PRD.md §4.2`, TEN-009).
//!
//! - Identitas tenant HANYA dari `Host` yang terdaftar di `tenant_domains`; tidak pernah
//!   dari body/query/`X-Forwarded-Host` (`PRD.md §4.2`: jangan terima tenant_id dari browser).
//! - Domain non-`active` (pending/suspended/disabled) atau tenant non-`active` -> 404,
//!   tanpa membedakan alasan ke klien (tidak membocorkan keberadaan tenant).
//! - Caddy meneruskan `Host` asli; header `X-Forwarded-Host` diabaikan sepenuhnya.

use axum::{
    extract::FromRequestParts,
    http::{header, request::Parts},
};
use umkm_domain::tenant::normalize_host;
use umkm_infrastructure::tenant_repo;
use umkm_shared::{http::ErrorCode, ids::TenantId};

use crate::{ApiError, AppState};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HostTenant(pub TenantId);

impl FromRequestParts<AppState> for HostTenant {
    type Rejection = ApiError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let raw = parts
            .headers
            .get(header::HOST)
            .and_then(|v| v.to_str().ok())
            .or_else(|| parts.uri.authority().map(|a| a.as_str()))
            .ok_or_else(|| ApiError::new(ErrorCode::BadRequest))?;
        let not_found = || ApiError::new(ErrorCode::ResourceNotFound);
        let host = normalize_host(raw).map_err(|_| not_found())?;
        let db = state
            .db
            .as_ref()
            .ok_or_else(|| ApiError::new(ErrorCode::ServiceUnavailable))?;
        let record = tenant_repo::resolve_host(db, &host).await.map_err(|e| {
            tracing::error!(error = %e, "tenant resolution failed");
            ApiError::new(ErrorCode::InternalError)
        })?;
        match record {
            Some(r) if r.servable() => Ok(Self(r.tenant_id)),
            Some(r) => {
                tracing::info!(%host, domain_status = %r.domain_status, tenant_status = %r.tenant_status, "host not servable");
                Err(not_found())
            }
            None => Err(not_found()),
        }
    }
}
