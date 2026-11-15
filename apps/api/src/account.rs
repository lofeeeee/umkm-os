//! Keamanan akun staf (AUTH-006 + ubah kata sandi): daftar sesi aktif, cabut satu / semua sesi
//! lain, dan ubah kata sandi. Semua butuh sesi; mutasi butuh CSRF + Origin (extractor `SessionUser`).
//! UAT 15 Nov: halaman Keamanan hanya berisi MFA.

use axum::{
    Json, Router,
    extract::{Path, State},
    response::{IntoResponse, Response},
    routing::{get, post},
};
use serde::{Deserialize, Serialize};
use umkm_domain::{
    audit::{AuditEvent, actions},
    auth::{Email, PasswordHasher, rate_rules, validate_password},
};
use umkm_infrastructure::{audit_repo, auth_repo, mfa as mfa_repo};
use umkm_shared::http::{ErrorCode, SuccessEnvelope};
use utoipa::ToSchema;
use uuid::Uuid;

use crate::{
    ApiError, AppState,
    auth::{AuthService, ClientIp, Reject, enforce, service, validation},
    middleware::ReqId,
    session::SessionUser,
};

#[derive(Debug, Serialize, ToSchema)]
pub struct SessionView {
    pub id: Uuid,
    /// User-Agent saat login (maks 512 karakter); `null` bila tidak dikirim.
    pub user_agent: Option<String>,
    pub created_at: String,
    pub last_seen_at: String,
    pub expires_at: String,
    /// Sesi yang dipakai request ini.
    pub current: bool,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct RevokedView {
    pub revoked: u64,
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct ChangePasswordRequest {
    pub current_password: String,
    pub new_password: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct PasswordChangedView {
    pub changed: bool,
    /// Jumlah sesi lain yang dicabut (perangkat lain harus masuk ulang).
    pub other_sessions_revoked: u64,
}

fn internal<E>(_: E) -> ApiError {
    ApiError::new(ErrorCode::InternalError)
}

async fn audit(
    svc: &AuthService,
    action: &'static str,
    u: &SessionUser,
    res: Uuid,
    req: &str,
    ip: &str,
) {
    audit_repo::record_best_effort(
        &svc.pool,
        &AuditEvent::new(action, "session")
            .by_user(u.user_id)
            .resource(res)
            .request(Some(req), Some(ip)),
    )
    .await;
}

/// Sesi aktif milik akun ini (maks 50, terakhir dipakai dulu).
#[utoipa::path(
    get, path = "/api/v1/auth/sessions", tag = "auth",
    responses(
        (status = 200, description = "Sesi aktif", body = SuccessEnvelope<Vec<SessionView>>),
        (status = 401, description = "Tidak ada sesi valid", body = umkm_shared::http::ErrorEnvelope)
    )
)]
pub async fn list_sessions(
    State(state): State<AppState>,
    ReqId(id): ReqId,
    u: SessionUser,
) -> Result<Json<SuccessEnvelope<Vec<SessionView>>>, ApiError> {
    let svc = service(&state)?;
    let list = auth_repo::list_active_sessions(&svc.pool, u.user_id)
        .await
        .map_err(internal)?
        .into_iter()
        .map(|s| SessionView {
            current: s.id == u.session_id,
            id: s.id,
            user_agent: s.user_agent,
            created_at: s.created_at,
            last_seen_at: s.last_seen_at,
            expires_at: s.expires_at,
        })
        .collect();
    Ok(Json(SuccessEnvelope::new(list, &id)))
}

/// Cabut satu sesi milik akun ini. Sesi saat ini tidak bisa dicabut di sini (pakai logout).
#[utoipa::path(
    post, path = "/api/v1/auth/sessions/{session_id}/revoke", tag = "auth",
    params(("session_id" = Uuid, Path)),
    responses(
        (status = 200, description = "Sesi dicabut", body = SuccessEnvelope<RevokedView>),
        (status = 401, description = "Tidak ada sesi valid", body = umkm_shared::http::ErrorEnvelope),
        (status = 403, description = "CSRF/Origin tidak valid", body = umkm_shared::http::ErrorEnvelope),
        (status = 404, description = "Sesi tidak ada / milik akun lain / sudah dicabut", body = umkm_shared::http::ErrorEnvelope),
        (status = 409, description = "Sesi saat ini — gunakan logout", body = umkm_shared::http::ErrorEnvelope)
    )
)]
pub async fn revoke_session(
    State(state): State<AppState>,
    ReqId(id): ReqId,
    ClientIp(ip): ClientIp,
    u: SessionUser,
    Path(session_id): Path<Uuid>,
) -> Result<Json<SuccessEnvelope<RevokedView>>, ApiError> {
    let svc = service(&state)?;
    if session_id == u.session_id {
        return Err(ApiError::new(ErrorCode::Conflict));
    }
    // Hanya sesi milik user ini (user_id di WHERE): sesi akun lain → 404 seragam, tanpa enumerasi.
    if !auth_repo::revoke_session(&svc.pool, u.user_id, session_id)
        .await
        .map_err(internal)?
    {
        return Err(ApiError::new(ErrorCode::ResourceNotFound));
    }
    audit(
        svc,
        actions::AUTH_SESSION_REVOKED,
        &u,
        session_id,
        id.as_str(),
        &ip,
    )
    .await;
    Ok(Json(SuccessEnvelope::new(RevokedView { revoked: 1 }, &id)))
}

/// Cabut semua sesi lain (keluar dari semua perangkat lain).
#[utoipa::path(
    post, path = "/api/v1/auth/sessions/revoke-others", tag = "auth",
    responses(
        (status = 200, description = "Jumlah sesi yang dicabut", body = SuccessEnvelope<RevokedView>),
        (status = 401, description = "Tidak ada sesi valid", body = umkm_shared::http::ErrorEnvelope),
        (status = 403, description = "CSRF/Origin tidak valid", body = umkm_shared::http::ErrorEnvelope)
    )
)]
pub async fn revoke_others(
    State(state): State<AppState>,
    ReqId(id): ReqId,
    ClientIp(ip): ClientIp,
    u: SessionUser,
) -> Result<Json<SuccessEnvelope<RevokedView>>, ApiError> {
    let svc = service(&state)?;
    let n = auth_repo::revoke_all_sessions(&svc.pool, u.user_id, Some(u.session_id))
        .await
        .map_err(internal)?;
    if n > 0 {
        audit(
            svc,
            actions::AUTH_SESSION_REVOKED,
            &u,
            u.session_id,
            id.as_str(),
            &ip,
        )
        .await;
    }
    Ok(Json(SuccessEnvelope::new(RevokedView { revoked: n }, &id)))
}

/// Ubah kata sandi: wajib kata sandi saat ini; sesi lain dicabut, sesi ini tetap.
#[utoipa::path(
    post, path = "/api/v1/auth/password", tag = "auth",
    request_body = ChangePasswordRequest,
    responses(
        (status = 200, description = "Kata sandi diubah", body = SuccessEnvelope<PasswordChangedView>),
        (status = 401, description = "Tidak ada sesi valid", body = umkm_shared::http::ErrorEnvelope),
        (status = 403, description = "CSRF/Origin tidak valid", body = umkm_shared::http::ErrorEnvelope),
        (status = 422, description = "Kata sandi saat ini salah / kata sandi baru lemah; `details.field`", body = umkm_shared::http::ErrorEnvelope),
        (status = 429, description = "Rate limit", body = umkm_shared::http::ErrorEnvelope)
    )
)]
pub async fn change_password(
    State(state): State<AppState>,
    ReqId(id): ReqId,
    ClientIp(ip): ClientIp,
    u: SessionUser,
    Json(req): Json<ChangePasswordRequest>,
) -> Result<Response, Reject> {
    let svc = service(&state).map_err(Reject::from)?;
    enforce(
        svc,
        &rate_rules::PASSWORD_CHANGE_PER_USER,
        &u.user_id.to_string(),
    )
    .await?;
    let acct = mfa_repo::account(&svc.pool, u.user_id)
        .await
        .map_err(|e| Reject::from(internal(e)))?
        .ok_or_else(|| Reject::from(ApiError::new(ErrorCode::Unauthorized)))?;
    let email = Email::parse(&acct.email).map_err(|e| Reject::from(internal(e)))?;
    if let Err(e) = validate_password(&req.new_password, &email) {
        return Err(Reject::from(validation("new_password", e.to_string())));
    }
    if req.new_password == req.current_password {
        return Err(Reject::from(validation(
            "new_password",
            "kata sandi baru harus berbeda dari yang lama",
        )));
    }
    let hasher = svc.hasher.clone();
    let (cur, new, hash) = (req.current_password, req.new_password, acct.password_hash);
    let new_hash = tokio::task::spawn_blocking(move || -> Result<Option<String>, ()> {
        if !hasher.verify(&cur, &hash).map_err(|_| ())? {
            return Ok(None);
        }
        hasher.hash(&new).map(Some).map_err(|_| ())
    })
    .await
    .map_err(|e| Reject::from(internal(e)))?
    .map_err(|e| Reject::from(internal(e)))?;
    let Some(new_hash) = new_hash else {
        return Err(Reject::from(validation(
            "current_password",
            "kata sandi saat ini salah",
        )));
    };
    let mut tx = svc
        .pool
        .begin()
        .await
        .map_err(|e| Reject::from(internal(e)))?;
    auth_repo::update_password_hash(&mut *tx, u.user_id, &new_hash)
        .await
        .map_err(|e| Reject::from(internal(e)))?;
    let revoked = auth_repo::revoke_all_sessions(&mut *tx, u.user_id, Some(u.session_id))
        .await
        .map_err(|e| Reject::from(internal(e)))?;
    tx.commit().await.map_err(|e| Reject::from(internal(e)))?;
    audit_repo::record_best_effort(
        &svc.pool,
        &AuditEvent::new(actions::AUTH_PASSWORD_CHANGED, "user")
            .by_user(u.user_id)
            .resource(u.user_id)
            .with_summary(serde_json::json!({ "other_sessions_revoked": revoked }))
            .request(Some(id.as_str()), Some(&ip)),
    )
    .await;
    Ok(Json(SuccessEnvelope::new(
        PasswordChangedView {
            changed: true,
            other_sessions_revoked: revoked,
        },
        &id,
    ))
    .into_response())
}

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/api/v1/auth/sessions", get(list_sessions))
        .route("/api/v1/auth/sessions/revoke-others", post(revoke_others))
        .route(
            "/api/v1/auth/sessions/{session_id}/revoke",
            post(revoke_session),
        )
        .route("/api/v1/auth/password", post(change_password))
}
