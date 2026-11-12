//! Akun customer di situs toko (P3-3, ADR-0005). Tenant dari Host (ADR-0014); cookie
//! host-only `__Host-umkm_cust` (dev: `umkm_cust`) sehingga toko lain tidak menerimanya.
//! Mutasi wajib Origin == situs toko itu sendiri + `X-CSRF-Token` terikat sesi.

use axum::{
    Json, Router,
    extract::{FromRequestParts, State},
    http::{HeaderMap, HeaderValue, Method, header, request::Parts},
    response::{IntoResponse, Response},
    routing::{get, post},
};
use serde::{Deserialize, Serialize};
use umkm_domain::auth::{AuthError, Email, PasswordHasher as _, rate_rules, validate_password};
use umkm_infrastructure::{
    auth::{hash_token, new_token},
    customer_auth_repo::{self as repo, CustomerAuthError},
};
use umkm_shared::{
    http::{ErrorBody, ErrorCode, SuccessEnvelope},
    ids::TenantId,
};
use utoipa::ToSchema;
use uuid::Uuid;

use crate::{
    ApiError, AppState,
    auth::{AuthService, CSRF_HEADER, ClientIp, Reject, constant_time_eq, csrf_token},
    http_util::internal,
    middleware::{HostTenant, ReqId},
};

fn svc(state: &AppState) -> Result<&AuthService, ApiError> {
    state
        .auth
        .as_deref()
        .ok_or_else(|| ApiError::new(ErrorCode::ServiceUnavailable))
}

fn cookie_name(s: &AuthService) -> &'static str {
    if s.secure_cookies {
        "__Host-umkm_cust"
    } else {
        "umkm_cust"
    }
}

fn set_cookie(s: &AuthService, token: &str) -> HeaderValue {
    let secure = if s.secure_cookies { "; Secure" } else { "" };
    HeaderValue::from_str(&format!(
        "{}={token}; Path=/; HttpOnly; SameSite=Lax; Max-Age={}{secure}",
        cookie_name(s),
        s.policy.session_ttl.as_secs()
    ))
    .expect("ascii")
}

fn clear_cookie(s: &AuthService) -> HeaderValue {
    let secure = if s.secure_cookies { "; Secure" } else { "" };
    HeaderValue::from_str(&format!(
        "{}=; Path=/; HttpOnly; SameSite=Lax; Max-Age=0{secure}",
        cookie_name(s)
    ))
    .expect("ascii")
}

fn read_cookie<'a>(s: &AuthService, h: &'a HeaderMap) -> Option<&'a str> {
    let name = cookie_name(s);
    h.get_all(header::COOKIE)
        .iter()
        .filter_map(|v| v.to_str().ok())
        .flat_map(|v| v.split(';'))
        .filter_map(|kv| kv.trim().split_once('='))
        .find(|(k, _)| *k == name)
        .map(|(_, v)| v)
}

fn forbidden(msg: &str) -> ApiError {
    ApiError(ErrorBody::new(ErrorCode::Forbidden).with_message(msg))
}

/// Origin wajib situs toko itu sendiri (bukan dashboard, bukan toko lain). Tanpa Origin =
/// klien non-browser (tidak dapat menjadi korban CSRF).
fn check_origin(s: &AuthService, h: &HeaderMap) -> Result<(), ApiError> {
    let Some(origin) = h.get(header::ORIGIN).and_then(|o| o.to_str().ok()) else {
        return Ok(());
    };
    let host = h
        .get(header::HOST)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    let ok = origin == format!("https://{host}")
        || (!s.secure_cookies && origin == format!("http://{host}"));
    if ok {
        Ok(())
    } else {
        Err(forbidden("Origin not allowed"))
    }
}

/// Sesi customer valid di toko dari Host.
#[derive(Debug, Clone)]
pub struct CustomerAuth {
    pub tenant: TenantId,
    pub session: repo::CustomerSession,
}

impl FromRequestParts<AppState> for CustomerAuth {
    type Rejection = ApiError;

    async fn from_request_parts(parts: &mut Parts, state: &AppState) -> Result<Self, ApiError> {
        let HostTenant(t) = HostTenant::from_request_parts(parts, state).await?;
        let s = svc(state)?;
        let token = read_cookie(s, &parts.headers)
            .ok_or_else(|| ApiError::new(ErrorCode::Unauthorized))?
            .to_owned();
        if !matches!(parts.method, Method::GET | Method::HEAD | Method::OPTIONS) {
            check_origin(s, &parts.headers)?;
            let sent = parts
                .headers
                .get(CSRF_HEADER)
                .and_then(|v| v.to_str().ok())
                .unwrap_or("");
            if !constant_time_eq(sent.as_bytes(), csrf_token(&token).as_bytes()) {
                return Err(forbidden("CSRF token invalid"));
            }
        }
        let session = repo::authenticate(&s.pool, t, hash_token(&token))
            .await
            .map_err(internal)?
            .ok_or_else(|| ApiError::new(ErrorCode::Unauthorized))?;
        Ok(Self { tenant: t, session })
    }
}

/// Opsional: `None` bila tidak ada cookie (dipakai checkout guest).
pub struct MaybeCustomer(pub Option<CustomerAuth>);

impl FromRequestParts<AppState> for MaybeCustomer {
    type Rejection = ApiError;

    async fn from_request_parts(parts: &mut Parts, state: &AppState) -> Result<Self, ApiError> {
        let has = svc(state)
            .ok()
            .and_then(|s| read_cookie(s, &parts.headers))
            .is_some();
        if !has {
            return Ok(Self(None));
        }
        CustomerAuth::from_request_parts(parts, state)
            .await
            .map(|c| Self(Some(c)))
    }
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct CustomerRegister {
    pub email: String,
    pub password: String,
    /// D-12: wajib `true`
    pub accept_privacy: bool,
    /// D-12: wajib `true`
    pub accept_terms: bool,
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct CustomerLogin {
    pub email: String,
    pub password: String,
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct CustomerVerify {
    pub token: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct CustomerSessionView {
    pub account_id: Uuid,
    pub email: String,
    pub email_verified: bool,
    pub csrf_token: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct CustomerMe {
    pub account_id: Uuid,
    pub email: String,
    pub email_verified: bool,
    /// Customer yang tertaut (riwayat order hanya dari sini; kosong sebelum verifikasi)
    pub customer_ids: Vec<Uuid>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct Registered {
    pub account_id: Uuid,
    /// Email verifikasi dikirim lewat outbox (D-08: adapter log-only)
    pub verification_sent: bool,
}

async fn enforce(
    s: &AuthService,
    rule: &umkm_domain::auth::RateRule,
    subject: &str,
) -> Result<(), ApiError> {
    match s.limiter.check(rule, subject).await {
        Ok(d) if d.allowed => Ok(()),
        Ok(d) => Err(ApiError(
            ErrorBody::new(ErrorCode::RateLimited)
                .with_details(serde_json::json!({ "retry_after_secs": d.retry_after_secs })),
        )),
        Err(e) => {
            tracing::error!(error = %e, "rate limiter unavailable; failing closed");
            Err(ApiError::new(ErrorCode::ServiceUnavailable))
        }
    }
}

fn invalid_credentials() -> ApiError {
    ApiError(
        ErrorBody::new(ErrorCode::Unauthorized)
            .with_message(AuthError::InvalidCredentials.to_string()),
    )
}

fn validation(path: &str, code: &str) -> ApiError {
    crate::validation::one(path, code)
}

/// Daftar akun customer di toko ini (opsional; checkout tetap bisa sebagai guest).
#[utoipa::path(
    post, path = "/api/v1/public/customer/register", tag = "public", request_body = CustomerRegister,
    responses(
        (status = 201, description = "Akun dibuat; email verifikasi diantrekan", body = SuccessEnvelope<Registered>),
        (status = 409, description = "Email sudah terdaftar di toko ini", body = umkm_shared::http::ErrorEnvelope),
        (status = 422, description = "Validasi gagal", body = umkm_shared::http::ErrorEnvelope)
    )
)]
pub async fn register(
    State(state): State<AppState>,
    ReqId(id): ReqId,
    ClientIp(ip): ClientIp,
    HostTenant(t): HostTenant,
    headers: HeaderMap,
    Json(req): Json<CustomerRegister>,
) -> Result<Response, ApiError> {
    let s = svc(&state)?;
    check_origin(s, &headers)?;
    enforce(s, &rate_rules::REGISTER_PER_IP, &format!("cust:{ip}")).await?;
    let email = Email::parse(&req.email).map_err(|_| validation("email", "invalid_email"))?;
    validate_password(&req.password, &email)
        .map_err(|_| validation("password", "weak_password"))?;
    if !req.accept_privacy {
        return Err(validation("accept_privacy", "required"));
    }
    if !req.accept_terms {
        return Err(validation("accept_terms", "required"));
    }
    let hasher = s.hasher.clone();
    let pw = req.password;
    let hash = tokio::task::spawn_blocking(move || hasher.hash(&pw))
        .await
        .map_err(internal)?
        .map_err(|_| ApiError::new(ErrorCode::InternalError))?;
    let account = repo::register(
        &s.pool,
        t,
        &email.canonical(),
        &hash,
        (&s.legal.privacy_policy, &s.legal.terms_of_service),
    )
    .await
    .map_err(|e| match e {
        CustomerAuthError::EmailTaken => {
            crate::http_util::conflict("email_taken", "Email sudah terdaftar di toko ini")
        }
        CustomerAuthError::Db(e) => internal(e),
    })?;
    let body = SuccessEnvelope::new(
        Registered {
            account_id: account,
            verification_sent: true,
        },
        &id,
    );
    Ok((axum::http::StatusCode::CREATED, Json(body)).into_response())
}

/// Login customer (per toko). Pesan error sama untuk email tidak ada / password salah.
#[utoipa::path(
    post, path = "/api/v1/public/customer/login", tag = "public", request_body = CustomerLogin,
    responses(
        (status = 200, description = "Sesi dibuat (cookie)", body = SuccessEnvelope<CustomerSessionView>),
        (status = 401, description = "Email/password salah", body = umkm_shared::http::ErrorEnvelope),
        (status = 429, description = "Terlalu banyak percobaan", body = umkm_shared::http::ErrorEnvelope)
    )
)]
pub async fn login(
    State(state): State<AppState>,
    ReqId(id): ReqId,
    ClientIp(ip): ClientIp,
    HostTenant(t): HostTenant,
    headers: HeaderMap,
    Json(req): Json<CustomerLogin>,
) -> Result<Response, Reject> {
    let s = svc(&state)?;
    check_origin(s, &headers)?;
    enforce(s, &rate_rules::LOGIN_PER_IP, &format!("cust:{ip}")).await?;
    let Ok(email) = Email::parse(&req.email) else {
        return Err(invalid_credentials().into());
    };
    let subject = format!("cust:{t}:{}", email.canonical());
    enforce(s, &rate_rules::LOGIN_PER_EMAIL, &subject).await?;
    let record = repo::find_login(&s.pool, t, &email.canonical())
        .await
        .map_err(internal)?;
    let hasher = s.hasher.clone();
    let pw = req.password;
    let rec = record.clone();
    let ok = tokio::task::spawn_blocking(move || match &rec {
        Some(r) => hasher.verify(&pw, &r.password_hash).unwrap_or(false),
        None => {
            hasher.verify_dummy(&pw);
            false
        }
    })
    .await
    .map_err(internal)?;
    let (true, Some(r)) = (ok, record) else {
        return Err(invalid_credentials().into());
    };
    let _ = s
        .limiter
        .reset(&rate_rules::LOGIN_PER_EMAIL, &subject)
        .await;
    let (token, hash) = new_token().map_err(|_| ApiError::new(ErrorCode::InternalError))?;
    repo::create_session(&s.pool, t, r.account_id, hash, s.policy.session_ttl)
        .await
        .map_err(internal)?;
    let sess = repo::authenticate(&s.pool, t, hash_token(&token))
        .await
        .map_err(internal)?
        .ok_or_else(|| ApiError::new(ErrorCode::InternalError))?;
    let body = SuccessEnvelope::new(
        CustomerSessionView {
            account_id: sess.account_id,
            email: sess.email,
            email_verified: sess.verified,
            csrf_token: csrf_token(&token),
        },
        &id,
    );
    Ok(([(header::SET_COOKIE, set_cookie(s, &token))], Json(body)).into_response())
}

/// Akun yang sedang login + customer tertaut.
#[utoipa::path(
    get, path = "/api/v1/public/customer/me", tag = "public",
    responses(
        (status = 200, description = "Akun", body = SuccessEnvelope<CustomerMe>),
        (status = 401, description = "Belum login", body = umkm_shared::http::ErrorEnvelope)
    )
)]
pub async fn me(
    State(state): State<AppState>,
    ReqId(id): ReqId,
    c: CustomerAuth,
) -> Result<Response, ApiError> {
    let s = svc(&state)?;
    let ids = repo::customer_ids(&s.pool, c.tenant, c.session.account_id)
        .await
        .map_err(internal)?;
    let mut res = Json(SuccessEnvelope::new(
        CustomerMe {
            account_id: c.session.account_id,
            email: c.session.email,
            email_verified: c.session.verified,
            customer_ids: ids,
        },
        &id,
    ))
    .into_response();
    res.headers_mut().insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static("private, no-store"),
    );
    Ok(res)
}

/// Ambil ulang `csrf_token` (reload halaman).
#[utoipa::path(
    get, path = "/api/v1/public/customer/session", tag = "public",
    responses((status = 200, description = "Sesi", body = SuccessEnvelope<CustomerSessionView>))
)]
pub async fn session(
    State(state): State<AppState>,
    ReqId(id): ReqId,
    headers: HeaderMap,
    c: CustomerAuth,
) -> Result<Json<SuccessEnvelope<CustomerSessionView>>, ApiError> {
    let s = svc(&state)?;
    let token = read_cookie(s, &headers).unwrap_or_default();
    Ok(Json(SuccessEnvelope::new(
        CustomerSessionView {
            account_id: c.session.account_id,
            email: c.session.email,
            email_verified: c.session.verified,
            csrf_token: csrf_token(token),
        },
        &id,
    )))
}

/// Logout (cabut sesi + hapus cookie).
#[utoipa::path(
    post, path = "/api/v1/public/customer/logout", tag = "public",
    responses((status = 204, description = "Keluar"))
)]
pub async fn logout(State(state): State<AppState>, c: CustomerAuth) -> Result<Response, ApiError> {
    let s = svc(&state)?;
    repo::revoke(&s.pool, c.tenant, c.session.session_id)
        .await
        .map_err(internal)?;
    Ok((
        axum::http::StatusCode::NO_CONTENT,
        [(header::SET_COOKIE, clear_cookie(s))],
    )
        .into_response())
}

/// Verifikasi email lalu tautkan customer guest beremail sama di toko ini (AC-CRM-005).
#[utoipa::path(
    post, path = "/api/v1/public/customer/verify-email", tag = "public", request_body = CustomerVerify,
    responses(
        (status = 200, description = "Terverifikasi", body = SuccessEnvelope<serde_json::Value>),
        (status = 400, description = "Token tidak valid/kedaluwarsa", body = umkm_shared::http::ErrorEnvelope)
    )
)]
pub async fn verify_email(
    State(state): State<AppState>,
    ReqId(id): ReqId,
    ClientIp(ip): ClientIp,
    HostTenant(t): HostTenant,
    headers: HeaderMap,
    Json(req): Json<CustomerVerify>,
) -> Result<Json<SuccessEnvelope<serde_json::Value>>, ApiError> {
    let s = svc(&state)?;
    check_origin(s, &headers)?;
    enforce(s, &rate_rules::VERIFY_TOKEN_PER_IP, &format!("cust:{ip}")).await?;
    let token = req.token.trim();
    if token.is_empty() || token.len() > 128 {
        return Err(ApiError(
            ErrorBody::new(ErrorCode::BadRequest).with_message("Token tidak valid"),
        ));
    }
    let r = repo::verify_email(&s.pool, t, hash_token(token))
        .await
        .map_err(internal)?
        .ok_or_else(|| {
            ApiError(
                ErrorBody::new(ErrorCode::BadRequest)
                    .with_message("Token tidak valid atau kedaluwarsa"),
            )
        })?;
    Ok(Json(SuccessEnvelope::new(
        serde_json::json!({ "account_id": r.0, "linked_customers": r.1 }),
        &id,
    )))
}

pub fn routes() -> Router<AppState> {
    let p = "/api/v1/public/customer";
    Router::new()
        .route(&format!("{p}/register"), post(register))
        .route(&format!("{p}/login"), post(login))
        .route(&format!("{p}/logout"), post(logout))
        .route(&format!("{p}/me"), get(me))
        .route(&format!("{p}/session"), get(session))
        .route(&format!("{p}/verify-email"), post(verify_email))
}
