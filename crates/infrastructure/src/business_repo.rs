//! Business profile, alamat, jam, exception (P2-5, `PRD.md §4.4`).
//! Semua query dalam transaksi ber-tenant-context (RLS) + filter `tenant_id`.

use sqlx::{PgPool, Postgres, Row, Transaction};
use umkm_domain::{
    audit::AuditEvent,
    business::{DayOverride, Minute, OpenStatus, Span, WeeklyHours, status_at},
    rbac::Actor,
};
use umkm_shared::ids::TenantId;
use uuid::Uuid;

pub const BUSINESS_UPDATED: &str = "business.updated";

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Social {
    pub platform: String,
    pub url: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Profile {
    pub name: String,
    pub description: Option<String>,
    pub logo_object_id: Option<Uuid>,
    pub cover_object_id: Option<Uuid>,
    pub phone: Option<String>,
    pub email: Option<String>,
    pub whatsapp: Option<String>,
    pub socials: Vec<Social>,
    pub timezone: String,
    pub pickup_available: bool,
    pub delivery_available: bool,
    pub fulfillment_note: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Address {
    pub label: Option<String>,
    pub address: String,
    pub latitude: Option<f64>,
    pub longitude: Option<f64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DayHours {
    pub day_of_week: u8,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HourException {
    /// `YYYY-MM-DD`
    pub date: String,
    pub hours: DayOverride,
    pub note: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct BusinessInfo {
    pub profile: Profile,
    pub addresses: Vec<Address>,
    pub hours: Vec<DayHours>,
    pub exceptions: Vec<HourException>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StatusNow {
    pub status: OpenStatus,
    pub timezone: String,
    /// `YYYY-MM-DDTHH:MM` waktu lokal tenant.
    pub local_time: String,
}

#[derive(Debug, thiserror::Error)]
pub enum BusinessError {
    #[error("timezone tidak dikenal")]
    InvalidTimezone,
    #[error("gambar bukan milik tenant")]
    InvalidAsset,
    #[error("tanggal tidak valid")]
    InvalidDate,
    #[error(transparent)]
    Db(#[from] sqlx::Error),
}

fn db_err(e: umkm_database::DbError) -> sqlx::Error {
    match e {
        umkm_database::DbError::Sqlx(e) => e,
        umkm_database::DbError::InvalidUrl => sqlx::Error::Configuration("invalid url".into()),
    }
}

fn code(e: &sqlx::Error) -> Option<String> {
    e.as_database_error()
        .and_then(|d| d.code())
        .map(|c| c.into_owned())
}

async fn tx(pool: &PgPool, t: TenantId) -> Result<Transaction<'static, Postgres>, sqlx::Error> {
    umkm_database::begin_tenant_tx(pool, t)
        .await
        .map_err(db_err)
}

fn span_of(open: &str, close: &str) -> Option<Span> {
    Span::new(Minute::parse(open)?, Minute::parse(close)?)
}

/// Seluruh data bisnis tenant (dashboard + situs publik).
pub async fn load(pool: &PgPool, t: TenantId) -> Result<Option<BusinessInfo>, sqlx::Error> {
    let mut tx = tx(pool, t).await?;
    let info = load_in(&mut tx, t).await?;
    tx.commit().await?;
    Ok(info)
}

pub async fn load_in(
    tx: &mut Transaction<'static, Postgres>,
    t: TenantId,
) -> Result<Option<BusinessInfo>, sqlx::Error> {
    let tid = t.as_uuid();
    let Some(r) = sqlx::query(
        "SELECT name, description, logo_object_id, cover_object_id, phone, email, whatsapp,
                socials, timezone, pickup_available, delivery_available, fulfillment_note
         FROM business_profiles WHERE tenant_id = $1",
    )
    .bind(tid)
    .fetch_optional(&mut **tx)
    .await?
    else {
        return Ok(None);
    };
    let socials: sqlx::types::Json<Vec<Social>> = r.try_get("socials").unwrap_or_default();
    let profile = Profile {
        name: r.get("name"),
        description: r.get("description"),
        logo_object_id: r.get("logo_object_id"),
        cover_object_id: r.get("cover_object_id"),
        phone: r.get("phone"),
        email: r.get("email"),
        whatsapp: r.get("whatsapp"),
        socials: socials.0,
        timezone: r.get("timezone"),
        pickup_available: r.get("pickup_available"),
        delivery_available: r.get("delivery_available"),
        fulfillment_note: r.get("fulfillment_note"),
    };
    let addresses = sqlx::query(
        "SELECT label, address, latitude, longitude FROM business_addresses
         WHERE tenant_id = $1 ORDER BY position",
    )
    .bind(tid)
    .fetch_all(&mut **tx)
    .await?
    .into_iter()
    .map(|r| Address {
        label: r.get("label"),
        address: r.get("address"),
        latitude: r.get("latitude"),
        longitude: r.get("longitude"),
    })
    .collect();
    let hours = sqlx::query(
        "SELECT day_of_week, to_char(open_time, 'HH24:MI') AS o, to_char(close_time, 'HH24:MI') AS c
         FROM business_hours WHERE tenant_id = $1 ORDER BY day_of_week",
    )
    .bind(tid)
    .fetch_all(&mut **tx)
    .await?
    .into_iter()
    .filter_map(|r| {
        let d: i16 = r.get("day_of_week");
        Some(DayHours {
            day_of_week: u8::try_from(d).ok()?,
            span: span_of(r.get("o"), r.get("c"))?,
        })
    })
    .collect();
    let exceptions = sqlx::query(
        "SELECT to_char(date, 'YYYY-MM-DD') AS d, is_closed, to_char(open_time, 'HH24:MI') AS o,
                to_char(close_time, 'HH24:MI') AS c, note
         FROM business_hour_exceptions WHERE tenant_id = $1 ORDER BY date",
    )
    .bind(tid)
    .fetch_all(&mut **tx)
    .await?
    .into_iter()
    .filter_map(|r| {
        let closed: bool = r.get("is_closed");
        let hours = if closed {
            DayOverride::Closed
        } else {
            DayOverride::Hours(span_of(
                r.get::<Option<&str>, _>("o")?,
                r.get::<Option<&str>, _>("c")?,
            )?)
        };
        Some(HourException {
            date: r.get("d"),
            hours,
            note: r.get("note"),
        })
    })
    .collect();
    Ok(Some(BusinessInfo {
        profile,
        addresses,
        hours,
        exceptions,
    }))
}

async fn audit(
    tx: &mut Transaction<'static, Postgres>,
    actor: &Actor,
    part: &str,
    (request_id, ip): (Option<&str>, Option<&str>),
) -> Result<(), sqlx::Error> {
    crate::audit_repo::record(
        &mut **tx,
        &AuditEvent::new(BUSINESS_UPDATED, "business_profile")
            .by_user(actor.user_id)
            .in_tenant(actor.tenant_id)
            .resource(actor.tenant_id)
            .request(request_id, ip)
            .with_summary(serde_json::json!({ "part": part })),
    )
    .await
}

/// Ganti profil. Validasi format sudah dilakukan pemanggil (domain); di sini timezone IANA
/// dicek ke katalog PostgreSQL dan kepemilikan gambar dijaga composite FK.
pub async fn update_profile(
    pool: &PgPool,
    actor: &Actor,
    p: &Profile,
    ctx: (Option<&str>, Option<&str>),
) -> Result<(), BusinessError> {
    let mut tx = tx(pool, actor.tenant_id).await?;
    let tz_ok: bool =
        sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM pg_timezone_names WHERE name = $1)")
            .bind(&p.timezone)
            .fetch_one(&mut *tx)
            .await?;
    if !tz_ok {
        return Err(BusinessError::InvalidTimezone);
    }
    let r = sqlx::query(
        "UPDATE business_profiles SET name = $2, description = $3, logo_object_id = $4,
            cover_object_id = $5, phone = $6, email = $7, whatsapp = $8, socials = $9,
            timezone = $10, pickup_available = $11, delivery_available = $12, fulfillment_note = $13
         WHERE tenant_id = $1",
    )
    .bind(actor.tenant_id.as_uuid())
    .bind(&p.name)
    .bind(&p.description)
    .bind(p.logo_object_id)
    .bind(p.cover_object_id)
    .bind(&p.phone)
    .bind(&p.email)
    .bind(&p.whatsapp)
    .bind(sqlx::types::Json(&p.socials))
    .bind(&p.timezone)
    .bind(p.pickup_available)
    .bind(p.delivery_available)
    .bind(&p.fulfillment_note)
    .execute(&mut *tx)
    .await;
    match r {
        Err(e) if code(&e).as_deref() == Some("23503") => return Err(BusinessError::InvalidAsset),
        Err(e) => return Err(e.into()),
        Ok(_) => {}
    }
    audit(&mut tx, actor, "profile", ctx).await?;
    tx.commit().await?;
    Ok(())
}

pub async fn replace_addresses(
    pool: &PgPool,
    actor: &Actor,
    list: &[Address],
    ctx: (Option<&str>, Option<&str>),
) -> Result<(), BusinessError> {
    let mut tx = tx(pool, actor.tenant_id).await?;
    let tid = actor.tenant_id.as_uuid();
    sqlx::query("DELETE FROM business_addresses WHERE tenant_id = $1")
        .bind(tid)
        .execute(&mut *tx)
        .await?;
    for (i, a) in list.iter().enumerate() {
        sqlx::query(
            "INSERT INTO business_addresses (tenant_id, position, label, address, latitude, longitude)
             VALUES ($1, $2, $3, $4, $5, $6)",
        )
        .bind(tid)
        .bind(i16::try_from(i).unwrap_or(i16::MAX))
        .bind(&a.label)
        .bind(&a.address)
        .bind(a.latitude)
        .bind(a.longitude)
        .execute(&mut *tx)
        .await?;
    }
    audit(&mut tx, actor, "addresses", ctx).await?;
    tx.commit().await?;
    Ok(())
}

pub async fn replace_hours(
    pool: &PgPool,
    actor: &Actor,
    list: &[DayHours],
    ctx: (Option<&str>, Option<&str>),
) -> Result<(), BusinessError> {
    let mut tx = tx(pool, actor.tenant_id).await?;
    let tid = actor.tenant_id.as_uuid();
    sqlx::query("DELETE FROM business_hours WHERE tenant_id = $1")
        .bind(tid)
        .execute(&mut *tx)
        .await?;
    for h in list {
        sqlx::query(
            "INSERT INTO business_hours (tenant_id, day_of_week, open_time, close_time)
             VALUES ($1, $2, $3::time, $4::time)",
        )
        .bind(tid)
        .bind(i16::from(h.day_of_week))
        .bind(h.span.open.format())
        .bind(h.span.close.format())
        .execute(&mut *tx)
        .await?;
    }
    audit(&mut tx, actor, "hours", ctx).await?;
    tx.commit().await?;
    Ok(())
}

pub async fn replace_exceptions(
    pool: &PgPool,
    actor: &Actor,
    list: &[HourException],
    ctx: (Option<&str>, Option<&str>),
) -> Result<(), BusinessError> {
    let mut tx = tx(pool, actor.tenant_id).await?;
    let tid = actor.tenant_id.as_uuid();
    sqlx::query("DELETE FROM business_hour_exceptions WHERE tenant_id = $1")
        .bind(tid)
        .execute(&mut *tx)
        .await?;
    for x in list {
        let (closed, o, c) = match x.hours {
            DayOverride::Closed => (true, None, None),
            DayOverride::Hours(s) => (false, Some(s.open.format()), Some(s.close.format())),
        };
        let r = sqlx::query(
            "INSERT INTO business_hour_exceptions (tenant_id, date, is_closed, open_time, close_time, note)
             VALUES ($1, $2::date, $3, $4::time, $5::time, $6)",
        )
        .bind(tid)
        .bind(&x.date)
        .bind(closed)
        .bind(o)
        .bind(c)
        .bind(&x.note)
        .execute(&mut *tx)
        .await;
        match r {
            // 22007/22008: format/rentang tanggal; 23505: tanggal ganda
            Err(e) if matches!(code(&e).as_deref(), Some("22007" | "22008" | "23505")) => {
                return Err(BusinessError::InvalidDate);
            }
            Err(e) => return Err(e.into()),
            Ok(_) => {}
        }
    }
    audit(&mut tx, actor, "hour_exceptions", ctx).await?;
    tx.commit().await?;
    Ok(())
}

/// Status buka/tutup sekarang (AC-BIZ-006). `at_utc` untuk uji (`None` = now()).
pub async fn status(
    pool: &PgPool,
    t: TenantId,
    at_utc: Option<&str>,
) -> Result<Option<StatusNow>, sqlx::Error> {
    let mut tx = tx(pool, t).await?;
    let Some(info) = load_in(&mut tx, t).await? else {
        return Ok(None);
    };
    let r = sqlx::query(
        "WITH l AS (SELECT (coalesce($2::timestamptz, now()) AT TIME ZONE $1) AS ts)
         SELECT to_char(ts, 'YYYY-MM-DD') AS today, to_char(ts - interval '1 day', 'YYYY-MM-DD') AS yesterday,
                extract(isodow FROM ts)::int AS dow, (extract(hour FROM ts) * 60 + extract(minute FROM ts))::int AS minute,
                to_char(ts, 'YYYY-MM-DD\"T\"HH24:MI') AS local
         FROM l",
    )
    .bind(&info.profile.timezone)
    .bind(at_utc)
    .fetch_one(&mut *tx)
    .await?;
    tx.commit().await?;
    let mut weekly = WeeklyHours::default();
    for h in &info.hours {
        weekly.set(h.day_of_week, h.span);
    }
    let find = |d: &str| {
        info.exceptions
            .iter()
            .find(|x| x.date == d)
            .map(|x| x.hours)
    };
    let today: String = r.get("today");
    let yesterday: String = r.get("yesterday");
    let dow: i32 = r.get("dow");
    let minute: i32 = r.get("minute");
    let now = Minute::from_minutes(u16::try_from(minute).unwrap_or(0))
        .unwrap_or(Minute::from_minutes(0).expect("0 valid"));
    Ok(Some(StatusNow {
        status: status_at(
            &weekly,
            u8::try_from(dow).unwrap_or(1),
            now,
            find(&today),
            find(&yesterday),
        ),
        timezone: info.profile.timezone,
        local_time: r.get("local"),
    }))
}
