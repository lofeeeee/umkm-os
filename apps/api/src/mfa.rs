//! MFA TOTP (AUTH-011/012, Gate F 2026-11-12).
//!
//! - `POST /auth/login/mfa`: langkah kedua setelah `/auth/login` menjawab 202.
//! - `/auth/mfa*`: status, setup (rahasia + URI otpauth), aktifkan (→ kode pemulihan), matikan,
//!   ganti kode pemulihan. Memakai `MfaPendingUser` agar akun yang wajib MFA bisa mendaftar.
//! - `/tenants/{id}/security`: Owner mewajibkan MFA untuk role admin tenant.

use std::time::{SystemTime, UNIX_EPOCH};

use axum::{
    Json, Router,
    extract::State,
    http::{HeaderMap, header},
    response::{IntoResponse, Response},
    routing::{get, post},
};
use serde::{Deserialize, Serialize};
use umkm_domain::{
    audit::{AuditEvent, actions},
    auth::{PasswordHasher, rate_rules},
    mfa::{self, Code},
};
use umkm_infrastructure::{
    audit_repo,
    mfa::{self as repo, MfaError},
    rbac_repo,
};
use umkm_shared::http::{ErrorBody, ErrorCode, SuccessEnvelope};
use utoipa::ToSchema;
use uuid::Uuid;

use crate::{
    ApiError, AppState,
    auth::{
        AuthService, ClientIp, Reject, SessionResponse, csrf_token, enforce, service, user_agent,
        validation,
    },
    middleware::ReqId,
    session::{Member, MfaPendingUser, SessionUser},
};

fn now_unix() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

fn keyring(svc: &AuthService) -> Result<&umkm_infrastructure::crypto::Keyring, ApiError> {
    svc.keyring
        .as_ref()
        .ok_or_else(|| ApiError::new(ErrorCode::ServiceUnavailable))
}

fn invalid_code() -> ApiError {
    validation("code", "Kode tidak valid")
}

impl From<MfaError> for ApiError {
    fn from(e: MfaError) -> Self {
        match e {
            MfaError::InvalidCode => invalid_code(),
            MfaError::InvalidChallenge => ApiError(
                ErrorBody::new(ErrorCode::Unauthorized)
                    .with_message("Sesi verifikasi berakhir; silakan masuk lagi"),
            ),
            MfaError::AlreadyEnabled => {
                ApiError(ErrorBody::new(ErrorCode::Conflict).with_message("MFA sudah aktif"))
            }
            MfaError::NotSetUp | MfaError::NotEnabled => {
                ApiError(ErrorBody::new(ErrorCode::Conflict).with_message(e.to_string()))
            }
            MfaError::Crypto(_) | MfaError::Random | MfaError::Db(_) => {
                tracing::error!(error = %e, "mfa operation failed");
                ApiError::new(ErrorCode::InternalError)
            }
        }
    }
}

fn parse(code: &str) -> Result<Code, ApiError> {
    mfa::parse_code(code).ok_or_else(invalid_code)
}

fn totp_only(code: &str) -> Result<u32, ApiError> {
    match parse(code)? {
        Code::Totp(c) => Ok(c),
        Code::Recovery(_) => Err(invalid_code()),
    }
}

async fn audit(svc: &AuthService, action: &'static str, user: Uuid, req: &str, ip: &str) {
    audit_repo::record_best_effort(
        &svc.pool,
        &AuditEvent::new(action, "user")
            .by_user(user)
            .resource(user)
            .request(Some(req), Some(ip)),
    )
    .await;
}

// ------------------------------------------------------------------ login langkah kedua

#[derive(Debug, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct LoginMfaRequest {
    pub mfa_token: String,
    /// 6 digit dari authenticator atau kode pemulihan `xxxxx-xxxxx`.
    pub code: String,
}

/// Selesaikan login dengan kode MFA. Kode salah 5× membatalkan tantangan.
#[utoipa::path(
    post, path = "/api/v1/auth/login/mfa", tag = "auth",
    request_body = LoginMfaRequest,
    responses(
        (status = 200, description = "Sesi baru bertanda MFA; cookie di-set", body = SuccessEnvelope<SessionResponse>),
        (status = 401, description = "Tantangan tidak valid/kedaluwarsa/habis percobaan", body = umkm_shared::http::ErrorEnvelope),
        (status = 403, description = "Origin tidak diizinkan", body = umkm_shared::http::ErrorEnvelope),
        (status = 422, description = "Kode salah; `details.field = code`", body = umkm_shared::http::ErrorEnvelope),
        (status = 429, description = "Rate limit; header Retry-After", body = umkm_shared::http::ErrorEnvelope),
        (status = 503, description = "Rate limiter/DB/KEK tidak tersedia", body = umkm_shared::http::ErrorEnvelope)
    )
)]
pub async fn login_mfa(
    State(state): State<AppState>,
    ReqId(id): ReqId,
    ClientIp(ip): ClientIp,
    headers: HeaderMap,
    Json(req): Json<LoginMfaRequest>,
) -> Result<Response, Reject> {
    let svc = service(&state).map_err(Reject::from)?;
    svc.check_origin(&headers).map_err(Reject::from)?;
    enforce(svc, &rate_rules::MFA_PER_IP, &ip).await?;
    let k = keyring(svc).map_err(Reject::from)?;
    // Kode yang formatnya salah tetap dihitung sebagai percobaan.
    let code = mfa::parse_code(&req.code).unwrap_or(Code::Totp(u32::MAX));
    let out = repo::complete_login(
        &svc.pool,
        k,
        req.mfa_token.trim(),
        &code,
        now_unix(),
        svc.policy.session_ttl,
        user_agent(&headers).as_deref(),
    )
    .await;
    let login = match out {
        Ok(l) => l,
        Err(e) => {
            if matches!(e, MfaError::InvalidCode) {
                audit_repo::record_best_effort(
                    &svc.pool,
                    &AuditEvent::new(actions::AUTH_MFA_FAILED, "user")
                        .request(Some(id.as_str()), Some(&ip)),
                )
                .await;
            }
            return Err(Reject::from(ApiError::from(e)));
        }
    };
    tracing::info!(user_id = %login.user_id, method = login.method.as_str(), "login succeeded (mfa)");
    audit_repo::record_best_effort(
        &svc.pool,
        &AuditEvent::new(actions::AUTH_LOGIN_SUCCEEDED, "session")
            .by_user(login.user_id)
            .resource(login.session_id)
            .request(Some(id.as_str()), Some(&ip))
            .with_summary(serde_json::json!({ "mfa": login.method.as_str() })),
    )
    .await;
    let body = SuccessEnvelope::new(
        SessionResponse {
            user_id: login.user_id,
            session_id: login.session_id,
            csrf_token: csrf_token(&login.session_token),
        },
        &id,
    );
    Ok((
        [(header::SET_COOKIE, svc.set_cookie(&login.session_token))],
        Json(body),
    )
        .into_response())
}

// ------------------------------------------------------------------ pengelolaan MFA akun

#[derive(Debug, Serialize, ToSchema)]
pub struct MfaStatusView {
    pub enabled: bool,
    pub recovery_codes_remaining: i64,
    /// Sesi ini lolos langkah kedua.
    pub session_verified: bool,
    /// AUTH-011: role platform wajib MFA (tidak bisa dimatikan).
    pub required_by_platform: bool,
}

/// Status MFA akun pemanggil.
#[utoipa::path(
    get, path = "/api/v1/auth/mfa", tag = "auth",
    responses(
        (status = 200, description = "Status MFA", body = SuccessEnvelope<MfaStatusView>),
        (status = 401, description = "Tidak ada sesi valid", body = umkm_shared::http::ErrorEnvelope)
    )
)]
pub async fn mfa_status(
    State(state): State<AppState>,
    ReqId(id): ReqId,
    u: MfaPendingUser,
) -> Result<Json<SuccessEnvelope<MfaStatusView>>, ApiError> {
    let svc = service(&state)?;
    let s = repo::status(&svc.pool, u.user.user_id).await?;
    Ok(Json(SuccessEnvelope::new(
        MfaStatusView {
            enabled: s.enabled,
            recovery_codes_remaining: s.recovery_codes_remaining,
            session_verified: u.user.mfa_verified,
            required_by_platform: u.platform_mfa_required,
        },
        &id,
    )))
}

#[derive(Debug, Serialize, ToSchema)]
pub struct MfaSetupView {
    /// Kunci manual base32 — ditampilkan sekali; tidak dapat diambil lagi.
    pub secret: String,
    pub otpauth_uri: String,
}

/// Buat rahasia TOTP baru (belum aktif sampai dikonfirmasi lewat `/auth/mfa/enable`).
#[utoipa::path(
    post, path = "/api/v1/auth/mfa/setup", tag = "auth",
    responses(
        (status = 200, description = "Rahasia baru", body = SuccessEnvelope<MfaSetupView>),
        (status = 401, description = "Tidak ada sesi valid", body = umkm_shared::http::ErrorEnvelope),
        (status = 403, description = "CSRF/Origin tidak valid", body = umkm_shared::http::ErrorEnvelope),
        (status = 409, description = "MFA sudah aktif", body = umkm_shared::http::ErrorEnvelope),
        (status = 429, description = "Rate limit", body = umkm_shared::http::ErrorEnvelope)
    )
)]
pub async fn mfa_setup(
    State(state): State<AppState>,
    ReqId(id): ReqId,
    u: MfaPendingUser,
) -> Result<Response, Reject> {
    let svc = service(&state).map_err(Reject::from)?;
    enforce(svc, &rate_rules::MFA_PER_USER, &u.user.user_id.to_string()).await?;
    let k = keyring(svc).map_err(Reject::from)?;
    let acct = repo::account(&svc.pool, u.user.user_id)
        .await
        .map_err(ApiError::from)?
        .ok_or_else(|| ApiError::new(ErrorCode::Unauthorized))?;
    let secret = repo::begin_setup(&svc.pool, k, u.user.user_id)
        .await
        .map_err(ApiError::from)?;
    let otpauth_uri = mfa::otpauth_uri(&acct.email, &secret);
    Ok(Json(SuccessEnvelope::new(
        MfaSetupView {
            secret,
            otpauth_uri,
        },
        &id,
    ))
    .into_response())
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct MfaCodeRequest {
    pub code: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct RecoveryCodesView {
    /// Ditampilkan sekali; hanya hash yang disimpan.
    pub recovery_codes: Vec<String>,
}

/// Aktifkan MFA dengan kode authenticator. Sesi lain akun ini dicabut.
#[utoipa::path(
    post, path = "/api/v1/auth/mfa/enable", tag = "auth",
    request_body = MfaCodeRequest,
    responses(
        (status = 200, description = "MFA aktif; kode pemulihan", body = SuccessEnvelope<RecoveryCodesView>),
        (status = 401, description = "Tidak ada sesi valid", body = umkm_shared::http::ErrorEnvelope),
        (status = 403, description = "CSRF/Origin tidak valid", body = umkm_shared::http::ErrorEnvelope),
        (status = 409, description = "Belum setup / sudah aktif", body = umkm_shared::http::ErrorEnvelope),
        (status = 422, description = "Kode salah", body = umkm_shared::http::ErrorEnvelope),
        (status = 429, description = "Rate limit", body = umkm_shared::http::ErrorEnvelope)
    )
)]
pub async fn mfa_enable(
    State(state): State<AppState>,
    ReqId(id): ReqId,
    ClientIp(ip): ClientIp,
    u: MfaPendingUser,
    Json(req): Json<MfaCodeRequest>,
) -> Result<Response, Reject> {
    let svc = service(&state).map_err(Reject::from)?;
    enforce(svc, &rate_rules::MFA_PER_USER, &u.user.user_id.to_string()).await?;
    let k = keyring(svc).map_err(Reject::from)?;
    let code = totp_only(&req.code).map_err(Reject::from)?;
    let codes = repo::enable(
        &svc.pool,
        k,
        u.user.user_id,
        u.user.session_id,
        code,
        now_unix(),
    )
    .await
    .map_err(ApiError::from)?;
    audit(
        svc,
        actions::AUTH_MFA_ENABLED,
        u.user.user_id,
        id.as_str(),
        &ip,
    )
    .await;
    Ok(Json(SuccessEnvelope::new(
        RecoveryCodesView {
            recovery_codes: codes,
        },
        &id,
    ))
    .into_response())
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct MfaDisableRequest {
    pub password: String,
    /// Kode authenticator atau kode pemulihan.
    pub code: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct MfaDisabledView {
    pub disabled: bool,
}

/// Matikan MFA: butuh password + kode. Ditolak untuk akun yang wajib MFA (AUTH-011).
#[utoipa::path(
    post, path = "/api/v1/auth/mfa/disable", tag = "auth",
    request_body = MfaDisableRequest,
    responses(
        (status = 200, description = "MFA dimatikan", body = SuccessEnvelope<MfaDisabledView>),
        (status = 401, description = "Tidak ada sesi valid", body = umkm_shared::http::ErrorEnvelope),
        (status = 403, description = "Akun wajib MFA, atau CSRF/Origin tidak valid", body = umkm_shared::http::ErrorEnvelope),
        (status = 409, description = "MFA belum aktif", body = umkm_shared::http::ErrorEnvelope),
        (status = 422, description = "Password/kode salah; `details.field`", body = umkm_shared::http::ErrorEnvelope),
        (status = 429, description = "Rate limit", body = umkm_shared::http::ErrorEnvelope)
    )
)]
pub async fn mfa_disable(
    State(state): State<AppState>,
    ReqId(id): ReqId,
    ClientIp(ip): ClientIp,
    u: SessionUser,
    Json(req): Json<MfaDisableRequest>,
) -> Result<Response, Reject> {
    let svc = service(&state).map_err(Reject::from)?;
    enforce(svc, &rate_rules::MFA_PER_USER, &u.user_id.to_string()).await?;
    let k = keyring(svc).map_err(Reject::from)?;
    let acct = repo::account(&svc.pool, u.user_id)
        .await
        .map_err(ApiError::from)?
        .ok_or_else(|| ApiError::new(ErrorCode::Unauthorized))?;
    if acct.platform_mfa_required {
        return Err(Reject::from(ApiError(
            ErrorBody::new(ErrorCode::Forbidden).with_message("Akun ini wajib memakai MFA"),
        )));
    }
    let hasher = svc.hasher.clone();
    let (pw, hash) = (req.password, acct.password_hash);
    let ok = tokio::task::spawn_blocking(move || hasher.verify(&pw, &hash))
        .await
        .map_err(|_| Reject::from(ApiError::new(ErrorCode::InternalError)))?
        .map_err(|_| Reject::from(ApiError::new(ErrorCode::InternalError)))?;
    if !ok {
        return Err(Reject::from(validation("password", "Password salah")));
    }
    let code = parse(&req.code).map_err(Reject::from)?;
    repo::disable(&svc.pool, k, u.user_id, &code, now_unix())
        .await
        .map_err(ApiError::from)?;
    audit(svc, actions::AUTH_MFA_DISABLED, u.user_id, id.as_str(), &ip).await;
    Ok(Json(SuccessEnvelope::new(
        MfaDisabledView { disabled: true },
        &id,
    ))
    .into_response())
}

/// Ganti seluruh kode pemulihan (butuh kode authenticator).
#[utoipa::path(
    post, path = "/api/v1/auth/mfa/recovery-codes", tag = "auth",
    request_body = MfaCodeRequest,
    responses(
        (status = 200, description = "Kode pemulihan baru", body = SuccessEnvelope<RecoveryCodesView>),
        (status = 401, description = "Tidak ada sesi valid", body = umkm_shared::http::ErrorEnvelope),
        (status = 409, description = "MFA belum aktif", body = umkm_shared::http::ErrorEnvelope),
        (status = 422, description = "Kode salah", body = umkm_shared::http::ErrorEnvelope),
        (status = 429, description = "Rate limit", body = umkm_shared::http::ErrorEnvelope)
    )
)]
pub async fn mfa_recovery_codes(
    State(state): State<AppState>,
    ReqId(id): ReqId,
    ClientIp(ip): ClientIp,
    u: SessionUser,
    Json(req): Json<MfaCodeRequest>,
) -> Result<Response, Reject> {
    let svc = service(&state).map_err(Reject::from)?;
    enforce(svc, &rate_rules::MFA_PER_USER, &u.user_id.to_string()).await?;
    let k = keyring(svc).map_err(Reject::from)?;
    let code = totp_only(&req.code).map_err(Reject::from)?;
    let codes = repo::regenerate_recovery(&svc.pool, k, u.user_id, code, now_unix())
        .await
        .map_err(ApiError::from)?;
    audit(
        svc,
        actions::AUTH_MFA_RECOVERY_REGENERATED,
        u.user_id,
        id.as_str(),
        &ip,
    )
    .await;
    Ok(Json(SuccessEnvelope::new(
        RecoveryCodesView {
            recovery_codes: codes,
        },
        &id,
    ))
    .into_response())
}

// ------------------------------------------------------------------ kebijakan tenant

#[derive(Debug, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct TenantSecurity {
    /// AUTH-012: Owner/Manager/Admin wajib MFA untuk mengakses tenant ini.
    pub require_admin_mfa: bool,
}

/// Kebijakan keamanan tenant. Butuh `tenant.read`.
#[utoipa::path(
    get, path = "/api/v1/tenants/{tenant_id}/security", tag = "tenants",
    params(("tenant_id" = Uuid, Path, description = "Selector tenant; diverifikasi terhadap membership")),
    responses(
        (status = 200, description = "Kebijakan", body = SuccessEnvelope<TenantSecurity>),
        (status = 403, description = "Permission kurang / MFA diwajibkan", body = umkm_shared::http::ErrorEnvelope),
        (status = 404, description = "Bukan anggota", body = umkm_shared::http::ErrorEnvelope)
    )
)]
pub async fn get_security(
    State(state): State<AppState>,
    ReqId(id): ReqId,
    Member(actor): Member,
) -> Result<Json<SuccessEnvelope<TenantSecurity>>, ApiError> {
    actor.require("tenant.read")?;
    let svc = service(&state)?;
    let v = rbac_repo::get_require_admin_mfa(&svc.pool, &actor)
        .await
        .map_err(|e| ApiError::from(MfaError::Db(e)))?;
    Ok(Json(SuccessEnvelope::new(
        TenantSecurity {
            require_admin_mfa: v,
        },
        &id,
    )))
}

/// Ubah kebijakan. Butuh `tenant.update`; mewajibkan MFA hanya dari sesi yang sudah lolos MFA
/// (pengubah tidak mengunci dirinya sendiri).
#[utoipa::path(
    put, path = "/api/v1/tenants/{tenant_id}/security", tag = "tenants",
    params(("tenant_id" = Uuid, Path, description = "Selector tenant; diverifikasi terhadap membership")),
    request_body = TenantSecurity,
    responses(
        (status = 200, description = "Kebijakan tersimpan", body = SuccessEnvelope<TenantSecurity>),
        (status = 403, description = "Permission kurang / CSRF", body = umkm_shared::http::ErrorEnvelope),
        (status = 404, description = "Bukan anggota", body = umkm_shared::http::ErrorEnvelope),
        (status = 409, description = "Aktifkan MFA akun Anda dulu", body = umkm_shared::http::ErrorEnvelope)
    )
)]
pub async fn put_security(
    State(state): State<AppState>,
    ReqId(id): ReqId,
    ClientIp(ip): ClientIp,
    Member(actor): Member,
    user: SessionUser,
    Json(req): Json<TenantSecurity>,
) -> Result<Json<SuccessEnvelope<TenantSecurity>>, ApiError> {
    actor.require("tenant.update")?;
    if req.require_admin_mfa && !user.mfa_verified {
        return Err(ApiError(
            ErrorBody::new(ErrorCode::Conflict)
                .with_message(
                    "Aktifkan MFA di akun Anda dan masuk dengan MFA sebelum mewajibkannya",
                )
                .with_details(serde_json::json!({ "reason": "mfa_required" })),
        ));
    }
    let svc = service(&state)?;
    rbac_repo::set_require_admin_mfa(
        &svc.pool,
        &actor,
        req.require_admin_mfa,
        Some(id.as_str()),
        Some(&ip),
    )
    .await
    .map_err(|e| ApiError::from(MfaError::Db(e)))?;
    Ok(Json(SuccessEnvelope::new(req, &id)))
}

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/api/v1/auth/login/mfa", post(login_mfa))
        .route("/api/v1/auth/mfa", get(mfa_status))
        .route("/api/v1/auth/mfa/setup", post(mfa_setup))
        .route("/api/v1/auth/mfa/enable", post(mfa_enable))
        .route("/api/v1/auth/mfa/disable", post(mfa_disable))
        .route("/api/v1/auth/mfa/recovery-codes", post(mfa_recovery_codes))
        .route(
            "/api/v1/tenants/{tenant_id}/security",
            get(get_security).put(put_security),
        )
}
