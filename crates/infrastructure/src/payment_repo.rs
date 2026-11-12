//! Pembayaran QRIS statis, refund, transisi status manual, approval order (P3-7/P3-8,
//! ADR-0004, `PRD.md §4.7`). Setiap operasi: kunci baris order `FOR UPDATE` lebih dulu,
//! baru payment/stok → dua aksi paralel pada order yang sama diproses berurutan; yang kedua
//! melihat status baru dan ditolak (BR-008 versi manual: konfirmasi ganda diproses sekali).

use sqlx::{PgPool, Row};
use umkm_application::notify::{OutboxEvent, events};
use umkm_domain::{
    audit::AuditEvent,
    catalog::Money,
    commerce::{
        payment::{CLAIM_HOLD_SECS, PaymentError, PaymentStatus, check_confirm, check_refund},
        state::{Fulfillment, OrderStatus},
    },
    inventory::MovementKind,
    rbac::Actor,
    storage::{UploadError, inspect_image},
};
use umkm_shared::ids::TenantId;
use uuid::Uuid;

use crate::{
    commerce_repo::{event_in, reference, settings_in},
    inventory_repo::{self, InventoryError},
    pgx::{Tx, tenant_tx},
};

pub type Ctx<'a> = (Option<&'a str>, Option<&'a str>);

#[derive(Debug, thiserror::Error)]
pub enum FlowError {
    #[error("order tidak ditemukan")]
    NotFound,
    #[error("status order {0} tidak mengizinkan aksi ini")]
    WrongStatus(&'static str),
    #[error(transparent)]
    Payment(#[from] PaymentError),
    #[error("penyetuju tidak boleh pemohon (separation of duties)")]
    SelfApproval,
    #[error(transparent)]
    Upload(#[from] UploadError),
    #[error(transparent)]
    Stock(InventoryError),
    #[error(transparent)]
    Db(#[from] sqlx::Error),
}

impl From<InventoryError> for FlowError {
    fn from(e: InventoryError) -> Self {
        match e {
            InventoryError::Db(e) => Self::Db(e),
            other => Self::Stock(other),
        }
    }
}

struct Locked {
    id: Uuid,
    status: OrderStatus,
    fulfillment: Fulfillment,
    total_sen: i64,
}

enum Key<'a> {
    Id(Uuid),
    Token(&'a [u8]),
}

async fn lock_order(tx: &mut Tx, t: TenantId, key: Key<'_>) -> Result<Locked, FlowError> {
    let (cond, id, hash): (&str, Option<Uuid>, Option<&[u8]>) = match key {
        Key::Id(i) => ("id = $2", Some(i), None),
        Key::Token(h) => ("access_token_hash = $3", None, Some(h)),
    };
    let r = sqlx::query(sqlx::AssertSqlSafe(format!(
        "SELECT id, status, fulfillment, (total * 100)::bigint AS total_sen FROM orders
         WHERE tenant_id = $1 AND {cond} FOR UPDATE"
    )))
    .bind(t.as_uuid())
    .bind(id)
    .bind(hash)
    .fetch_optional(&mut **tx)
    .await?
    .ok_or(FlowError::NotFound)?;
    Ok(Locked {
        id: r.get("id"),
        status: OrderStatus::parse(r.get("status")).ok_or(FlowError::WrongStatus("UNKNOWN"))?,
        fulfillment: Fulfillment::parse(r.get("fulfillment")).unwrap_or(Fulfillment::Pickup),
        total_sen: r.get("total_sen"),
    })
}

struct LockedPayment {
    id: Uuid,
    status: PaymentStatus,
    received_sen: Option<i64>,
    refunded_sen: i64,
}

async fn lock_payment(tx: &mut Tx, t: TenantId, order: Uuid) -> Result<LockedPayment, FlowError> {
    let r = sqlx::query(
        "SELECT id, status, (amount_received * 100)::bigint AS received_sen,
                (SELECT COALESCE(sum(amount), 0) * 100 FROM refunds f WHERE f.tenant_id = p.tenant_id AND f.payment_id = p.id)::bigint AS refunded_sen
         FROM payments p WHERE tenant_id = $1 AND order_id = $2 FOR UPDATE",
    )
    .bind(t.as_uuid())
    .bind(order)
    .fetch_optional(&mut **tx)
    .await?
    .ok_or(FlowError::NotFound)?;
    Ok(LockedPayment {
        id: r.get("id"),
        status: PaymentStatus::parse(r.get("status")).unwrap_or(PaymentStatus::Expired),
        received_sen: r.get("received_sen"),
        refunded_sen: r.get("refunded_sen"),
    })
}

/// Ubah status (trigger DB memvalidasi ulang) + event + outbox notifikasi customer.
async fn transition(
    tx: &mut Tx,
    t: TenantId,
    o: &mut Locked,
    to: OrderStatus,
    actor: Option<Uuid>,
    reason: Option<&str>,
) -> Result<(), FlowError> {
    if !o.status.can_transition(to, o.fulfillment) {
        return Err(FlowError::WrongStatus(o.status.as_str()));
    }
    sqlx::query("UPDATE orders SET status = $3 WHERE tenant_id = $1 AND id = $2")
        .bind(t.as_uuid())
        .bind(o.id)
        .bind(to.as_str())
        .execute(&mut **tx)
        .await?;
    let actor_type = if actor.is_some() { "staff" } else { "system" };
    event_in(tx, t, o.id, Some(o.status), to, actor_type, actor, reason).await?;
    crate::outbox::enqueue(
        &mut **tx,
        &OutboxEvent {
            tenant_id: Some(*t.as_uuid()),
            event_type: events::ORDER_STATUS_CHANGED,
            payload: serde_json::json!({ "order_id": o.id, "status": to.as_str() }),
            idempotency_key: format!("order_status:{}:{}", o.id, to.as_str()),
        },
    )
    .await?;
    o.status = to;
    Ok(())
}

async fn audit(
    tx: &mut Tx,
    actor: &Actor,
    action: &'static str,
    order: Uuid,
    summary: serde_json::Value,
    (request_id, ip): Ctx<'_>,
) -> Result<(), sqlx::Error> {
    crate::audit_repo::record(
        &mut **tx,
        &AuditEvent::new(action, "order")
            .by_user(actor.user_id)
            .in_tenant(actor.tenant_id)
            .resource(order)
            .request(request_id, ip)
            .with_summary(summary),
    )
    .await
}

/// Reservasi milik order dibuat ulang bila sudah tidak aktif (mis. order PENDING staf).
async fn release(tx: &mut Tx, t: TenantId, order: Uuid) -> Result<(), FlowError> {
    inventory_repo::release_in(tx, t, &reference(order)).await?;
    Ok(())
}

/// Kembalikan stok item order (order ditolak / refund dengan restock).
async fn restock(
    tx: &mut Tx,
    t: TenantId,
    order: Uuid,
    actor: Uuid,
    reason: &str,
) -> Result<(), FlowError> {
    let items = sqlx::query(
        "SELECT variant_id, quantity FROM order_items
         WHERE tenant_id = $1 AND order_id = $2 AND variant_id IS NOT NULL ORDER BY variant_id",
    )
    .bind(t.as_uuid())
    .bind(order)
    .fetch_all(&mut **tx)
    .await?;
    for r in items {
        inventory_repo::movement_in(
            tx,
            t,
            r.get("variant_id"),
            MovementKind::StockIn,
            i64::from(r.get::<i32, _>("quantity")),
            reason,
            Some(&reference(order)),
            Some(actor),
        )
        .await?;
    }
    Ok(())
}

// ------------------------------------------------------------------ pembeli

/// Pembeli menandai sudah bayar (bukan bukti): payment `claimed`, batas order diperpanjang.
pub async fn claim(
    pool: &PgPool,
    t: TenantId,
    token_hash: &[u8],
    payer_reference: Option<&str>,
) -> Result<(), FlowError> {
    let mut tx = tenant_tx(pool, t).await?;
    let o = lock_order(&mut tx, t, Key::Token(token_hash)).await?;
    if o.status != OrderStatus::AwaitingPayment {
        return Err(FlowError::WrongStatus(o.status.as_str()));
    }
    let p = lock_payment(&mut tx, t, o.id).await?;
    if !p.status.open() {
        return Err(PaymentError::AlreadyDecided.into());
    }
    hold(&mut tx, t, o.id, p.id, payer_reference).await?;
    tx.commit().await?;
    Ok(())
}

async fn hold(
    tx: &mut Tx,
    t: TenantId,
    order: Uuid,
    payment: Uuid,
    payer_reference: Option<&str>,
) -> Result<(), FlowError> {
    sqlx::query(
        "UPDATE payments SET status = 'claimed', claimed_at = COALESCE(claimed_at, now()),
             payer_reference = COALESCE($3, payer_reference)
         WHERE tenant_id = $1 AND id = $2",
    )
    .bind(t.as_uuid())
    .bind(payment)
    .bind(payer_reference)
    .execute(&mut **tx)
    .await?;
    let hold = CLAIM_HOLD_SECS as f64;
    sqlx::query(
        "UPDATE orders SET expires_at = greatest(expires_at, now() + make_interval(secs => $3))
         WHERE tenant_id = $1 AND id = $2",
    )
    .bind(t.as_uuid())
    .bind(order)
    .bind(hold)
    .execute(&mut **tx)
    .await?;
    sqlx::query(
        "UPDATE inventory_reservations SET expires_at = greatest(expires_at, now() + make_interval(secs => $3))
         WHERE tenant_id = $1 AND reference = $2 AND status = 'active'",
    )
    .bind(t.as_uuid())
    .bind(reference(order))
    .bind(hold)
    .execute(&mut **tx)
    .await?;
    crate::outbox::enqueue(
        &mut **tx,
        &OutboxEvent {
            tenant_id: Some(*t.as_uuid()),
            event_type: events::PAYMENT_CLAIMED,
            payload: serde_json::json!({ "order_id": order }),
            idempotency_key: format!("payment_claimed:{payment}"),
        },
    )
    .await?;
    crate::audit_repo::record(
        &mut **tx,
        &AuditEvent::new(umkm_domain::audit::actions::PAYMENT_CLAIMED, "order")
            .in_tenant(t)
            .resource(order)
            .with_summary(serde_json::json!({ "payment_id": payment })),
    )
    .await?;
    Ok(())
}

/// Bukti bayar (1 per payment, menggantikan yang lama). Isi gambar diperiksa server.
pub async fn put_proof(
    pool: &PgPool,
    t: TenantId,
    token_hash: &[u8],
    bytes: &[u8],
    max_bytes: usize,
) -> Result<(), FlowError> {
    let ty = inspect_image(bytes, max_bytes)?;
    let mut tx = tenant_tx(pool, t).await?;
    let o = lock_order(&mut tx, t, Key::Token(token_hash)).await?;
    if o.status != OrderStatus::AwaitingPayment {
        return Err(FlowError::WrongStatus(o.status.as_str()));
    }
    let p = lock_payment(&mut tx, t, o.id).await?;
    if !p.status.open() {
        return Err(PaymentError::AlreadyDecided.into());
    }
    sqlx::query("DELETE FROM payment_proofs WHERE tenant_id = $1 AND payment_id = $2")
        .bind(t.as_uuid())
        .bind(p.id)
        .execute(&mut *tx)
        .await?;
    use sha2::Digest as _;
    sqlx::query(
        "INSERT INTO payment_proofs (tenant_id, payment_id, content_type, byte_size, sha256, bytes)
         VALUES ($1, $2, $3, $4, $5, $6)",
    )
    .bind(t.as_uuid())
    .bind(p.id)
    .bind(ty.content_type())
    .bind(i32::try_from(bytes.len()).unwrap_or(i32::MAX))
    .bind(sha2::Sha256::digest(bytes).as_slice())
    .bind(bytes)
    .execute(&mut *tx)
    .await?;
    hold(&mut tx, t, o.id, p.id, None).await?;
    tx.commit().await?;
    Ok(())
}

/// Bukti bayar untuk staf: `(content_type, bytes)`.
pub async fn proof(
    pool: &PgPool,
    t: TenantId,
    order: Uuid,
) -> Result<Option<(String, Vec<u8>)>, sqlx::Error> {
    let mut tx = tenant_tx(pool, t).await?;
    let r = sqlx::query(
        "SELECT pp.content_type, pp.bytes FROM payment_proofs pp
         JOIN payments p ON p.tenant_id = pp.tenant_id AND p.id = pp.payment_id
         WHERE pp.tenant_id = $1 AND p.order_id = $2",
    )
    .bind(t.as_uuid())
    .bind(order)
    .fetch_optional(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(r.map(|r| (r.get("content_type"), r.get("bytes"))))
}

// ------------------------------------------------------------------ staf: pembayaran

/// AC-ORD-007: konfirmasi pembayaran (`payments.verify`). Stok dipotong (reservasi → sale),
/// order → PAID → APPROVAL_PENDING (bila toko mewajibkan approval) atau PROCESSING.
pub async fn confirm(
    pool: &PgPool,
    actor: &Actor,
    order: Uuid,
    received: Money,
    note: Option<&str>,
    ctx: Ctx<'_>,
) -> Result<OrderStatus, FlowError> {
    let t = actor.tenant_id;
    let mut tx = tenant_tx(pool, t).await?;
    let mut o = lock_order(&mut tx, t, Key::Id(order)).await?;
    if o.status != OrderStatus::AwaitingPayment {
        return Err(FlowError::WrongStatus(o.status.as_str()));
    }
    let p = lock_payment(&mut tx, t, o.id).await?;
    let billed = Money::from_sen(o.total_sen).unwrap_or(Money::ZERO);
    check_confirm(p.status, billed, received, note)?;
    sqlx::query(
        "UPDATE payments SET status = 'confirmed', amount_received = $3::bigint::numeric / 100,
             verifier_note = $4, decided_at = now(), decided_by = $5
         WHERE tenant_id = $1 AND id = $2",
    )
    .bind(t.as_uuid())
    .bind(p.id)
    .bind(received.sen())
    .bind(note)
    .bind(actor.user_id)
    .execute(&mut *tx)
    .await?;
    inventory_repo::commit_in(&mut tx, t, &reference(o.id), Some(actor.user_id)).await?;
    transition(
        &mut tx,
        t,
        &mut o,
        OrderStatus::Paid,
        Some(actor.user_id),
        None,
    )
    .await?;
    let s = settings_in(&mut tx, t).await?;
    let next = if s.order_approval_required {
        OrderStatus::ApprovalPending
    } else {
        OrderStatus::Processing
    };
    transition(&mut tx, t, &mut o, next, Some(actor.user_id), None).await?;
    audit(
        &mut tx,
        actor,
        "payment.confirmed",
        o.id,
        serde_json::json!({
            "billed": billed.format(), "received": received.format(), "mismatch": received != billed,
        }),
        ctx,
    )
    .await?;
    tx.commit().await?;
    Ok(next)
}

/// Tolak klaim pembayaran (alasan wajib): order dibatalkan, reservasi dilepas.
pub async fn reject_payment(
    pool: &PgPool,
    actor: &Actor,
    order: Uuid,
    reason: &str,
    ctx: Ctx<'_>,
) -> Result<(), FlowError> {
    let t = actor.tenant_id;
    let mut tx = tenant_tx(pool, t).await?;
    let mut o = lock_order(&mut tx, t, Key::Id(order)).await?;
    if o.status != OrderStatus::AwaitingPayment {
        return Err(FlowError::WrongStatus(o.status.as_str()));
    }
    let p = lock_payment(&mut tx, t, o.id).await?;
    if !p.status.open() {
        return Err(PaymentError::AlreadyDecided.into());
    }
    sqlx::query(
        "UPDATE payments SET status = 'rejected', rejection_reason = $3, decided_at = now(), decided_by = $4
         WHERE tenant_id = $1 AND id = $2",
    )
    .bind(t.as_uuid())
    .bind(p.id)
    .bind(reason)
    .bind(actor.user_id)
    .execute(&mut *tx)
    .await?;
    release(&mut tx, t, o.id).await?;
    transition(
        &mut tx,
        t,
        &mut o,
        OrderStatus::Cancelled,
        Some(actor.user_id),
        Some(reason),
    )
    .await?;
    audit(
        &mut tx,
        actor,
        "payment.rejected",
        o.id,
        serde_json::json!({ "reason": reason }),
        ctx,
    )
    .await?;
    tx.commit().await?;
    Ok(())
}

/// AC-ORD-008: refund dicatat (uang dikembalikan pemilik di luar platform). Lunas → REFUNDED.
pub async fn refund(
    pool: &PgPool,
    actor: &Actor,
    order: Uuid,
    amount: Money,
    reason: &str,
    restock_items: bool,
    ctx: Ctx<'_>,
) -> Result<OrderStatus, FlowError> {
    let t = actor.tenant_id;
    let mut tx = tenant_tx(pool, t).await?;
    let mut o = lock_order(&mut tx, t, Key::Id(order)).await?;
    if !o.status.is_paid() {
        return Err(FlowError::WrongStatus(o.status.as_str()));
    }
    let p = lock_payment(&mut tx, t, o.id).await?;
    let received = p
        .received_sen
        .ok_or(FlowError::WrongStatus(o.status.as_str()))?;
    let full = check_refund(received, p.refunded_sen, amount)?;
    if full
        && !o
            .status
            .can_transition(OrderStatus::Refunded, o.fulfillment)
    {
        return Err(FlowError::WrongStatus(o.status.as_str()));
    }
    sqlx::query(
        "INSERT INTO refunds (tenant_id, payment_id, amount, reason, actor_user_id)
         VALUES ($1, $2, $3::bigint::numeric / 100, $4, $5)",
    )
    .bind(t.as_uuid())
    .bind(p.id)
    .bind(amount.sen())
    .bind(reason)
    .bind(actor.user_id)
    .execute(&mut *tx)
    .await?;
    // Order ditolak sudah mengembalikan stok saat REJECTED.
    let restocked = restock_items && o.status != OrderStatus::Rejected;
    if restocked {
        restock(
            &mut tx,
            t,
            o.id,
            actor.user_id,
            "Refund (stok dikembalikan)",
        )
        .await?;
    }
    if full {
        transition(
            &mut tx,
            t,
            &mut o,
            OrderStatus::Refunded,
            Some(actor.user_id),
            Some(reason),
        )
        .await?;
    }
    audit(
        &mut tx,
        actor,
        "payment.refunded",
        o.id,
        serde_json::json!({ "amount": amount.format(), "reason": reason, "full": full, "restock": restocked }),
        ctx,
    )
    .await?;
    tx.commit().await?;
    Ok(o.status)
}

// ------------------------------------------------------------------ staf: status & approval

/// Transisi manual (pemrosesan/pengiriman/selesai/batal belum bayar). Aliran lain punya
/// endpoint sendiri. Batal melepas reservasi.
pub async fn set_status(
    pool: &PgPool,
    actor: &Actor,
    order: Uuid,
    to: OrderStatus,
    reason: Option<&str>,
    ctx: Ctx<'_>,
) -> Result<(), FlowError> {
    if !to.manual_target() {
        return Err(FlowError::WrongStatus(to.as_str()));
    }
    let t = actor.tenant_id;
    let mut tx = tenant_tx(pool, t).await?;
    let mut o = lock_order(&mut tx, t, Key::Id(order)).await?;
    let from = o.status;
    if to == OrderStatus::Cancelled && o.status.can_transition(to, o.fulfillment) {
        release(&mut tx, t, o.id).await?;
        sqlx::query(
            "UPDATE payments SET status = 'expired'
             WHERE tenant_id = $1 AND order_id = $2 AND status IN ('pending', 'claimed')",
        )
        .bind(t.as_uuid())
        .bind(o.id)
        .execute(&mut *tx)
        .await?;
    }
    transition(&mut tx, t, &mut o, to, Some(actor.user_id), reason).await?;
    audit(
        &mut tx,
        actor,
        "order.status_changed",
        o.id,
        serde_json::json!({ "from": from.as_str(), "to": to.as_str() }),
        ctx,
    )
    .await?;
    tx.commit().await?;
    Ok(())
}

/// Pemohon approval = staf yang memasukkan order ke APPROVAL_PENDING (verifikator pembayaran).
async fn requester(tx: &mut Tx, t: TenantId, order: Uuid) -> Result<Option<Uuid>, sqlx::Error> {
    sqlx::query_scalar(
        "SELECT actor_user_id FROM order_events
         WHERE tenant_id = $1 AND order_id = $2 AND to_status = 'APPROVAL_PENDING'
         ORDER BY seq DESC LIMIT 1",
    )
    .bind(t.as_uuid())
    .bind(order)
    .fetch_optional(&mut **tx)
    .await
    .map(Option::flatten)
}

/// AC-ORD-005: approve / reject (alasan wajib) dengan separation-of-duties bila aktif.
pub async fn decide(
    pool: &PgPool,
    actor: &Actor,
    order: Uuid,
    approve: bool,
    reason: Option<&str>,
    ctx: Ctx<'_>,
) -> Result<(), FlowError> {
    let t = actor.tenant_id;
    let mut tx = tenant_tx(pool, t).await?;
    let mut o = lock_order(&mut tx, t, Key::Id(order)).await?;
    if o.status != OrderStatus::ApprovalPending {
        return Err(FlowError::WrongStatus(o.status.as_str()));
    }
    let s = settings_in(&mut tx, t).await?;
    if s.separation_of_duties && requester(&mut tx, t, o.id).await? == Some(actor.user_id) {
        return Err(FlowError::SelfApproval);
    }
    if approve {
        transition(
            &mut tx,
            t,
            &mut o,
            OrderStatus::Approved,
            Some(actor.user_id),
            reason,
        )
        .await?;
    } else {
        transition(
            &mut tx,
            t,
            &mut o,
            OrderStatus::Rejected,
            Some(actor.user_id),
            reason,
        )
        .await?;
        restock(&mut tx, t, o.id, actor.user_id, "Order ditolak").await?;
    }
    audit(
        &mut tx,
        actor,
        if approve {
            "order.approved"
        } else {
            "order.rejected"
        },
        o.id,
        serde_json::json!({ "reason": reason, "sod": s.separation_of_duties }),
        ctx,
    )
    .await?;
    tx.commit().await?;
    Ok(())
}
