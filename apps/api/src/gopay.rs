//! Integrasi GoPay Merchant — endpoint pemilik toko (P10, ADR-0020): hubungkan akun GoBiz via OTP,
//! status, putuskan. Baca: `tenant.read`; ubah: `tenant.update`.
//!
//! OTP: `request-otp` mengembalikan `otp_token` opaque + metadata; klien mengirimnya kembali ke
//! `verify` bersama kode OTP. Token GoBiz hanya hidup sesaat di server lalu disegel envelope.

use std::time::Duration;

use axum::{
    Json, Router,
    extract::State,
    routing::{get, post},
};
use serde::{Deserialize, Serialize};
use umkm_infrastructure::{
    gopay::{GobizClient, GobizError, poller::now_ms},
    gopay_repo,
    http_client::{Egress, EgressPolicy, Resolver},
};
use umkm_shared::http::{ErrorBody, ErrorCode, SuccessEnvelope};
use utoipa::ToSchema;

use crate::{
    ApiError, AppState,
    http_util::{internal, pool},
    middleware::ReqId,
    session::Member,
};

// ------------------------------------------------------------------ DTO

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct StatusView {
    pub connected: bool,
    /// `active` | `needs_relogin`
    pub status: Option<String>,
    pub outlet_name: Option<String>,
    /// Nomor HP ter-mask, mis. `+62••••7890`.
    pub phone_masked: Option<String>,
    pub has_static_qris: bool,
}

#[derive(Debug, Clone, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct RequestOtpDto {
    pub phone: String,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct OtpChallengeView {
    pub otp_token: String,
    pub otp_length: u8,
    pub expires_in_secs: i64,
}

#[derive(Debug, Clone, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct VerifyOtpDto {
    pub phone: String,
    pub otp: String,
    pub otp_token: String,
}

#[derive(Debug, Clone, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct ConnectSessionDto {
    pub phone: String,
    /// Cookie/token sesi GoBiz dari browser pemilik (harus memuat `access_token` & `refresh_token`).
    pub session: String,
}

// ------------------------------------------------------------------ util

fn keyring(state: &AppState) -> Result<&umkm_infrastructure::crypto::Keyring, ApiError> {
    state
        .auth
        .as_ref()
        .and_then(|a| a.keyring.as_ref())
        .ok_or_else(|| ApiError::new(ErrorCode::ServiceUnavailable))
}

fn client() -> GobizClient {
    GobizClient::new(Egress::new(
        EgressPolicy::tenant(Duration::from_secs(15)),
        Resolver::default(),
        2,
    ))
}

fn mask_phone(e164: &str) -> String {
    let n = e164.len();
    if n <= 6 {
        return "+62••••".into();
    }
    format!("+62••••{}", &e164[n - 4..])
}

fn msg(code: ErrorCode, m: &str) -> ApiError {
    ErrorBody::new(code).with_message(m).into()
}

/// Peta error GoBiz → HTTP. `bad_request` dipakai untuk penolakan 4xx GoBiz (pesannya berbeda
/// antara langkah kirim-OTP dan verifikasi, agar "OTP salah" tidak muncul saat baru mengirim).
fn gobiz_err(e: GobizError, bad_request: &str) -> ApiError {
    match e {
        GobizError::BadPhone => msg(ErrorCode::ValidationFailed, "Nomor HP tidak valid"),
        GobizError::Unauthorized | GobizError::Status(_) => msg(ErrorCode::BadRequest, bad_request),
        GobizError::Egress(_) | GobizError::Parse(_) => msg(
            ErrorCode::ServiceUnavailable,
            "Layanan GoPay sedang tidak dapat dihubungi",
        ),
    }
}

/// Pesan 4xx saat meminta OTP (bukan soal kode salah).
const OTP_SEND_FAIL: &str = "Gagal mengirim OTP. Pastikan nomor terdaftar di aplikasi GoPay/GoBiz Merchant, lalu coba lagi.";
/// Pesan 4xx saat verifikasi OTP.
const OTP_VERIFY_FAIL: &str = "Kode OTP salah atau kedaluwarsa.";

fn repo_err(e: gopay_repo::GopayRepoError) -> ApiError {
    use gopay_repo::GopayRepoError as E;
    match e {
        E::NotConnected => msg(ErrorCode::BadRequest, "Akun GoPay belum terhubung"),
        E::BadQris => msg(ErrorCode::ValidationFailed, "QRIS statis tidak valid"),
        E::NoStaticQris | E::NoUniqueCode => {
            msg(ErrorCode::Conflict, "Tidak dapat membuat tagihan GoPay")
        }
        E::Crypto | E::Db(_) => internal(e),
    }
}

// ------------------------------------------------------------------ handlers

/// Status koneksi GoPay toko.
#[utoipa::path(
    get, path = "/api/v1/tenants/{tenant_id}/gopay", tag = "gopay",
    params(("tenant_id" = Uuid, Path)),
    responses((status = 200, description = "Status", body = SuccessEnvelope<StatusView>))
)]
pub async fn get_status(
    State(state): State<AppState>,
    ReqId(id): ReqId,
    Member(actor): Member,
) -> Result<Json<SuccessEnvelope<StatusView>>, ApiError> {
    actor.require("tenant.read")?;
    let qris_ready = crate::commerce::ensure_qris_payload(&state, actor.tenant_id)
        .await?
        .qris_static_payload
        .is_some();
    let view = match gopay_repo::get_session(pool(&state)?, actor.tenant_id, keyring(&state)?)
        .await
        .map_err(repo_err)?
    {
        None => StatusView {
            connected: false,
            status: None,
            outlet_name: None,
            phone_masked: None,
            has_static_qris: false,
        },
        Some(s) => StatusView {
            connected: true,
            status: Some(s.status),
            outlet_name: s.session.outlet_name.clone(),
            phone_masked: Some(mask_phone(&s.session.phone_e164)),
            has_static_qris: qris_ready,
        },
    };
    Ok(Json(SuccessEnvelope::new(view, &id)))
}

/// Minta OTP ke nomor HP GoBiz toko.
#[utoipa::path(
    post, path = "/api/v1/tenants/{tenant_id}/gopay/connect/request-otp", tag = "gopay",
    params(("tenant_id" = Uuid, Path)), request_body = RequestOtpDto,
    responses((status = 200, description = "OTP terkirim", body = SuccessEnvelope<OtpChallengeView>))
)]
pub async fn request_otp(
    State(state): State<AppState>,
    ReqId(id): ReqId,
    Member(actor): Member,
    Json(body): Json<RequestOtpDto>,
) -> Result<Json<SuccessEnvelope<OtpChallengeView>>, ApiError> {
    actor.require("tenant.update")?;
    pool(&state)?; // pastikan DB tersedia walau belum dipakai di langkah ini
    let (local, _) = umkm_infrastructure::gopay::normalize_phone(&body.phone)
        .map_err(|e| gobiz_err(e, OTP_SEND_FAIL))?;
    let ch = client()
        .request_otp(&local)
        .await
        .map_err(|e| gobiz_err(e, OTP_SEND_FAIL))?;
    Ok(Json(SuccessEnvelope::new(
        OtpChallengeView {
            otp_token: ch.otp_token,
            otp_length: ch.otp_length,
            expires_in_secs: ch.expires_in_secs,
        },
        &id,
    )))
}

/// Verifikasi OTP → simpan sesi (akun terhubung).
#[utoipa::path(
    post, path = "/api/v1/tenants/{tenant_id}/gopay/connect/verify", tag = "gopay",
    params(("tenant_id" = Uuid, Path)), request_body = VerifyOtpDto,
    responses((status = 200, description = "Terhubung", body = SuccessEnvelope<StatusView>))
)]
pub async fn verify_otp(
    State(state): State<AppState>,
    ReqId(id): ReqId,
    Member(actor): Member,
    Json(body): Json<VerifyOtpDto>,
) -> Result<Json<SuccessEnvelope<StatusView>>, ApiError> {
    actor.require("tenant.update")?;
    let p = pool(&state)?;
    let (_, e164) = umkm_infrastructure::gopay::normalize_phone(&body.phone)
        .map_err(|e| gobiz_err(e, OTP_VERIFY_FAIL))?;
    let session = client()
        .verify_otp(&e164, body.otp.trim(), &body.otp_token, now_ms())
        .await
        .map_err(|e| gobiz_err(e, OTP_VERIFY_FAIL))?;
    gopay_repo::put_session(p, actor.tenant_id, &session, keyring(&state)?)
        .await
        .map_err(repo_err)?;
    let qris_ready = crate::commerce::ensure_qris_payload(&state, actor.tenant_id)
        .await?
        .qris_static_payload
        .is_some();
    let view = StatusView {
        connected: true,
        status: Some("active".into()),
        outlet_name: session.outlet_name.clone(),
        phone_masked: Some(mask_phone(&session.phone_e164)),
        has_static_qris: qris_ready,
    };
    Ok(Json(SuccessEnvelope::new(view, &id)))
}

/// Hubungkan via sesi browser. Login OTP GoBiz kini terkunci reCAPTCHA (hanya bisa di browser),
/// jadi pemilik login di browser lalu menempel cookie/token sesinya di sini; refresh token & poll
/// transaksi berikutnya berjalan otomatis di server.
#[utoipa::path(
    post, path = "/api/v1/tenants/{tenant_id}/gopay/connect/session", tag = "gopay",
    params(("tenant_id" = Uuid, Path)), request_body = ConnectSessionDto,
    responses(
        (status = 200, description = "Terhubung", body = SuccessEnvelope<StatusView>),
        (status = 422, description = "Token sesi tidak ditemukan", body = umkm_shared::http::ErrorEnvelope)
    )
)]
pub async fn connect_session(
    State(state): State<AppState>,
    ReqId(id): ReqId,
    Member(actor): Member,
    Json(body): Json<ConnectSessionDto>,
) -> Result<Json<SuccessEnvelope<StatusView>>, ApiError> {
    actor.require("tenant.update")?;
    let p = pool(&state)?;
    let (_, e164) = umkm_infrastructure::gopay::normalize_phone(&body.phone)
        .map_err(|e| gobiz_err(e, "Nomor HP tidak valid"))?;
    let (access, refresh) = umkm_infrastructure::gopay::parse_session_blob(&body.session)
        .ok_or_else(|| {
            msg(
                ErrorCode::ValidationFailed,
                "Token sesi tidak ditemukan. Tempel cookie/token dari browser yang memuat access_token dan refresh_token.",
            )
        })?;
    let session = client()
        .session_from_tokens(access, refresh, &e164, now_ms())
        .await;
    gopay_repo::put_session(p, actor.tenant_id, &session, keyring(&state)?)
        .await
        .map_err(repo_err)?;
    let qris_ready = crate::commerce::ensure_qris_payload(&state, actor.tenant_id)
        .await?
        .qris_static_payload
        .is_some();
    let view = StatusView {
        connected: true,
        status: Some("active".into()),
        outlet_name: session.outlet_name.clone(),
        phone_masked: Some(mask_phone(&session.phone_e164)),
        has_static_qris: qris_ready,
    };
    Ok(Json(SuccessEnvelope::new(view, &id)))
}

/// Putuskan hubungan akun GoPay.
#[utoipa::path(
    delete, path = "/api/v1/tenants/{tenant_id}/gopay", tag = "gopay",
    params(("tenant_id" = Uuid, Path)),
    responses((status = 200, description = "Terputus", body = SuccessEnvelope<StatusView>))
)]
pub async fn disconnect(
    State(state): State<AppState>,
    ReqId(id): ReqId,
    Member(actor): Member,
) -> Result<Json<SuccessEnvelope<StatusView>>, ApiError> {
    actor.require("tenant.update")?;
    gopay_repo::delete_session(pool(&state)?, actor.tenant_id)
        .await
        .map_err(repo_err)?;
    Ok(Json(SuccessEnvelope::new(
        StatusView {
            connected: false,
            status: None,
            outlet_name: None,
            phone_masked: None,
            has_static_qris: false,
        },
        &id,
    )))
}

pub fn routes() -> Router<AppState> {
    let base = "/api/v1/tenants/{tenant_id}/gopay";
    Router::new()
        .route(base, get(get_status).delete(disconnect))
        .route(&format!("{base}/connect/request-otp"), post(request_otp))
        .route(&format!("{base}/connect/verify"), post(verify_otp))
        .route(&format!("{base}/connect/session"), post(connect_session))
}
