//! Endpoint autentikasi (P1-3) — `POST /api/v1/auth/{register,login,logout}`,
//! `GET /api/v1/auth/session`. Transport sesi: ADR-0015 (cookie HttpOnly host-only + CSRF).

use std::{net::SocketAddr, sync::Arc};

use axum::{
    Json, Router,
    extract::{ConnectInfo, FromRequestParts, State},
    http::{HeaderMap, HeaderValue, StatusCode, header, request::Parts},
    response::{IntoResponse, Response},
    routing::{get, post},
};
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use sqlx::PgPool;
use umkm_application::onboarding::{LegalVersions, RegisterCommand, RegisterError, register};
use umkm_domain::{
    audit::{AuditEvent, actions},
    auth::{AuthError, AuthPolicy, Email, PasswordHasher, RateRule, rate_rules},
    tenant::TenantError,
};
use umkm_infrastructure::{
    audit_repo,
    auth::{Argon2idHasher, hash_token, new_token},
    auth_repo,
    onboarding_repo::{BlockingArgon2, OsTokenIssuer, PgRegistrationStore},
    rate_limit::RateLimiter,
};
use umkm_shared::http::{ErrorBody, ErrorCode, SuccessEnvelope};
use utoipa::ToSchema;
use uuid::Uuid;

use crate::{ApiError, AppState, middleware::ReqId};

pub const CSRF_HEADER: &str = "x-csrf-token";

/// Dependensi autentikasi. `None` di `AppState` = route auth tidak dipasang.
pub struct AuthService {
    pub pool: PgPool,
    pub limiter: RateLimiter,
    pub hasher: Arc<Argon2idHasher>,
    pub policy: AuthPolicy,
    pub legal: LegalVersions,
    pub root_domain: String,
    /// ADR-0015: `false` hanya untuk environment development (HTTP lokal).
    pub secure_cookies: bool,
    pub allowed_origins: Vec<String>,
    pub trust_forwarded_for: bool,
    /// Master key envelope (ADR-0011) untuk rahasia MFA. `None` -> endpoint MFA 503.
    pub keyring: Option<umkm_infrastructure::crypto::Keyring>,
}

impl AuthService {
    pub fn cookie_name(&self) -> &'static str {
        if self.secure_cookies {
            "__Host-umkm_session"
        } else {
            "umkm_session"
        }
    }

    pub(crate) fn set_cookie(&self, token: &str) -> HeaderValue {
        let max_age = self.policy.session_ttl.as_secs();
        let secure = if self.secure_cookies { "; Secure" } else { "" };
        HeaderValue::from_str(&format!(
            "{}={token}; Path=/; HttpOnly; SameSite=Lax; Max-Age={max_age}{secure}",
            self.cookie_name()
        ))
        .expect("token base64url ascii")
    }

    fn clear_cookie(&self) -> HeaderValue {
        let secure = if self.secure_cookies { "; Secure" } else { "" };
        HeaderValue::from_str(&format!(
            "{}=; Path=/; HttpOnly; SameSite=Lax; Max-Age=0{secure}",
            self.cookie_name()
        ))
        .expect("ascii")
    }

    pub fn session_cookie<'a>(&self, headers: &'a HeaderMap) -> Option<&'a str> {
        let name = self.cookie_name();
        headers
            .get_all(header::COOKIE)
            .iter()
            .filter_map(|v| v.to_str().ok())
            .flat_map(|v| v.split(';'))
            .filter_map(|kv| kv.trim().split_once('='))
            .find(|(k, _)| *k == name)
            .map(|(_, v)| v)
    }

    /// `PRD.md §6.1` CSRF + ADR-0015: request lintas-origin yang tidak di-allowlist ditolak.
    /// Browser selalu mengirim `Origin` pada POST; klien tanpa `Origin` (non-browser)
    /// tidak dapat menjadi korban CSRF.
    pub fn check_origin(&self, headers: &HeaderMap) -> Result<(), ApiError> {
        match headers.get(header::ORIGIN).and_then(|o| o.to_str().ok()) {
            None => Ok(()),
            Some(o) if self.allowed_origins.iter().any(|a| a == o) => Ok(()),
            Some(_) => Err(ApiError(
                ErrorBody::new(ErrorCode::Forbidden).with_message("Origin not allowed"),
            )),
        }
    }
}

/// Token CSRF terikat sesi (synchronizer): turunan SHA-256 dari token sesi.
/// Tidak dapat dihitung tanpa token sesi, yang HttpOnly.
pub fn csrf_token(session_token: &str) -> String {
    URL_SAFE_NO_PAD.encode(Sha256::digest(format!("csrf:{session_token}").as_bytes()))
}

pub fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    a.len() == b.len() && a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

/// IP klien: dari `X-Forwarded-For` (hanya bila di belakang Caddy) atau koneksi TCP.
pub struct ClientIp(pub String);

impl FromRequestParts<AppState> for ClientIp {
    type Rejection = std::convert::Infallible;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let trust = state.auth.as_ref().is_some_and(|a| a.trust_forwarded_for);
        if trust
            && let Some(ip) = parts
                .headers
                .get("x-forwarded-for")
                .and_then(|v| v.to_str().ok())
                .and_then(|v| v.split(',').next())
                .map(str::trim)
                .filter(|s| s.parse::<std::net::IpAddr>().is_ok())
        {
            return Ok(Self(ip.to_owned()));
        }
        let ip = parts
            .extensions
            .get::<ConnectInfo<SocketAddr>>()
            .map(|c| c.0.ip().to_string())
            // Tanpa info koneksi semua klien berbagi satu bucket (lebih ketat, bukan lebih longgar).
            .unwrap_or_else(|| "unknown".to_owned());
        Ok(Self(ip))
    }
}

/// Error handler yang di-box (Response besar; clippy::result_large_err).
pub struct Reject(Box<Response>);

impl IntoResponse for Reject {
    fn into_response(self) -> Response {
        *self.0
    }
}

impl From<ApiError> for Reject {
    fn from(e: ApiError) -> Self {
        Self(Box::new(e.into_response()))
    }
}

impl From<Response> for Reject {
    fn from(r: Response) -> Self {
        Self(Box::new(r))
    }
}

pub(crate) fn service(state: &AppState) -> Result<&Arc<AuthService>, ApiError> {
    state
        .auth
        .as_ref()
        .ok_or_else(|| ApiError::new(ErrorCode::ServiceUnavailable))
}

/// Rate limit fail-closed: Redis bermasalah -> 503, bukan dilewati.
pub(crate) async fn enforce(
    svc: &AuthService,
    rule: &RateRule,
    subject: &str,
) -> Result<(), Reject> {
    match svc.limiter.check(rule, subject).await {
        Ok(d) if d.allowed => Ok(()),
        Ok(d) => {
            let mut res = ApiError(
                ErrorBody::new(ErrorCode::RateLimited)
                    .with_details(serde_json::json!({ "retry_after_secs": d.retry_after_secs })),
            )
            .into_response();
            res.headers_mut()
                .insert(header::RETRY_AFTER, HeaderValue::from(d.retry_after_secs));
            Err(res.into())
        }
        Err(e) => {
            tracing::error!(error = %e, rule = rule.name, "rate limiter unavailable; failing closed");
            Err(ApiError::new(ErrorCode::ServiceUnavailable).into())
        }
    }
}

pub(crate) fn validation(field: &str, message: impl Into<String>) -> ApiError {
    ApiError(
        ErrorBody::new(ErrorCode::ValidationFailed)
            .with_message(message)
            .with_details(serde_json::json!({ "field": field })),
    )
}

impl From<RegisterError> for ApiError {
    fn from(e: RegisterError) -> Self {
        match e {
            RegisterError::ConsentRequired => validation("accept_terms", e.to_string()),
            RegisterError::Auth(AuthError::InvalidEmail) => validation("email", e.to_string()),
            RegisterError::Auth(AuthError::WeakPassword(_)) => {
                validation("password", e.to_string())
            }
            RegisterError::Tenant(TenantError::InvalidName) => {
                validation("business_name", e.to_string())
            }
            RegisterError::Tenant(TenantError::InvalidSlug) => validation("slug", e.to_string()),
            RegisterError::Tenant(TenantError::SlugUnavailable) => ApiError(
                ErrorBody::new(ErrorCode::Conflict)
                    .with_message(e.to_string())
                    .with_details(serde_json::json!({ "field": "slug" })),
            ),
            RegisterError::EmailTaken => ApiError(
                ErrorBody::new(ErrorCode::Conflict)
                    .with_message(e.to_string())
                    .with_details(serde_json::json!({ "field": "email" })),
            ),
            // InvalidHost tidak terjadi pada registrasi; tetap dipetakan eksplisit.
            RegisterError::Auth(_)
            | RegisterError::Tenant(TenantError::InvalidHost)
            | RegisterError::Internal => ApiError::new(ErrorCode::InternalError),
        }
    }
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct RegisterRequest {
    pub email: String,
    pub password: String,
    pub business_name: String,
    /// Kosong = dibuat otomatis dari nama bisnis.
    pub slug: Option<String>,
    pub accept_terms: bool,
    pub accept_privacy: bool,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct TenantSummary {
    pub id: Uuid,
    pub slug: String,
    pub hostname: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct RegisterResponse {
    pub user_id: Uuid,
    pub tenant: TenantSummary,
    /// Kirim balik di header `X-CSRF-Token` untuk request yang mengubah state.
    pub csrf_token: String,
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct LoginRequest {
    pub email: String,
    pub password: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct SessionResponse {
    pub user_id: Uuid,
    pub session_id: Uuid,
    pub csrf_token: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct MfaChallengeResponse {
    pub mfa_required: bool,
    /// Token tantangan sekali pakai (5 menit, 5 percobaan); bukan sesi.
    pub mfa_token: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct SessionStateResponse {
    pub user_id: Uuid,
    pub session_id: Uuid,
    pub csrf_token: String,
    /// Sesi lolos langkah kedua MFA.
    pub mfa_verified: bool,
    /// AUTH-011: akun wajib MFA dan sesi belum lolos → hanya halaman Keamanan yang bisa dipakai.
    pub mfa_enrollment_required: bool,
    /// Akun memegang peran platform (mis. super admin). Dashboard hanya memeriksa
    /// `/platform/me` bila true — sesi merchant biasa tidak lagi memicu 403 (UAT 15 Nov).
    pub platform_staff: bool,
}

pub(crate) fn user_agent(headers: &HeaderMap) -> Option<String> {
    headers
        .get(header::USER_AGENT)
        .and_then(|v| v.to_str().ok())
        .map(str::to_owned)
}

/// Registrasi bisnis: user + tenant + Owner + subdomain + sesi (`PRD.md §13.3`).
#[utoipa::path(
    post, path = "/api/v1/auth/register", tag = "auth",
    request_body = RegisterRequest,
    responses(
        (status = 201, description = "Terdaftar; cookie sesi di-set", body = SuccessEnvelope<RegisterResponse>),
        (status = 403, description = "Origin tidak diizinkan", body = umkm_shared::http::ErrorEnvelope),
        (status = 409, description = "Email atau slug sudah dipakai/dicadangkan", body = umkm_shared::http::ErrorEnvelope),
        (status = 422, description = "Validasi gagal; `details.field`", body = umkm_shared::http::ErrorEnvelope),
        (status = 429, description = "Rate limit; header Retry-After", body = umkm_shared::http::ErrorEnvelope),
        (status = 503, description = "Rate limiter/DB tidak tersedia (fail-closed)", body = umkm_shared::http::ErrorEnvelope)
    )
)]
pub async fn register_handler(
    State(state): State<AppState>,
    ReqId(id): ReqId,
    ClientIp(ip): ClientIp,
    headers: HeaderMap,
    Json(req): Json<RegisterRequest>,
) -> Result<Response, Reject> {
    let svc = service(&state).map_err(Reject::from)?;
    svc.check_origin(&headers).map_err(Reject::from)?;
    enforce(svc, &rate_rules::REGISTER_PER_IP, &ip).await?;

    let outcome = register(
        &PgRegistrationStore::new(svc.pool.clone()),
        &BlockingArgon2(svc.hasher.clone()),
        &OsTokenIssuer,
        RegisterCommand {
            email: req.email,
            password: req.password,
            business_name: req.business_name,
            slug: req.slug,
            accept_terms: req.accept_terms,
            accept_privacy: req.accept_privacy,
            user_agent: user_agent(&headers),
            request_id: Some(id.to_string()),
            client_ip: Some(ip.clone()),
        },
        &svc.root_domain,
        &svc.policy,
        &svc.legal,
    )
    .await
    .map_err(|e| Reject::from(ApiError::from(e)))?;

    let r = outcome.registered;
    tracing::info!(user_id = %r.user_id, tenant_id = %r.tenant_id, slug = %r.slug, "tenant registered");
    let body = SuccessEnvelope::new(
        RegisterResponse {
            user_id: r.user_id,
            tenant: TenantSummary {
                id: *r.tenant_id.as_uuid(),
                slug: r.slug.to_string(),
                hostname: r.hostname,
            },
            csrf_token: csrf_token(&outcome.session_token),
        },
        &id,
    );
    Ok((
        StatusCode::CREATED,
        [(header::SET_COOKIE, svc.set_cookie(&outcome.session_token))],
        Json(body),
    )
        .into_response())
}

/// Login email + password. Pesan gagal seragam (anti user-enumeration).
#[utoipa::path(
    post, path = "/api/v1/auth/login", tag = "auth",
    request_body = LoginRequest,
    responses(
        (status = 200, description = "Sesi baru (rotasi); cookie di-set", body = SuccessEnvelope<SessionResponse>),
        (status = 202, description = "Password benar, MFA aktif: lanjutkan ke `/auth/login/mfa` (tanpa cookie)", body = SuccessEnvelope<MfaChallengeResponse>),
        (status = 401, description = "Email atau password salah", body = umkm_shared::http::ErrorEnvelope),
        (status = 403, description = "Origin tidak diizinkan", body = umkm_shared::http::ErrorEnvelope),
        (status = 429, description = "Rate limit; header Retry-After", body = umkm_shared::http::ErrorEnvelope),
        (status = 503, description = "Rate limiter/DB tidak tersedia (fail-closed)", body = umkm_shared::http::ErrorEnvelope)
    )
)]
pub async fn login_handler(
    State(state): State<AppState>,
    ReqId(id): ReqId,
    ClientIp(ip): ClientIp,
    headers: HeaderMap,
    Json(req): Json<LoginRequest>,
) -> Result<Response, Reject> {
    let svc = service(&state).map_err(Reject::from)?;
    svc.check_origin(&headers).map_err(Reject::from)?;
    enforce(svc, &rate_rules::LOGIN_PER_IP, &ip).await?;
    let invalid = || {
        Reject::from(ApiError(
            ErrorBody::new(ErrorCode::Unauthorized)
                .with_message(AuthError::InvalidCredentials.to_string()),
        ))
    };
    let Ok(email) = Email::parse(&req.email) else {
        return Err(invalid());
    };
    enforce(svc, &rate_rules::LOGIN_PER_EMAIL, &email.canonical()).await?;

    let internal = |_| Reject::from(ApiError::new(ErrorCode::InternalError));
    let record = auth_repo::find_login(&svc.pool, &email)
        .await
        .map_err(internal)?;
    let hasher = svc.hasher.clone();
    let password = req.password;
    let verified = tokio::task::spawn_blocking(move || match &record {
        Some(r) if r.active => hasher.verify(&password, &r.password_hash).map(|ok| {
            (
                ok,
                record.clone(),
                hasher
                    .needs_rehash(&r.password_hash)
                    .then(|| hasher.hash(&password)),
            )
        }),
        _ => {
            // Biaya waktu sama dengan akun yang ada.
            hasher.verify_dummy(&password);
            Ok((false, None, None))
        }
    })
    .await
    .map_err(|_| Reject::from(ApiError::new(ErrorCode::InternalError)))?
    .map_err(|_| Reject::from(ApiError::new(ErrorCode::InternalError)))?;

    let (true, Some(record), rehash) = verified else {
        tracing::info!("login failed");
        audit_repo::record_best_effort(
            &svc.pool,
            &AuditEvent::new(actions::AUTH_LOGIN_FAILED, "user")
                .request(Some(id.as_str()), Some(&ip))
                // Email di-hash (pseudonim) untuk forensik brute-force tanpa menyimpan alamatnya.
                .with_summary(serde_json::json!({ "email_sha256": umkm_infrastructure::auth::subject_digest(&email.canonical()) })),
        )
        .await;
        return Err(invalid());
    };
    if let Some(Ok(new_hash)) = rehash {
        auth_repo::update_password_hash(&svc.pool, record.user_id, &new_hash)
            .await
            .map_err(internal)?;
    }
    let _ = svc
        .limiter
        .reset(&rate_rules::LOGIN_PER_EMAIL, &email.canonical())
        .await;

    // MFA aktif → belum ada sesi; klien menyelesaikan lewat `/auth/login/mfa`.
    let mfa = umkm_infrastructure::mfa::status(&svc.pool, record.user_id)
        .await
        .map_err(|_| Reject::from(ApiError::new(ErrorCode::InternalError)))?;
    if mfa.enabled {
        let mfa_token = umkm_infrastructure::mfa::create_challenge(&svc.pool, record.user_id)
            .await
            .map_err(|_| Reject::from(ApiError::new(ErrorCode::InternalError)))?;
        tracing::info!(user_id = %record.user_id, "login password ok; MFA challenge issued");
        return Ok((
            StatusCode::ACCEPTED,
            Json(SuccessEnvelope::new(
                MfaChallengeResponse {
                    mfa_required: true,
                    mfa_token,
                },
                &id,
            )),
        )
            .into_response());
    }

    let (token, hash) =
        new_token().map_err(|_| Reject::from(ApiError::new(ErrorCode::InternalError)))?;
    let session_id = auth_repo::create_session(
        &svc.pool,
        record.user_id,
        hash,
        svc.policy.session_ttl,
        user_agent(&headers).as_deref(),
    )
    .await
    .map_err(internal)?;
    tracing::info!(user_id = %record.user_id, "login succeeded");
    audit_repo::record_best_effort(
        &svc.pool,
        &AuditEvent::new(actions::AUTH_LOGIN_SUCCEEDED, "session")
            .by_user(record.user_id)
            .resource(session_id)
            .request(Some(id.as_str()), Some(&ip)),
    )
    .await;
    let body = SuccessEnvelope::new(
        SessionResponse {
            user_id: record.user_id,
            session_id,
            csrf_token: csrf_token(&token),
        },
        &id,
    );
    Ok(([(header::SET_COOKIE, svc.set_cookie(&token))], Json(body)).into_response())
}

/// Sesi aktif dari cookie (untuk reload SPA: mengambil ulang `csrf_token`).
#[utoipa::path(
    get, path = "/api/v1/auth/session", tag = "auth",
    responses(
        (status = 200, description = "Sesi valid", body = SuccessEnvelope<SessionStateResponse>),
        (status = 401, description = "Tidak ada sesi valid", body = umkm_shared::http::ErrorEnvelope)
    )
)]
pub async fn session_handler(
    State(state): State<AppState>,
    ReqId(id): ReqId,
    headers: HeaderMap,
) -> Result<Response, ApiError> {
    let svc = service(&state)?;
    let token = svc
        .session_cookie(&headers)
        .ok_or_else(|| ApiError::new(ErrorCode::Unauthorized))?;
    let s = auth_repo::authenticate_session(&svc.pool, hash_token(token))
        .await
        .map_err(|_| ApiError::new(ErrorCode::InternalError))?
        .ok_or_else(|| ApiError::new(ErrorCode::Unauthorized))?;
    Ok(Json(SuccessEnvelope::new(
        SessionStateResponse {
            user_id: s.user_id,
            session_id: s.session_id,
            csrf_token: csrf_token(token),
            mfa_verified: s.mfa_verified,
            mfa_enrollment_required: s.platform_mfa_required && !s.mfa_verified,
            platform_staff: s.platform_mfa_required,
        },
        &id,
    ))
    .into_response())
}

#[derive(Debug, Serialize, ToSchema)]
pub struct LogoutResponse {
    pub revoked: bool,
}

/// Logout: cabut sesi ini (AUTH-006). Wajib header `X-CSRF-Token`.
#[utoipa::path(
    post, path = "/api/v1/auth/logout", tag = "auth",
    responses(
        (status = 200, description = "Sesi dicabut; cookie dihapus", body = SuccessEnvelope<LogoutResponse>),
        (status = 401, description = "Tidak ada sesi valid", body = umkm_shared::http::ErrorEnvelope),
        (status = 403, description = "CSRF token/Origin tidak valid", body = umkm_shared::http::ErrorEnvelope)
    )
)]
pub async fn logout_handler(
    State(state): State<AppState>,
    ReqId(id): ReqId,
    // Cek cookie + Origin + CSRF dilakukan oleh extractor (session.rs). Logout tetap boleh
    // untuk sesi yang tertahan gerbang MFA platform.
    crate::session::MfaPendingUser { user, .. }: crate::session::MfaPendingUser,
) -> Result<Response, ApiError> {
    let svc = service(&state)?;
    let revoked = auth_repo::revoke_session(&svc.pool, user.user_id, user.session_id)
        .await
        .map_err(|_| ApiError::new(ErrorCode::InternalError))?;
    audit_repo::record_best_effort(
        &svc.pool,
        &AuditEvent::new(actions::AUTH_LOGOUT, "session")
            .by_user(user.user_id)
            .resource(user.session_id)
            .request(Some(id.as_str()), None),
    )
    .await;
    Ok((
        [(header::SET_COOKIE, svc.clear_cookie())],
        Json(SuccessEnvelope::new(LogoutResponse { revoked }, &id)),
    )
        .into_response())
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct VerifyEmailRequest {
    /// Token dari fragment tautan email (`#token=`).
    pub token: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct VerifyEmailResponse {
    pub verified: bool,
}

/// Verifikasi email dengan token sekali pakai (AUTH-003). Tidak butuh sesi.
#[utoipa::path(
    post, path = "/api/v1/auth/verify-email", tag = "auth",
    request_body = VerifyEmailRequest,
    responses(
        (status = 200, description = "Email terverifikasi", body = SuccessEnvelope<VerifyEmailResponse>),
        (status = 403, description = "Origin tidak diizinkan", body = umkm_shared::http::ErrorEnvelope),
        (status = 422, description = "Token tidak valid, kedaluwarsa, atau sudah dipakai", body = umkm_shared::http::ErrorEnvelope),
        (status = 429, description = "Rate limit; header Retry-After", body = umkm_shared::http::ErrorEnvelope),
        (status = 503, description = "Rate limiter/DB tidak tersedia (fail-closed)", body = umkm_shared::http::ErrorEnvelope)
    )
)]
pub async fn verify_email_handler(
    State(state): State<AppState>,
    ReqId(id): ReqId,
    ClientIp(ip): ClientIp,
    headers: HeaderMap,
    Json(req): Json<VerifyEmailRequest>,
) -> Result<Response, Reject> {
    let svc = service(&state).map_err(Reject::from)?;
    svc.check_origin(&headers).map_err(Reject::from)?;
    enforce(svc, &rate_rules::VERIFY_TOKEN_PER_IP, &ip).await?;
    let user = umkm_infrastructure::outbox::verify_email(&svc.pool, hash_token(req.token.trim()))
        .await
        .map_err(|_| Reject::from(ApiError::new(ErrorCode::InternalError)))?;
    match user {
        Some(user_id) => {
            tracing::info!(%user_id, "email verified");
            audit_repo::record_best_effort(
                &svc.pool,
                &AuditEvent::new(actions::AUTH_EMAIL_VERIFIED, "user")
                    .by_user(user_id)
                    .resource(user_id)
                    .request(Some(id.as_str()), Some(&ip)),
            )
            .await;
            Ok(Json(SuccessEnvelope::new(
                VerifyEmailResponse { verified: true },
                &id,
            ))
            .into_response())
        }
        None => Err(Reject::from(validation(
            "token",
            "token tidak valid atau kedaluwarsa",
        ))),
    }
}

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/api/v1/auth/register", post(register_handler))
        .route("/api/v1/auth/login", post(login_handler))
        .route("/api/v1/auth/session", get(session_handler))
        .route("/api/v1/auth/logout", post(logout_handler))
        .route("/api/v1/auth/verify-email", post(verify_email_handler))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn csrf_is_deterministic_and_bound_to_session() {
        assert_eq!(csrf_token("a"), csrf_token("a"));
        assert_ne!(csrf_token("a"), csrf_token("b"));
        assert_eq!(csrf_token("a").len(), 43);
    }

    #[test]
    fn constant_time_eq_works() {
        assert!(constant_time_eq(b"abc", b"abc"));
        assert!(!constant_time_eq(b"abc", b"abd"));
        assert!(!constant_time_eq(b"abc", b"ab"));
    }
}
