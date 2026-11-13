//! Extractor sesi & anggota tenant (P1-5).
//!
//! - `SessionUser`: cookie sesi valid. Untuk method yang mengubah state (selain GET/HEAD/
//!   OPTIONS) WAJIB lolos cek Origin + `X-CSRF-Token` (ADR-0015) — tidak bisa dilupakan
//!   per-handler.
//! - `Member`: `SessionUser` + `{tenant_id}` di path sebagai *selector* yang diverifikasi
//!   terhadap membership aktif (`PRD.md §5.7`). Bukan anggota -> 404 (tidak bocor).

use axum::{
    extract::{FromRequestParts, Path},
    http::{Method, request::Parts},
};
use serde::Deserialize;
use umkm_domain::rbac::{Actor, AuthzError};
use umkm_infrastructure::{auth::hash_token, auth_repo, rbac_repo};
use umkm_shared::{
    http::{ErrorBody, ErrorCode},
    ids::TenantId,
};
use uuid::Uuid;

use crate::{
    ApiError, AppState,
    auth::{CSRF_HEADER, constant_time_eq, csrf_token},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SessionUser {
    pub user_id: Uuid,
    pub session_id: Uuid,
    /// Sesi lolos langkah kedua MFA.
    pub mfa_verified: bool,
}

fn forbidden(msg: &str) -> ApiError {
    ApiError(ErrorBody::new(ErrorCode::Forbidden).with_message(msg))
}

/// 403 dengan `details.reason = "mfa_required"`: frontend mengarahkan ke halaman Keamanan akun.
pub fn mfa_required(msg: &str) -> ApiError {
    ApiError(
        ErrorBody::new(ErrorCode::Forbidden)
            .with_message(msg)
            .with_details(serde_json::json!({ "reason": "mfa_required" })),
    )
}

impl FromRequestParts<AppState> for SessionUser {
    type Rejection = ApiError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let s = MfaPendingUser::from_request_parts(parts, state).await?;
        // AUTH-011: role platform wajib MFA → sesi tanpa langkah kedua hanya boleh mengurus MFA.
        if s.platform_mfa_required && !s.user.mfa_verified {
            return Err(mfa_required("Akun ini wajib memakai MFA"));
        }
        Ok(s.user)
    }
}

/// Sesi valid tanpa gerbang MFA platform — HANYA untuk endpoint pendaftaran/status MFA,
/// sesi, dan logout (agar pengguna yang wajib MFA bisa mendaftarkannya).
#[derive(Debug, Clone, Copy)]
pub struct MfaPendingUser {
    pub user: SessionUser,
    pub platform_mfa_required: bool,
}

impl FromRequestParts<AppState> for MfaPendingUser {
    type Rejection = ApiError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let svc = state
            .auth
            .as_ref()
            .ok_or_else(|| ApiError::new(ErrorCode::ServiceUnavailable))?;
        let token = svc
            .session_cookie(&parts.headers)
            .ok_or_else(|| ApiError::new(ErrorCode::Unauthorized))?
            .to_owned();
        let safe = matches!(parts.method, Method::GET | Method::HEAD | Method::OPTIONS);
        if !safe {
            svc.check_origin(&parts.headers)?;
            let sent = parts
                .headers
                .get(CSRF_HEADER)
                .and_then(|v| v.to_str().ok())
                .unwrap_or("");
            if !constant_time_eq(sent.as_bytes(), csrf_token(&token).as_bytes()) {
                return Err(forbidden("CSRF token invalid"));
            }
        }
        let s = auth_repo::authenticate_session(&svc.pool, hash_token(&token))
            .await
            .map_err(|e| {
                tracing::error!(error = %e, "session lookup failed");
                ApiError::new(ErrorCode::InternalError)
            })?
            .ok_or_else(|| ApiError::new(ErrorCode::Unauthorized))?;
        Ok(Self {
            user: SessionUser {
                user_id: s.user_id,
                session_id: s.session_id,
                mfa_verified: s.mfa_verified,
            },
            platform_mfa_required: s.platform_mfa_required,
        })
    }
}

/// Sesi opsional: tanpa cookie → `None`; ada cookie → wajib valid (CSRF untuk mutasi).
impl axum::extract::OptionalFromRequestParts<AppState> for SessionUser {
    type Rejection = ApiError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Option<Self>, Self::Rejection> {
        let has = state
            .auth
            .as_ref()
            .is_some_and(|s| s.session_cookie(&parts.headers).is_some());
        if !has {
            return Ok(None);
        }
        <Self as FromRequestParts<AppState>>::from_request_parts(parts, state)
            .await
            .map(Some)
    }
}

#[derive(Deserialize)]
struct TenantPath {
    tenant_id: Uuid,
}

/// Anggota aktif tenant di path, dengan permission-nya.
#[derive(Debug, Clone)]
pub struct Member(pub Actor);

impl FromRequestParts<AppState> for Member {
    type Rejection = ApiError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let user = SessionUser::from_request_parts(parts, state).await?;
        let not_found = || ApiError::new(ErrorCode::ResourceNotFound);
        let Path(TenantPath { tenant_id }) = Path::<TenantPath>::from_request_parts(parts, state)
            .await
            .map_err(|_| not_found())?;
        let svc = state
            .auth
            .as_ref()
            .ok_or_else(|| ApiError::new(ErrorCode::ServiceUnavailable))?;
        let (actor, require_mfa) = rbac_repo::load_actor_with_policy(
            &svc.pool,
            user.user_id,
            TenantId::from_uuid(tenant_id),
        )
        .await
        .map_err(|e| {
            tracing::error!(error = %e, "actor lookup failed");
            ApiError::new(ErrorCode::InternalError)
        })?
        .ok_or_else(not_found)?;
        // AUTH-012: tenant mewajibkan MFA untuk role admin; sesi tanpa langkah kedua ditolak.
        if umkm_domain::mfa::tenant_requires_mfa(require_mfa, &actor.role_name)
            && !user.mfa_verified
        {
            return Err(mfa_required("Toko ini mewajibkan MFA untuk peran Anda"));
        }
        Ok(Self(actor))
    }
}

impl From<AuthzError> for ApiError {
    fn from(e: AuthzError) -> Self {
        match e {
            AuthzError::MissingPermission(p) => ApiError(
                ErrorBody::new(ErrorCode::Forbidden)
                    .with_message(e.to_string())
                    .with_details(serde_json::json!({ "required_permission": p })),
            ),
            // Objek tenant lain / tidak ditugaskan: jangan bocorkan keberadaannya.
            AuthzError::ObjectDenied => ApiError::new(ErrorCode::ResourceNotFound),
        }
    }
}
