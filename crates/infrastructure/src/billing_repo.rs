//! Paket & langganan (P7-1, ADR-0016). `plans` = katalog platform (hanya-baca untuk aplikasi);
//! `subscriptions` = per tenant (RLS), ditulis hanya oleh fungsi/peran platform.

use sqlx::{PgPool, Row};
use umkm_domain::billing::{LimitsError, PlanLimits, SubscriptionStatus, effective, parse_limits};
use umkm_shared::ids::TenantId;

use crate::pgx::tenant_tx;

#[derive(Debug, thiserror::Error)]
pub enum BillingError {
    #[error("langganan tenant tidak ada")]
    NoSubscription,
    #[error("konfigurasi paket rusak: {0}")]
    BadPlan(#[from] LimitsError),
    #[error(transparent)]
    Db(#[from] sqlx::Error),
}

#[derive(Debug, Clone)]
pub struct Plan {
    pub code: String,
    pub name: String,
    pub price_idr: i32,
    pub period_days: i32,
    pub trial_days: Option<i32>,
    pub limits: PlanLimits,
}

fn plan_of(r: &sqlx::postgres::PgRow) -> Result<Plan, BillingError> {
    Ok(Plan {
        code: r.get("code"),
        name: r.get("name"),
        price_idr: r.get("price_idr"),
        period_days: r.get("period_days"),
        trial_days: r.get("trial_days"),
        limits: parse_limits(&r.get::<serde_json::Value, _>("limits_json"))?,
    })
}

/// Paket aktif (urutan tampil). Dibaca langsung tiap permintaan — perubahan data berlaku tanpa deploy.
pub async fn plans(pool: &PgPool) -> Result<Vec<Plan>, BillingError> {
    sqlx::query(
        "SELECT code, name, price_idr, period_days, trial_days, limits_json FROM plans WHERE active ORDER BY position, code",
    )
    .fetch_all(pool)
    .await?
    .iter()
    .map(plan_of)
    .collect()
}

#[derive(Debug, Clone)]
pub struct TenantPlan {
    pub plan: Plan,
    /// Status tersimpan.
    pub stored_status: SubscriptionStatus,
    /// Status efektif saat ini (uji coba/periode lewat = expired).
    pub status: SubscriptionStatus,
    pub trial_ends_at: Option<String>,
    pub current_period_end: Option<String>,
}

const TS: &str = "'YYYY-MM-DD\"T\"HH24:MI:SS.US\"Z\"'";

/// Paket + status langganan tenant.
pub async fn current(pool: &PgPool, t: TenantId) -> Result<TenantPlan, BillingError> {
    let mut tx = tenant_tx(pool, t).await?;
    let r = sqlx::query(sqlx::AssertSqlSafe(format!(
        "SELECT p.code, p.name, p.price_idr, p.period_days, p.trial_days, p.limits_json, s.status,
                extract(epoch FROM s.trial_ends_at)::bigint AS trial_epoch,
                extract(epoch FROM s.current_period_end)::bigint AS period_epoch,
                extract(epoch FROM now())::bigint AS now_epoch,
                to_char(s.trial_ends_at AT TIME ZONE 'UTC', {TS}) AS trial_ends_at,
                to_char(s.current_period_end AT TIME ZONE 'UTC', {TS}) AS current_period_end
           FROM subscriptions s JOIN plans p ON p.code = s.plan_code WHERE s.tenant_id = $1"
    )))
    .bind(t.as_uuid())
    .fetch_optional(&mut *tx)
    .await?
    .ok_or(BillingError::NoSubscription)?;
    tx.commit().await?;
    let stored = SubscriptionStatus::parse(r.get::<&str, _>("status"))
        .unwrap_or(SubscriptionStatus::Expired);
    Ok(TenantPlan {
        plan: plan_of(&r)?,
        stored_status: stored,
        status: effective(
            stored,
            r.get("trial_epoch"),
            r.get("period_epoch"),
            r.get("now_epoch"),
        ),
        trial_ends_at: r.get("trial_ends_at"),
        current_period_end: r.get("current_period_end"),
    })
}

/// Pemakaian tenant untuk batas paket (P7-2). Bulan = bulan kalender Asia/Jakarta.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Usage {
    pub active_members: i64,
    pub pending_invitations: i64,
    /// Produk selain `archived`.
    pub products: i64,
    pub storage_bytes: i64,
    /// Percakapan baru bulan ini (dari agregat P6-6, tidak berkurang saat purge).
    pub conversations_month: i64,
    /// Token AI platform bulan ini (giliran chat & uji coba).
    pub platform_tokens_month: i64,
}

pub async fn usage(pool: &PgPool, t: TenantId) -> Result<Usage, BillingError> {
    let mut tx = tenant_tx(pool, t).await?;
    let r = sqlx::query(
        "SELECT
           (SELECT count(*) FROM tenant_memberships WHERE tenant_id = $1 AND status = 'active') AS members,
           (SELECT count(*) FROM invitations WHERE tenant_id = $1 AND accepted_at IS NULL AND revoked_at IS NULL
                AND expires_at > now()) AS invites,
           (SELECT count(*) FROM products WHERE tenant_id = $1 AND status <> 'archived') AS products,
           (SELECT coalesce(sum(byte_size), 0)::bigint FROM stored_objects WHERE tenant_id = $1) AS storage,
           (SELECT coalesce(sum(started), 0)::bigint FROM conversation_daily_stats WHERE tenant_id = $1
                AND date_trunc('month', day) = date_trunc('month', app.jkt_day(now()))) AS conv,
           (SELECT coalesce(sum(prompt_tokens + completion_tokens), 0)::bigint FROM ai_runs WHERE tenant_id = $1
                AND source = 'platform'
                AND date_trunc('month', app.jkt_day(created_at)) = date_trunc('month', app.jkt_day(now()))) AS tokens",
    )
    .bind(t.as_uuid())
    .fetch_one(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(Usage {
        active_members: r.get("members"),
        pending_invitations: r.get("invites"),
        products: r.get("products"),
        storage_bytes: r.get("storage"),
        conversations_month: r.get("conv"),
        platform_tokens_month: r.get("tokens"),
    })
}

// ------------------------------------------------------------------ tagihan & pembayaran manual (P7-3)

pub type Ctx<'a> = (Option<&'a str>, Option<&'a str>);

#[derive(Debug, Clone)]
pub struct Invoice {
    pub id: uuid::Uuid,
    pub tenant_id: uuid::Uuid,
    pub code: String,
    pub plan_code: String,
    pub amount_idr: i32,
    pub period_days: i32,
    /// `awaiting_payment` | `submitted` | `paid` | `rejected` | `cancelled`
    pub status: String,
    pub has_proof: bool,
    pub rejection_reason: Option<String>,
    pub period_end: Option<String>,
    pub created_at: String,
    /// Hanya pada daftar platform.
    pub tenant_name: Option<String>,
    pub tenant_slug: Option<String>,
}

#[derive(Debug, thiserror::Error)]
pub enum InvoiceError {
    #[error("tidak ditemukan")]
    NotFound,
    #[error("paket tidak tersedia")]
    PlanUnavailable,
    #[error("masih ada tagihan menunggu verifikasi")]
    PendingExists,
    #[error("status tagihan tidak mengizinkan aksi ini")]
    InvalidState,
    #[error("acak tidak tersedia")]
    Random,
    #[error(transparent)]
    Db(#[from] sqlx::Error),
}

fn inv_err(e: sqlx::Error) -> InvoiceError {
    match crate::pgx::violation(&e) {
        Some((_, c)) if c == "ck_si_plan_active" => InvoiceError::PlanUnavailable,
        Some((s, _)) if s == "23503" => InvoiceError::PlanUnavailable,
        Some((_, c)) if c == "uq_si_open" => InvoiceError::PendingExists,
        Some((_, c)) if c == "ck_si_not_submitted" || c == "ck_si_platform_only" => {
            InvoiceError::InvalidState
        }
        Some((s, _)) if s == "P0002" => InvoiceError::NotFound,
        _ => InvoiceError::Db(e),
    }
}

fn inv_code() -> Result<String, InvoiceError> {
    const A: &[u8; 32] = b"ABCDEFGHJKLMNPQRSTUVWXYZ23456789";
    let mut b = [0u8; 10];
    getrandom::fill(&mut b).map_err(|_| InvoiceError::Random)?;
    Ok(format!(
        "INV-{}",
        b.iter()
            .map(|x| A[(x & 31) as usize] as char)
            .collect::<String>()
    ))
}

const INV_COLS: &str = "id, tenant_id, code, plan_code, amount_idr, period_days, status, proof_bytes IS NOT NULL AS has_proof,
    rejection_reason, to_char(period_end AT TIME ZONE 'UTC', 'YYYY-MM-DD\"T\"HH24:MI:SS.US\"Z\"') AS period_end,
    to_char(created_at AT TIME ZONE 'UTC', 'YYYY-MM-DD\"T\"HH24:MI:SS.US\"Z\"') AS created_at";

fn invoice_of(r: &sqlx::postgres::PgRow) -> Invoice {
    Invoice {
        id: r.get("id"),
        tenant_id: r.get("tenant_id"),
        code: r.get("code"),
        plan_code: r.get("plan_code"),
        amount_idr: r.get("amount_idr"),
        period_days: r.get("period_days"),
        status: r.get("status"),
        has_proof: r.get("has_proof"),
        rejection_reason: r.get("rejection_reason"),
        period_end: r.get("period_end"),
        created_at: r.get("created_at"),
        tenant_name: None,
        tenant_slug: None,
    }
}

/// Buat tagihan paket. Tagihan lama yang belum dibayar dibatalkan; yang sudah menunggu verifikasi → PendingExists.
/// Harga & periode diambil trigger dari katalog (klien tidak menentukan nominal).
pub async fn create_invoice(
    pool: &PgPool,
    actor: &umkm_domain::rbac::Actor,
    plan_code: &str,
    ctx: Ctx<'_>,
) -> Result<Invoice, InvoiceError> {
    let t = actor.tenant_id;
    let mut tx = tenant_tx(pool, t).await?;
    let submitted: Option<uuid::Uuid> = sqlx::query_scalar(
        "SELECT id FROM subscription_invoices WHERE tenant_id = $1 AND status = 'submitted'",
    )
    .bind(t.as_uuid())
    .fetch_optional(&mut *tx)
    .await?;
    if submitted.is_some() {
        return Err(InvoiceError::PendingExists);
    }
    sqlx::query("UPDATE subscription_invoices SET status = 'cancelled' WHERE tenant_id = $1 AND status = 'awaiting_payment'")
        .bind(t.as_uuid())
        .execute(&mut *tx)
        .await
        .map_err(inv_err)?;
    let r = sqlx::query(sqlx::AssertSqlSafe(format!(
        "INSERT INTO subscription_invoices (tenant_id, code, plan_code, amount_idr, period_days, created_by)
         VALUES ($1, $2, $3, 0, 1, $4) RETURNING {INV_COLS}"
    )))
    .bind(t.as_uuid())
    .bind(inv_code()?)
    .bind(plan_code)
    .bind(actor.user_id)
    .fetch_one(&mut *tx)
    .await
    .map_err(inv_err)?;
    let inv = invoice_of(&r);
    crate::audit_repo::record(
        &mut *tx,
        &umkm_domain::audit::AuditEvent::new(umkm_domain::audit::actions::SUBSCRIPTION_INVOICE_CREATED, "subscription_invoice")
            .by_user(actor.user_id)
            .in_tenant(t)
            .resource(inv.id)
            .request(ctx.0, ctx.1)
            .with_summary(serde_json::json!({ "code": inv.code, "plan": inv.plan_code, "amount_idr": inv.amount_idr })),
    )
    .await?;
    tx.commit().await?;
    Ok(inv)
}

pub async fn invoices(pool: &PgPool, t: TenantId) -> Result<Vec<Invoice>, InvoiceError> {
    let mut tx = tenant_tx(pool, t).await?;
    let rows = sqlx::query(sqlx::AssertSqlSafe(format!(
        "SELECT {INV_COLS} FROM subscription_invoices WHERE tenant_id = $1 ORDER BY created_at DESC LIMIT 50"
    )))
    .bind(t.as_uuid())
    .fetch_all(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(rows.iter().map(invoice_of).collect())
}

/// Unggah/ganti bukti bayar (awaiting_payment/submitted → submitted).
pub async fn submit_proof(
    pool: &PgPool,
    actor: &umkm_domain::rbac::Actor,
    id: uuid::Uuid,
    content_type: &str,
    bytes: &[u8],
    ctx: Ctx<'_>,
) -> Result<Invoice, InvoiceError> {
    use sha2::{Digest, Sha256};
    let t = actor.tenant_id;
    let mut tx = tenant_tx(pool, t).await?;
    let r = sqlx::query(sqlx::AssertSqlSafe(format!(
        "UPDATE subscription_invoices SET status = 'submitted', proof_content_type = $3, proof_bytes = $4,
                proof_sha256 = $5, submitted_at = now()
          WHERE tenant_id = $1 AND id = $2 AND status IN ('awaiting_payment', 'submitted') RETURNING {INV_COLS}"
    )))
    .bind(t.as_uuid())
    .bind(id)
    .bind(content_type)
    .bind(bytes)
    .bind(Sha256::digest(bytes).to_vec())
    .fetch_optional(&mut *tx)
    .await
    .map_err(inv_err)?;
    let Some(r) = r else {
        let exists: Option<uuid::Uuid> = sqlx::query_scalar(
            "SELECT id FROM subscription_invoices WHERE tenant_id = $1 AND id = $2",
        )
        .bind(t.as_uuid())
        .bind(id)
        .fetch_optional(&mut *tx)
        .await?;
        return Err(if exists.is_some() {
            InvoiceError::InvalidState
        } else {
            InvoiceError::NotFound
        });
    };
    let inv = invoice_of(&r);
    crate::audit_repo::record(
        &mut *tx,
        &umkm_domain::audit::AuditEvent::new(
            umkm_domain::audit::actions::SUBSCRIPTION_PROOF_SUBMITTED,
            "subscription_invoice",
        )
        .by_user(actor.user_id)
        .in_tenant(t)
        .resource(inv.id)
        .request(ctx.0, ctx.1)
        .with_summary(serde_json::json!({ "code": inv.code, "bytes": bytes.len() })),
    )
    .await?;
    tx.commit().await?;
    Ok(inv)
}

/// Batalkan tagihan yang belum diputus.
pub async fn cancel_invoice(
    pool: &PgPool,
    t: TenantId,
    id: uuid::Uuid,
) -> Result<(), InvoiceError> {
    let mut tx = tenant_tx(pool, t).await?;
    let n = sqlx::query(
        "UPDATE subscription_invoices SET status = 'cancelled' WHERE tenant_id = $1 AND id = $2
            AND status IN ('awaiting_payment', 'submitted')",
    )
    .bind(t.as_uuid())
    .bind(id)
    .execute(&mut *tx)
    .await
    .map_err(inv_err)?
    .rows_affected();
    tx.commit().await?;
    if n == 0 {
        return Err(InvoiceError::InvalidState);
    }
    Ok(())
}

/// Bukti bayar milik tenant (pemilik toko melihat unggahannya sendiri).
pub async fn proof(
    pool: &PgPool,
    t: TenantId,
    id: uuid::Uuid,
) -> Result<Option<(String, Vec<u8>)>, InvoiceError> {
    let mut tx = tenant_tx(pool, t).await?;
    let r = sqlx::query(
        "SELECT proof_content_type, proof_bytes FROM subscription_invoices WHERE tenant_id = $1 AND id = $2 AND proof_bytes IS NOT NULL",
    )
    .bind(t.as_uuid())
    .bind(id)
    .fetch_optional(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(r.map(|r| (r.get("proof_content_type"), r.get("proof_bytes"))))
}

// ---- jalur platform (lintas tenant lewat fungsi SECURITY DEFINER; otorisasi super_admin+MFA di API)

pub async fn platform_invoices(
    pool: &PgPool,
    status: Option<&str>,
) -> Result<Vec<Invoice>, InvoiceError> {
    let rows = sqlx::query(
        "SELECT id, tenant_id, tenant_name, tenant_slug, code, plan_code, amount_idr, period_days, status,
                rejection_reason, to_char(created_at AT TIME ZONE 'UTC', 'YYYY-MM-DD\"T\"HH24:MI:SS.US\"Z\"') AS created_at,
                to_char(submitted_at AT TIME ZONE 'UTC', 'YYYY-MM-DD\"T\"HH24:MI:SS.US\"Z\"') AS submitted_at
           FROM app.platform_list_invoices($1)",
    )
    .bind(status)
    .fetch_all(pool)
    .await?;
    Ok(rows
        .iter()
        .map(|r| Invoice {
            id: r.get("id"),
            tenant_id: r.get("tenant_id"),
            code: r.get("code"),
            plan_code: r.get("plan_code"),
            amount_idr: r.get("amount_idr"),
            period_days: r.get("period_days"),
            status: r.get("status"),
            has_proof: r.get::<Option<String>, _>("submitted_at").is_some(),
            rejection_reason: r.get("rejection_reason"),
            period_end: None,
            created_at: r.get("created_at"),
            tenant_name: r.get("tenant_name"),
            tenant_slug: r.get("tenant_slug"),
        })
        .collect())
}

/// Bukti bayar untuk verifikasi platform (+ audit akses).
pub async fn platform_proof(
    pool: &PgPool,
    admin: uuid::Uuid,
    id: uuid::Uuid,
    ctx: Ctx<'_>,
) -> Result<Option<(String, Vec<u8>)>, InvoiceError> {
    let mut tx = pool.begin().await?;
    let r = sqlx::query("SELECT content_type, bytes FROM app.platform_invoice_proof($1)")
        .bind(id)
        .fetch_optional(&mut *tx)
        .await?;
    if r.is_some() {
        crate::audit_repo::record(
            &mut *tx,
            &umkm_domain::audit::AuditEvent::new(
                umkm_domain::audit::actions::PLATFORM_PROOF_VIEWED,
                "subscription_invoice",
            )
            .by_platform(admin)
            .resource(id)
            .request(ctx.0, ctx.1),
        )
        .await?;
    }
    tx.commit().await?;
    Ok(r.map(|r| (r.get("content_type"), r.get("bytes"))))
}

/// Putuskan tagihan (atomik): status langganan, audit platform, dan notifikasi tenant dalam satu transaksi.
pub async fn platform_decide(
    pool: &PgPool,
    admin: uuid::Uuid,
    id: uuid::Uuid,
    approve: bool,
    reason: Option<&str>,
    ctx: Ctx<'_>,
) -> Result<(uuid::Uuid, Option<String>), InvoiceError> {
    let mut tx = pool.begin().await?;
    let r = sqlx::query(
        "SELECT tenant_id, status, to_char(period_end AT TIME ZONE 'UTC', 'YYYY-MM-DD\"T\"HH24:MI:SS.US\"Z\"') AS period_end
           FROM app.platform_decide_invoice($1, $2, $3, $4)",
    )
    .bind(id)
    .bind(admin)
    .bind(approve)
    .bind(reason)
    .fetch_one(&mut *tx)
    .await
    .map_err(inv_err)?;
    let tenant: uuid::Uuid = r.get("tenant_id");
    let period_end: Option<String> = r.get("period_end");
    let t = TenantId::from_uuid(tenant);
    crate::audit_repo::record(
        &mut *tx,
        &umkm_domain::audit::AuditEvent::new(
            if approve {
                umkm_domain::audit::actions::PLATFORM_INVOICE_APPROVED
            } else {
                umkm_domain::audit::actions::PLATFORM_INVOICE_REJECTED
            },
            "subscription_invoice",
        )
        .by_platform(admin)
        .in_tenant(t)
        .resource(id)
        .request(ctx.0, ctx.1)
        .with_summary(
            serde_json::json!({ "approved": approve, "period_end": period_end, "reason": reason }),
        ),
    )
    .await?;
    crate::outbox::enqueue(
        &mut *tx,
        &umkm_application::notify::OutboxEvent {
            tenant_id: Some(tenant),
            event_type: crate::notification_repo::SUBSCRIPTION_STATUS,
            payload: serde_json::json!({
                "status": if approve { "aktif" } else { "pembayaran ditolak" },
                "invoice_id": id,
            }),
            idempotency_key: format!("subscription_invoice:{id}"),
        },
    )
    .await?;
    tx.commit().await?;
    Ok((tenant, period_end))
}
