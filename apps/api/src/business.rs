//! Business profile/alamat/jam (P2-5, `PRD.md §4.4`). Baca: `tenant.read`; ubah: `tenant.update`.

use axum::{
    Json, Router,
    extract::State,
    routing::{get, put},
};
use serde::{Deserialize, Serialize};
use umkm_domain::business::{
    self, ADDRESS_MAX, DESCRIPTION_MAX, DayOverride, FieldError, MAX_ADDRESSES, MAX_SOCIALS,
    Minute, NAME_MAX, OpenStatus, Span,
};
use umkm_infrastructure::business_repo::{
    self as repo, Address, BusinessError, BusinessInfo, DayHours, HourException, Profile, Social,
};
use umkm_shared::http::{ErrorCode, SuccessEnvelope};
use utoipa::ToSchema;
use uuid::Uuid;

use crate::{
    ApiError, AppState,
    auth::ClientIp,
    middleware::ReqId,
    session::Member,
    validation::{field_errors, one},
};

// ------------------------------------------------------------------ DTO

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct SocialDto {
    pub platform: String,
    pub url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct ProfileDto {
    pub name: String,
    pub description: Option<String>,
    /// `asset:<uuid>` milik tenant (ADR-0009)
    pub logo: Option<String>,
    pub cover: Option<String>,
    pub phone: Option<String>,
    pub email: Option<String>,
    pub whatsapp: Option<String>,
    #[serde(default)]
    pub socials: Vec<SocialDto>,
    /// Timezone IANA, mis. `Asia/Jakarta`
    pub timezone: String,
    #[serde(default)]
    pub pickup_available: bool,
    #[serde(default)]
    pub delivery_available: bool,
    pub fulfillment_note: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct AddressDto {
    pub label: Option<String>,
    pub address: String,
    pub latitude: Option<f64>,
    pub longitude: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct HoursDto {
    /// ISO: 1 = Senin … 7 = Minggu
    pub day_of_week: u8,
    /// `HH:MM`
    pub open: String,
    /// `HH:MM`; lebih kecil dari `open` = tutup lewat tengah malam
    pub close: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct ExceptionDto {
    /// `YYYY-MM-DD` (tanggal lokal tenant)
    pub date: String,
    pub is_closed: bool,
    pub open: Option<String>,
    pub close: Option<String>,
    pub note: Option<String>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct BusinessView {
    pub profile: ProfileDto,
    pub addresses: Vec<AddressDto>,
    pub hours: Vec<HoursDto>,
    pub exceptions: Vec<ExceptionDto>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct StatusView {
    /// `open` | `closed`
    pub status: String,
    pub timezone: String,
    pub local_time: String,
}

// ------------------------------------------------------------------ konversi

fn asset_ref(id: Option<Uuid>) -> Option<String> {
    id.map(|i| format!("asset:{i}"))
}

pub fn to_view(i: BusinessInfo) -> BusinessView {
    let p = i.profile;
    BusinessView {
        profile: ProfileDto {
            name: p.name,
            description: p.description,
            logo: asset_ref(p.logo_object_id),
            cover: asset_ref(p.cover_object_id),
            phone: p.phone,
            email: p.email,
            whatsapp: p.whatsapp,
            socials: p
                .socials
                .into_iter()
                .map(|s| SocialDto {
                    platform: s.platform,
                    url: s.url,
                })
                .collect(),
            timezone: p.timezone,
            pickup_available: p.pickup_available,
            delivery_available: p.delivery_available,
            fulfillment_note: p.fulfillment_note,
        },
        addresses: i
            .addresses
            .into_iter()
            .map(|a| AddressDto {
                label: a.label,
                address: a.address,
                latitude: a.latitude,
                longitude: a.longitude,
            })
            .collect(),
        hours: i
            .hours
            .into_iter()
            .map(|h| HoursDto {
                day_of_week: h.day_of_week,
                open: h.span.open.format(),
                close: h.span.close.format(),
            })
            .collect(),
        exceptions: i
            .exceptions
            .into_iter()
            .map(|x| {
                let (closed, o, c) = match x.hours {
                    DayOverride::Closed => (true, None, None),
                    DayOverride::Hours(s) => (false, Some(s.open.format()), Some(s.close.format())),
                };
                ExceptionDto {
                    date: x.date,
                    is_closed: closed,
                    open: o,
                    close: c,
                    note: x.note,
                }
            })
            .collect(),
    }
}

fn reject(errs: Vec<FieldError>) -> ApiError {
    field_errors(errs.iter().map(|e| (e.path.as_str(), e.code)))
}

fn opt_trim(v: Option<String>) -> Option<String> {
    v.map(|s| s.trim().to_owned()).filter(|s| !s.is_empty())
}

fn parse_asset(errs: &mut Vec<FieldError>, path: &str, v: Option<&str>) -> Option<Uuid> {
    let s = v?;
    match s.strip_prefix("asset:").map(Uuid::parse_str) {
        Some(Ok(id)) => Some(id),
        _ => {
            business::push(errs, path, "invalid_asset_ref");
            None
        }
    }
}

fn validate_profile(d: ProfileDto) -> Result<Profile, ApiError> {
    let mut e = vec![];
    business::plain_text(&mut e, "name", Some(&d.name), NAME_MAX, true);
    let description = opt_trim(d.description);
    business::plain_text(
        &mut e,
        "description",
        description.as_deref(),
        DESCRIPTION_MAX,
        false,
    );
    let logo = parse_asset(&mut e, "logo", d.logo.as_deref());
    let cover = parse_asset(&mut e, "cover", d.cover.as_deref());
    let mut phone_of = |path: &str, v: Option<String>| {
        let v = opt_trim(v)?;
        // Nomor HP: hanya angka, diawali `0` (lokal) atau `62` (internasional).
        match business::normalize_phone_id(&v) {
            Some(n) => Some(n),
            None => {
                business::push(&mut e, path, "invalid_phone");
                None
            }
        }
    };
    let phone = phone_of("phone", d.phone);
    let whatsapp = phone_of("whatsapp", d.whatsapp);
    let email = opt_trim(d.email).map(|s| s.to_lowercase());
    if let Some(m) = &email
        && !business::is_valid_email(m)
    {
        business::push(&mut e, "email", "invalid_email");
    }
    if d.socials.len() > MAX_SOCIALS {
        business::push(&mut e, "socials", "too_many");
    }
    for (i, s) in d.socials.iter().enumerate().take(MAX_SOCIALS) {
        business::validate_social(&mut e, &format!("socials[{i}]"), &s.platform, &s.url);
    }
    let note = opt_trim(d.fulfillment_note);
    business::plain_text(&mut e, "fulfillment_note", note.as_deref(), 500, false);
    let tz = d.timezone.trim().to_owned();
    if tz.is_empty() || tz.len() > 64 {
        business::push(&mut e, "timezone", "invalid_timezone");
    }
    if !e.is_empty() {
        return Err(reject(e));
    }
    Ok(Profile {
        name: d.name.trim().to_owned(),
        description,
        logo_object_id: logo,
        cover_object_id: cover,
        phone,
        email,
        whatsapp,
        socials: d
            .socials
            .into_iter()
            .map(|s| Social {
                platform: s.platform,
                url: s.url,
            })
            .collect(),
        timezone: tz,
        pickup_available: d.pickup_available,
        delivery_available: d.delivery_available,
        fulfillment_note: note,
    })
}

fn span(e: &mut Vec<FieldError>, path: &str, open: &str, close: &str) -> Option<Span> {
    let (o, c) = (Minute::parse(open), Minute::parse(close));
    if o.is_none() {
        business::push(e, format!("{path}.open"), "invalid_time");
    }
    if c.is_none() {
        business::push(e, format!("{path}.close"), "invalid_time");
    }
    let s = Span::new(o?, c?);
    if s.is_none() {
        business::push(e, path, "open_equals_close");
    }
    s
}

fn is_iso_date(s: &str) -> bool {
    let b = s.as_bytes();
    b.len() == 10
        && b[4] == b'-'
        && b[7] == b'-'
        && b.iter()
            .enumerate()
            .all(|(i, c)| i == 4 || i == 7 || c.is_ascii_digit())
}

fn repo_err(e: BusinessError) -> ApiError {
    match e {
        BusinessError::InvalidTimezone => one("timezone", "invalid_timezone"),
        BusinessError::InvalidAsset => one("logo", "invalid_asset_ref"),
        BusinessError::InvalidDate => one("exceptions", "invalid_or_duplicate_date"),
        BusinessError::Db(e) => internal(e),
    }
}

fn internal(e: sqlx::Error) -> ApiError {
    tracing::error!(error = %e, "business query failed");
    ApiError::new(ErrorCode::InternalError)
}

fn pool(state: &AppState) -> Result<&sqlx::PgPool, ApiError> {
    state
        .auth
        .as_ref()
        .map(|a| &a.pool)
        .ok_or_else(|| ApiError::new(ErrorCode::ServiceUnavailable))
}

async fn current(
    state: &AppState,
    t: umkm_shared::ids::TenantId,
) -> Result<BusinessView, ApiError> {
    repo::load(pool(state)?, t)
        .await
        .map_err(internal)?
        .map(to_view)
        .ok_or_else(|| ApiError::new(ErrorCode::ResourceNotFound))
}

// ------------------------------------------------------------------ handler

/// Profil, alamat, jam, dan exception bisnis.
#[utoipa::path(
    get, path = "/api/v1/tenants/{tenant_id}/business/profile", tag = "business",
    params(("tenant_id" = Uuid, Path)),
    responses((status = 200, description = "Data bisnis", body = SuccessEnvelope<BusinessView>))
)]
pub async fn get_profile(
    State(state): State<AppState>,
    ReqId(id): ReqId,
    Member(actor): Member,
) -> Result<Json<SuccessEnvelope<BusinessView>>, ApiError> {
    actor.require("tenant.read")?;
    Ok(Json(SuccessEnvelope::new(
        current(&state, actor.tenant_id).await?,
        &id,
    )))
}

/// Ganti profil bisnis (AC-BIZ-001/002/007).
#[utoipa::path(
    put, path = "/api/v1/tenants/{tenant_id}/business/profile", tag = "business",
    params(("tenant_id" = Uuid, Path)), request_body = ProfileDto,
    responses(
        (status = 200, description = "Tersimpan", body = SuccessEnvelope<BusinessView>),
        (status = 422, description = "Validasi gagal", body = umkm_shared::http::ErrorEnvelope)
    )
)]
pub async fn put_profile(
    State(state): State<AppState>,
    ReqId(id): ReqId,
    ClientIp(ip): ClientIp,
    Member(actor): Member,
    Json(req): Json<ProfileDto>,
) -> Result<Json<SuccessEnvelope<BusinessView>>, ApiError> {
    actor.require("tenant.update")?;
    let p = validate_profile(req)?;
    repo::update_profile(pool(&state)?, &actor, &p, (Some(id.as_str()), Some(&ip)))
        .await
        .map_err(repo_err)?;
    Ok(Json(SuccessEnvelope::new(
        current(&state, actor.tenant_id).await?,
        &id,
    )))
}

/// Ganti seluruh daftar alamat (urutan = urutan tampil; AC-BIZ-003).
#[utoipa::path(
    put, path = "/api/v1/tenants/{tenant_id}/business/addresses", tag = "business",
    params(("tenant_id" = Uuid, Path)), request_body = Vec<AddressDto>,
    responses((status = 200, description = "Tersimpan", body = SuccessEnvelope<BusinessView>))
)]
pub async fn put_addresses(
    State(state): State<AppState>,
    ReqId(id): ReqId,
    ClientIp(ip): ClientIp,
    Member(actor): Member,
    Json(req): Json<Vec<AddressDto>>,
) -> Result<Json<SuccessEnvelope<BusinessView>>, ApiError> {
    actor.require("tenant.update")?;
    let mut e = vec![];
    if req.len() > MAX_ADDRESSES {
        business::push(&mut e, "addresses", "too_many");
    }
    let mut list = vec![];
    for (i, a) in req.into_iter().enumerate().take(MAX_ADDRESSES) {
        let p = format!("[{i}]");
        business::plain_text(
            &mut e,
            &format!("{p}.address"),
            Some(&a.address),
            ADDRESS_MAX,
            true,
        );
        let label = opt_trim(a.label);
        business::plain_text(&mut e, &format!("{p}.label"), label.as_deref(), 60, false);
        business::validate_coordinates(&mut e, &p, a.latitude, a.longitude);
        list.push(Address {
            label,
            address: a.address.trim().to_owned(),
            latitude: a.latitude,
            longitude: a.longitude,
        });
    }
    if !e.is_empty() {
        return Err(reject(e));
    }
    repo::replace_addresses(pool(&state)?, &actor, &list, (Some(id.as_str()), Some(&ip)))
        .await
        .map_err(repo_err)?;
    Ok(Json(SuccessEnvelope::new(
        current(&state, actor.tenant_id).await?,
        &id,
    )))
}

/// Ganti jam operasional mingguan (AC-BIZ-004).
#[utoipa::path(
    put, path = "/api/v1/tenants/{tenant_id}/business/hours", tag = "business",
    params(("tenant_id" = Uuid, Path)), request_body = Vec<HoursDto>,
    responses((status = 200, description = "Tersimpan", body = SuccessEnvelope<BusinessView>))
)]
pub async fn put_hours(
    State(state): State<AppState>,
    ReqId(id): ReqId,
    ClientIp(ip): ClientIp,
    Member(actor): Member,
    Json(req): Json<Vec<HoursDto>>,
) -> Result<Json<SuccessEnvelope<BusinessView>>, ApiError> {
    actor.require("tenant.update")?;
    let mut e = vec![];
    let mut seen = [false; 8];
    let mut list = vec![];
    for (i, h) in req.iter().enumerate().take(7 * 2) {
        let p = format!("[{i}]");
        if !(1..=7).contains(&h.day_of_week) {
            business::push(&mut e, format!("{p}.day_of_week"), "out_of_range");
            continue;
        }
        if std::mem::replace(&mut seen[usize::from(h.day_of_week)], true) {
            business::push(&mut e, format!("{p}.day_of_week"), "duplicate_day");
        }
        if let Some(s) = span(&mut e, &p, &h.open, &h.close) {
            list.push(DayHours {
                day_of_week: h.day_of_week,
                span: s,
            });
        }
    }
    if req.len() > 7 {
        business::push(&mut e, "hours", "too_many");
    }
    if !e.is_empty() {
        return Err(reject(e));
    }
    repo::replace_hours(pool(&state)?, &actor, &list, (Some(id.as_str()), Some(&ip)))
        .await
        .map_err(repo_err)?;
    Ok(Json(SuccessEnvelope::new(
        current(&state, actor.tenant_id).await?,
        &id,
    )))
}

/// Ganti daftar hari libur / jam khusus (AC-BIZ-005). Maks 100 tanggal.
#[utoipa::path(
    put, path = "/api/v1/tenants/{tenant_id}/business/hour-exceptions", tag = "business",
    params(("tenant_id" = Uuid, Path)), request_body = Vec<ExceptionDto>,
    responses((status = 200, description = "Tersimpan", body = SuccessEnvelope<BusinessView>))
)]
pub async fn put_exceptions(
    State(state): State<AppState>,
    ReqId(id): ReqId,
    ClientIp(ip): ClientIp,
    Member(actor): Member,
    Json(req): Json<Vec<ExceptionDto>>,
) -> Result<Json<SuccessEnvelope<BusinessView>>, ApiError> {
    actor.require("tenant.update")?;
    let mut e = vec![];
    if req.len() > 100 {
        business::push(&mut e, "exceptions", "too_many");
    }
    let mut list = vec![];
    for (i, x) in req.into_iter().enumerate().take(100) {
        let p = format!("[{i}]");
        if !is_iso_date(&x.date) {
            business::push(&mut e, format!("{p}.date"), "invalid_date");
        }
        let note = opt_trim(x.note);
        business::plain_text(&mut e, &format!("{p}.note"), note.as_deref(), 120, false);
        let hours = match (x.is_closed, x.open.as_deref(), x.close.as_deref()) {
            (true, None, None) => Some(DayOverride::Closed),
            (false, Some(o), Some(c)) => span(&mut e, &p, o, c).map(DayOverride::Hours),
            _ => {
                business::push(&mut e, &p, "closed_xor_hours");
                None
            }
        };
        if let Some(hours) = hours {
            list.push(HourException {
                date: x.date,
                hours,
                note,
            });
        }
    }
    if !e.is_empty() {
        return Err(reject(e));
    }
    repo::replace_exceptions(pool(&state)?, &actor, &list, (Some(id.as_str()), Some(&ip)))
        .await
        .map_err(repo_err)?;
    Ok(Json(SuccessEnvelope::new(
        current(&state, actor.tenant_id).await?,
        &id,
    )))
}

/// Status buka/tutup saat ini menurut timezone tenant (AC-BIZ-006).
#[utoipa::path(
    get, path = "/api/v1/tenants/{tenant_id}/business/status", tag = "business",
    params(("tenant_id" = Uuid, Path)),
    responses((status = 200, description = "Status", body = SuccessEnvelope<StatusView>))
)]
pub async fn get_status(
    State(state): State<AppState>,
    ReqId(id): ReqId,
    Member(actor): Member,
) -> Result<Json<SuccessEnvelope<StatusView>>, ApiError> {
    actor.require("tenant.read")?;
    let s = repo::status(pool(&state)?, actor.tenant_id, None)
        .await
        .map_err(internal)?
        .ok_or_else(|| ApiError::new(ErrorCode::ResourceNotFound))?;
    Ok(Json(SuccessEnvelope::new(status_view(s), &id)))
}

pub fn status_view(s: repo::StatusNow) -> StatusView {
    StatusView {
        status: match s.status {
            OpenStatus::Open => "open",
            OpenStatus::Closed => "closed",
        }
        .into(),
        timezone: s.timezone,
        local_time: s.local_time,
    }
}

pub fn routes() -> Router<AppState> {
    let base = "/api/v1/tenants/{tenant_id}/business";
    Router::new()
        .route(
            &format!("{base}/profile"),
            get(get_profile).put(put_profile),
        )
        .route(&format!("{base}/addresses"), put(put_addresses))
        .route(&format!("{base}/hours"), put(put_hours))
        .route(&format!("{base}/hour-exceptions"), put(put_exceptions))
        .route(&format!("{base}/status"), get(get_status))
}
