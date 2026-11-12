//! Customer & CRM (P3-3, `PRD.md §4.6`, ADR-0005). Semua query di tenant context (RLS).
//! Audit tidak memuat PII (hanya id + bagian yang berubah).

use sqlx::{PgPool, Row};
use umkm_domain::{
    audit::AuditEvent,
    crm::{Contact, CustomerAddress},
    rbac::Actor,
};
use umkm_shared::ids::TenantId;
use uuid::Uuid;

use crate::pgx::{Tx, tenant_tx};

pub const CUSTOMER_CREATED: &str = "customer.created";
pub const CUSTOMER_UPDATED: &str = "customer.updated";
pub const CUSTOMER_DELETION_REQUESTED: &str = "customer.deletion_requested";
pub const CUSTOMER_EXPORTED: &str = "customer.exported";
pub const CUSTOMER_LINKED: &str = "customer.linked";

pub type Ctx<'a> = (Option<&'a str>, Option<&'a str>);

#[derive(Debug, thiserror::Error)]
pub enum CrmError {
    #[error("customer tidak ditemukan")]
    NotFound,
    #[error("customer sudah dianonimkan / dalam proses hapus")]
    Deleted,
    #[error(transparent)]
    Db(#[from] sqlx::Error),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CustomerSummary {
    pub id: Uuid,
    pub name: String,
    pub email: Option<String>,
    pub phone: Option<String>,
    pub tags: Vec<String>,
    /// Ditautkan ke akun terverifikasi
    pub has_account: bool,
    pub created_at: String,
    pub deletion_requested: bool,
    pub anonymized: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Customer {
    pub summary: CustomerSummary,
    pub internal_note: Option<String>,
    pub addresses: Vec<CustomerAddress>,
}

const COLS: &str = "c.id, c.name, c.email, c.phone, c.tags, (c.account_id IS NOT NULL) AS has_account,
    to_char(c.created_at AT TIME ZONE 'UTC', 'YYYY-MM-DD\"T\"HH24:MI:SS.US\"Z\"') AS created_at,
    (c.deletion_requested_at IS NOT NULL) AS deletion_requested, (c.anonymized_at IS NOT NULL) AS anonymized,
    c.internal_note";

fn summary_of(r: &sqlx::postgres::PgRow) -> CustomerSummary {
    CustomerSummary {
        id: r.get("id"),
        name: r.get("name"),
        email: r.get("email"),
        phone: r.get("phone"),
        tags: r.get("tags"),
        has_account: r.get("has_account"),
        created_at: r.get("created_at"),
        deletion_requested: r.get("deletion_requested"),
        anonymized: r.get("anonymized"),
    }
}

async fn audit(
    tx: &mut Tx,
    actor: Option<&Actor>,
    t: TenantId,
    action: &'static str,
    id: Uuid,
    summary: serde_json::Value,
    (request_id, ip): Ctx<'_>,
) -> Result<(), sqlx::Error> {
    let mut e = AuditEvent::new(action, "customer")
        .in_tenant(t)
        .resource(id)
        .request(request_id, ip)
        .with_summary(summary);
    if let Some(a) = actor {
        e = e.by_user(a.user_id);
    }
    crate::audit_repo::record(&mut **tx, &e).await
}

/// Customer baru (guest checkout, staf, atau profil akun). Tidak pernah menggabungkan
/// dengan customer lain berdasarkan email (ADR-0005).
pub async fn create_in(
    tx: &mut Tx,
    t: TenantId,
    c: &Contact,
    account_id: Option<Uuid>,
) -> Result<Uuid, sqlx::Error> {
    sqlx::query_scalar(
        "INSERT INTO customers (tenant_id, account_id, name, email, phone) VALUES ($1, $2, $3, $4, $5)
         RETURNING id",
    )
    .bind(t.as_uuid())
    .bind(account_id)
    .bind(&c.name)
    .bind(&c.email)
    .bind(&c.phone)
    .fetch_one(&mut **tx)
    .await
}

pub async fn create(
    pool: &PgPool,
    actor: &Actor,
    c: &Contact,
    ctx: Ctx<'_>,
) -> Result<Uuid, sqlx::Error> {
    let t = actor.tenant_id;
    let mut tx = tenant_tx(pool, t).await?;
    let id = create_in(&mut tx, t, c, None).await?;
    audit(
        &mut tx,
        Some(actor),
        t,
        CUSTOMER_CREATED,
        id,
        serde_json::json!({ "source": "staff" }),
        ctx,
    )
    .await?;
    tx.commit().await?;
    Ok(id)
}

/// Pencarian nama/email/telepon (AC-CRM-002: hanya tenant sendiri, via RLS + filter).
pub async fn list(
    pool: &PgPool,
    t: TenantId,
    q: Option<&str>,
    tag: Option<&str>,
    limit: i64,
    offset: i64,
) -> Result<(Vec<CustomerSummary>, i64), sqlx::Error> {
    let mut tx = tenant_tx(pool, t).await?;
    let q = q.map(|q| {
        format!(
            "%{}%",
            q.replace('\\', "\\\\")
                .replace('%', "\\%")
                .replace('_', "\\_")
        )
    });
    let w = "FROM customers c WHERE c.tenant_id = $1 AND c.anonymized_at IS NULL
        AND ($2::text IS NULL OR c.name ILIKE $2 OR c.email ILIKE $2 OR c.phone ILIKE $2)
        AND ($3::text IS NULL OR $3 = ANY(c.tags))";
    let total: i64 = sqlx::query_scalar(sqlx::AssertSqlSafe(format!("SELECT count(*) {w}")))
        .bind(t.as_uuid())
        .bind(&q)
        .bind(tag)
        .fetch_one(&mut *tx)
        .await?;
    let rows = sqlx::query(sqlx::AssertSqlSafe(format!(
        "SELECT {COLS} {w} ORDER BY c.created_at DESC, c.id LIMIT $4 OFFSET $5"
    )))
    .bind(t.as_uuid())
    .bind(&q)
    .bind(tag)
    .bind(limit)
    .bind(offset)
    .fetch_all(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok((rows.iter().map(summary_of).collect(), total))
}

pub async fn get_in(tx: &mut Tx, t: TenantId, id: Uuid) -> Result<Option<Customer>, sqlx::Error> {
    let Some(r) = sqlx::query(sqlx::AssertSqlSafe(format!(
        "SELECT {COLS} FROM customers c WHERE c.tenant_id = $1 AND c.id = $2"
    )))
    .bind(t.as_uuid())
    .bind(id)
    .fetch_optional(&mut **tx)
    .await?
    else {
        return Ok(None);
    };
    let addresses = sqlx::query(
        "SELECT label, address, note FROM customer_addresses
         WHERE tenant_id = $1 AND customer_id = $2 ORDER BY position",
    )
    .bind(t.as_uuid())
    .bind(id)
    .fetch_all(&mut **tx)
    .await?
    .iter()
    .map(|a| CustomerAddress {
        label: a.get("label"),
        address: a.get("address"),
        note: a.get("note"),
    })
    .collect();
    Ok(Some(Customer {
        summary: summary_of(&r),
        internal_note: r.get("internal_note"),
        addresses,
    }))
}

pub async fn get(pool: &PgPool, t: TenantId, id: Uuid) -> Result<Option<Customer>, sqlx::Error> {
    let mut tx = tenant_tx(pool, t).await?;
    let c = get_in(&mut tx, t, id).await?;
    tx.commit().await?;
    Ok(c)
}

async fn lock_live(tx: &mut Tx, t: TenantId, id: Uuid) -> Result<(), CrmError> {
    let r = sqlx::query(
        "SELECT deletion_requested_at IS NOT NULL OR anonymized_at IS NOT NULL AS gone
         FROM customers WHERE tenant_id = $1 AND id = $2 FOR UPDATE",
    )
    .bind(t.as_uuid())
    .bind(id)
    .fetch_optional(&mut **tx)
    .await?
    .ok_or(CrmError::NotFound)?;
    if r.get::<bool, _>("gone") {
        return Err(CrmError::Deleted);
    }
    Ok(())
}

/// Ubah profil + tag + catatan internal (staf, `customers.manage`).
pub async fn update(
    pool: &PgPool,
    actor: &Actor,
    id: Uuid,
    c: &Contact,
    tags: &[String],
    note: Option<&str>,
    ctx: Ctx<'_>,
) -> Result<(), CrmError> {
    let t = actor.tenant_id;
    let mut tx = tenant_tx(pool, t).await?;
    lock_live(&mut tx, t, id).await?;
    sqlx::query(
        "UPDATE customers SET name = $3, email = $4, phone = $5, tags = $6, internal_note = $7,
             updated_at = now()
         WHERE tenant_id = $1 AND id = $2",
    )
    .bind(t.as_uuid())
    .bind(id)
    .bind(&c.name)
    .bind(&c.email)
    .bind(&c.phone)
    .bind(tags)
    .bind(note)
    .execute(&mut *tx)
    .await?;
    audit(
        &mut tx,
        Some(actor),
        t,
        CUSTOMER_UPDATED,
        id,
        serde_json::json!({ "part": "profile" }),
        ctx,
    )
    .await?;
    tx.commit().await?;
    Ok(())
}

pub async fn replace_addresses_in(
    tx: &mut Tx,
    t: TenantId,
    id: Uuid,
    list: &[CustomerAddress],
) -> Result<(), sqlx::Error> {
    sqlx::query("DELETE FROM customer_addresses WHERE tenant_id = $1 AND customer_id = $2")
        .bind(t.as_uuid())
        .bind(id)
        .execute(&mut **tx)
        .await?;
    for (i, a) in list.iter().enumerate() {
        sqlx::query(
            "INSERT INTO customer_addresses (tenant_id, customer_id, position, label, address, note)
             VALUES ($1, $2, $3, $4, $5, $6)",
        )
        .bind(t.as_uuid())
        .bind(id)
        .bind(i16::try_from(i).unwrap_or(i16::MAX))
        .bind(&a.label)
        .bind(&a.address)
        .bind(&a.note)
        .execute(&mut **tx)
        .await?;
    }
    Ok(())
}

pub async fn replace_addresses(
    pool: &PgPool,
    actor: &Actor,
    id: Uuid,
    list: &[CustomerAddress],
    ctx: Ctx<'_>,
) -> Result<(), CrmError> {
    let t = actor.tenant_id;
    let mut tx = tenant_tx(pool, t).await?;
    lock_live(&mut tx, t, id).await?;
    replace_addresses_in(&mut tx, t, id, list).await?;
    audit(
        &mut tx,
        Some(actor),
        t,
        CUSTOMER_UPDATED,
        id,
        serde_json::json!({ "part": "addresses" }),
        ctx,
    )
    .await?;
    tx.commit().await?;
    Ok(())
}

/// AC-CRM-006: tandai hapus; sesi akun tertaut dicabut; anonimisasi oleh job +30 hari.
pub async fn request_deletion(
    pool: &PgPool,
    actor: &Actor,
    id: Uuid,
    ctx: Ctx<'_>,
) -> Result<(), CrmError> {
    let t = actor.tenant_id;
    let mut tx = tenant_tx(pool, t).await?;
    lock_live(&mut tx, t, id).await?;
    let account: Option<Uuid> = sqlx::query_scalar(
        "UPDATE customers SET deletion_requested_at = now(), updated_at = now()
         WHERE tenant_id = $1 AND id = $2 RETURNING account_id",
    )
    .bind(t.as_uuid())
    .bind(id)
    .fetch_one(&mut *tx)
    .await?;
    if let Some(acc) = account {
        sqlx::query(
            "UPDATE customer_sessions SET revoked_at = now()
             WHERE tenant_id = $1 AND account_id = $2 AND revoked_at IS NULL",
        )
        .bind(t.as_uuid())
        .bind(acc)
        .execute(&mut *tx)
        .await?;
    }
    audit(
        &mut tx,
        Some(actor),
        t,
        CUSTOMER_DELETION_REQUESTED,
        id,
        serde_json::json!({}),
        ctx,
    )
    .await?;
    tx.commit().await?;
    Ok(())
}

/// Export data customer (portabilitas). Diaudit tanpa isi.
pub async fn export(
    pool: &PgPool,
    actor: &Actor,
    id: Uuid,
    ctx: Ctx<'_>,
) -> Result<Customer, CrmError> {
    let t = actor.tenant_id;
    let mut tx = tenant_tx(pool, t).await?;
    let c = get_in(&mut tx, t, id).await?.ok_or(CrmError::NotFound)?;
    audit(
        &mut tx,
        Some(actor),
        t,
        CUSTOMER_EXPORTED,
        id,
        serde_json::json!({}),
        ctx,
    )
    .await?;
    tx.commit().await?;
    Ok(c)
}

/// Job retensi (ADR-0012): anonimkan customer yang diminta hapus > grace.
pub async fn purge(pool: &PgPool, grace_secs: i64) -> Result<i64, sqlx::Error> {
    sqlx::query_scalar("SELECT app.purge_customers(make_interval(secs => $1))")
        .bind(grace_secs as f64)
        .fetch_one(pool)
        .await
}
