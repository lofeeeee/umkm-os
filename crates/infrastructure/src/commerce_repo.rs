//! Checkout + order (P3-5, `PRD.md §11.1`). Satu transaksi:
//! validasi cart → kunci katalog → hitung ulang harga di server → customer → order →
//! snapshot item → payment pending → reservasi stok (kunci baris urut varian) → event → outbox.
//! Tidak ada panggilan eksternal di dalam transaksi (QRIS statis tidak memanggil provider).

use sqlx::{PgPool, Row};
use umkm_application::notify::OutboxEvent;
use umkm_domain::{
    audit::AuditEvent,
    catalog::{Money, format_sen},
    commerce::{
        pricing::{CartError, CartLine, CatalogItem, quote},
        state::{Fulfillment, OrderStatus},
    },
    crm::Contact,
    rbac::Actor,
};
use umkm_shared::ids::TenantId;
use uuid::Uuid;

use crate::{
    inventory_repo::{self, InventoryError, ReservationLine},
    pgx::{Tx, tenant_tx, violation},
};

pub const COMMERCE_SETTINGS_CHANGED: &str = "commerce.settings_changed";
pub const ORDER_CREATED_EVENT: &str = umkm_application::notify::events::ORDER_CREATED;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Settings {
    pub qris_object_id: Option<Uuid>,
    pub qris_receiver_name: Option<String>,
    pub checkout_enabled: bool,
    pub delivery_fee: Money,
    pub order_approval_required: bool,
    pub reservation_ttl_minutes: i32,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            qris_object_id: None,
            qris_receiver_name: None,
            checkout_enabled: false,
            delivery_fee: Money::ZERO,
            order_approval_required: false,
            reservation_ttl_minutes: 60,
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum SettingsError {
    #[error("QRIS wajib diatur sebelum checkout diaktifkan")]
    QrisRequired,
    #[error("gambar QRIS bukan milik tenant")]
    InvalidImage,
    #[error(transparent)]
    Db(#[from] sqlx::Error),
}

#[derive(Debug, Clone)]
pub struct CheckoutInput {
    /// Dibuat pemanggil agar token akses dapat diturunkan sebelum insert.
    pub order_id: Uuid,
    pub access_token_hash: [u8; 32],
    pub idempotency_key: String,
    pub request_hash: [u8; 32],
    pub cart: Vec<CartLine>,
    pub fulfillment: Fulfillment,
    pub contact: Contact,
    pub delivery_address: Option<String>,
    pub note: Option<String>,
    /// Customer login (akun toko ini), bila ada.
    pub account_id: Option<Uuid>,
    pub privacy_version: String,
    pub terms_version: String,
}

#[derive(Debug, thiserror::Error)]
pub enum CheckoutError {
    #[error("toko belum mengaktifkan checkout / QRIS")]
    NotConfigured,
    #[error("metode pengambilan tidak tersedia")]
    FulfillmentUnavailable,
    #[error(transparent)]
    Cart(#[from] CartError),
    #[error(transparent)]
    Stock(InventoryError),
    #[error("idempotency key dipakai untuk request lain")]
    IdempotencyMismatch,
    #[error(transparent)]
    Db(#[from] sqlx::Error),
}

impl From<InventoryError> for CheckoutError {
    fn from(e: InventoryError) -> Self {
        match e {
            InventoryError::Db(e) => Self::Db(e),
            other => Self::Stock(other),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Placed {
    pub order_id: Uuid,
    /// `true` = request ulang dengan kunci & isi sama (tidak membuat order baru)
    pub replay: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OrderItem {
    pub product_id: Option<Uuid>,
    pub variant_id: Option<Uuid>,
    pub product_name: String,
    pub variant_name: String,
    pub sku: String,
    pub unit_price: String,
    pub quantity: i64,
    pub line_total: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Payment {
    pub id: Uuid,
    pub method: String,
    pub status: String,
    pub amount: String,
    pub amount_received: Option<String>,
    pub payer_reference: Option<String>,
    pub proof_object_id: Option<Uuid>,
    pub verifier_note: Option<String>,
    pub rejection_reason: Option<String>,
    pub claimed_at: Option<String>,
    pub decided_at: Option<String>,
    pub decided_by: Option<Uuid>,
    pub refunded: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OrderEvent {
    pub from_status: Option<String>,
    pub to_status: String,
    pub actor_type: String,
    pub actor_user_id: Option<Uuid>,
    pub reason: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Order {
    pub id: Uuid,
    pub code: String,
    pub customer_id: Uuid,
    pub status: OrderStatus,
    pub fulfillment: Fulfillment,
    pub customer_name: Option<String>,
    pub customer_email: Option<String>,
    pub customer_phone: Option<String>,
    pub delivery_address: Option<String>,
    pub customer_note: Option<String>,
    pub subtotal: String,
    pub delivery_fee: String,
    pub total: String,
    pub total_sen: i64,
    pub created_at: String,
    pub expires_at: String,
    pub items: Vec<OrderItem>,
    pub payment: Option<Payment>,
    pub events: Vec<OrderEvent>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OrderSummary {
    pub id: Uuid,
    pub code: String,
    pub status: String,
    pub fulfillment: String,
    pub customer_id: Uuid,
    pub customer_name: Option<String>,
    pub total: String,
    pub payment_status: Option<String>,
    pub created_at: String,
}

const TS: &str = "'YYYY-MM-DD\"T\"HH24:MI:SS.US\"Z\"'";

fn ts(col: &str) -> String {
    format!("to_char({col} AT TIME ZONE 'UTC', {TS})")
}

// ------------------------------------------------------------------ settings

pub async fn settings_in(tx: &mut Tx, t: TenantId) -> Result<Settings, sqlx::Error> {
    let r = sqlx::query(
        "SELECT qris_object_id, qris_receiver_name, checkout_enabled, (delivery_fee * 100)::bigint AS fee,
                order_approval_required, reservation_ttl_minutes
         FROM commerce_settings WHERE tenant_id = $1",
    )
    .bind(t.as_uuid())
    .fetch_optional(&mut **tx)
    .await?;
    Ok(r.map(|r| Settings {
        qris_object_id: r.get("qris_object_id"),
        qris_receiver_name: r.get("qris_receiver_name"),
        checkout_enabled: r.get("checkout_enabled"),
        delivery_fee: Money::from_sen(r.get("fee")).unwrap_or(Money::ZERO),
        order_approval_required: r.get("order_approval_required"),
        reservation_ttl_minutes: r.get("reservation_ttl_minutes"),
    })
    .unwrap_or_default())
}

pub async fn settings(pool: &PgPool, t: TenantId) -> Result<Settings, sqlx::Error> {
    let mut tx = tenant_tx(pool, t).await?;
    let s = settings_in(&mut tx, t).await?;
    tx.commit().await?;
    Ok(s)
}

/// Simpan pengaturan (`tenant.update`). Pergantian QRIS diaudit before/after (ADR-0004).
pub async fn put_settings(
    pool: &PgPool,
    actor: &Actor,
    s: &Settings,
    (request_id, ip): (Option<&str>, Option<&str>),
) -> Result<(), SettingsError> {
    let t = actor.tenant_id;
    let mut tx = tenant_tx(pool, t).await?;
    let before = settings_in(&mut tx, t).await?;
    sqlx::query(
        "INSERT INTO commerce_settings (tenant_id, qris_object_id, qris_receiver_name, checkout_enabled,
             delivery_fee, order_approval_required, reservation_ttl_minutes)
         VALUES ($1, $2, $3, $4, $5::bigint::numeric / 100, $6, $7)
         ON CONFLICT (tenant_id) DO UPDATE SET qris_object_id = EXCLUDED.qris_object_id,
             qris_receiver_name = EXCLUDED.qris_receiver_name, checkout_enabled = EXCLUDED.checkout_enabled,
             delivery_fee = EXCLUDED.delivery_fee, order_approval_required = EXCLUDED.order_approval_required,
             reservation_ttl_minutes = EXCLUDED.reservation_ttl_minutes, updated_at = now()",
    )
    .bind(t.as_uuid())
    .bind(s.qris_object_id)
    .bind(&s.qris_receiver_name)
    .bind(s.checkout_enabled)
    .bind(s.delivery_fee.sen())
    .bind(s.order_approval_required)
    .bind(s.reservation_ttl_minutes)
    .execute(&mut *tx)
    .await
    .map_err(|e| match violation(&e) {
        Some((c, n)) if c == "23514" && n == "ck_commerce_settings_enabled" => SettingsError::QrisRequired,
        Some((c, n)) if c == "23503" && n == "fk_commerce_settings_qris" => SettingsError::InvalidImage,
        _ => SettingsError::Db(e),
    })?;
    crate::audit_repo::record(
        &mut *tx,
        &AuditEvent::new(COMMERCE_SETTINGS_CHANGED, "commerce_settings")
            .by_user(actor.user_id)
            .in_tenant(t)
            .resource(t)
            .request(request_id, ip)
            .with_summary(serde_json::json!({
                "qris_changed": before.qris_object_id != s.qris_object_id,
                "qris_before": before.qris_object_id,
                "qris_after": s.qris_object_id,
                "checkout_enabled": s.checkout_enabled,
                "order_approval_required": s.order_approval_required,
            })),
    )
    .await?;
    tx.commit().await?;
    Ok(())
}

// ------------------------------------------------------------------ checkout

async fn find_by_key(
    tx: &mut Tx,
    t: TenantId,
    key: &str,
    hash: &[u8; 32],
) -> Result<Option<Placed>, CheckoutError> {
    let r = sqlx::query(
        "SELECT id, request_hash FROM orders WHERE tenant_id = $1 AND idempotency_key = $2",
    )
    .bind(t.as_uuid())
    .bind(key)
    .fetch_optional(&mut **tx)
    .await?;
    match r {
        None => Ok(None),
        Some(r) if r.get::<Vec<u8>, _>("request_hash") == hash.as_slice() => Ok(Some(Placed {
            order_id: r.get("id"),
            replay: true,
        })),
        Some(_) => Err(CheckoutError::IdempotencyMismatch),
    }
}

/// Katalog untuk varian di cart, dikunci `FOR SHARE` (harga tidak berubah selama checkout).
async fn catalog(tx: &mut Tx, t: TenantId, ids: &[Uuid]) -> Result<Vec<CatalogItem>, sqlx::Error> {
    let rows = sqlx::query(
        "SELECT v.id, v.product_id, p.name AS product_name, v.name AS variant_name, v.sku,
                (COALESCE(v.promo_price, v.price) * 100)::bigint AS price_sen,
                (p.status = 'active' AND p.available AND v.is_active) AS sellable
         FROM product_variants v JOIN products p ON p.tenant_id = v.tenant_id AND p.id = v.product_id
         WHERE v.tenant_id = $1 AND v.id = ANY($2)
         ORDER BY v.id FOR SHARE OF v, p",
    )
    .bind(t.as_uuid())
    .bind(ids)
    .fetch_all(&mut **tx)
    .await?;
    Ok(rows
        .iter()
        .map(|r| CatalogItem {
            variant_id: r.get("id"),
            product_id: r.get("product_id"),
            product_name: r.get("product_name"),
            variant_name: r.get("variant_name"),
            sku: r.get("sku"),
            unit_price: Money::from_sen(r.get("price_sen")).unwrap_or(Money::ZERO),
            sellable: r.get("sellable"),
        })
        .collect())
}

fn order_code(id: Uuid) -> String {
    format!("ORD-{}", id.simple().to_string()[..10].to_uppercase())
}

async fn place(pool: &PgPool, t: TenantId, i: &CheckoutInput) -> Result<Placed, CheckoutError> {
    let mut tx = tenant_tx(pool, t).await?;
    if let Some(p) = find_by_key(&mut tx, t, &i.idempotency_key, &i.request_hash).await? {
        tx.commit().await?;
        return Ok(p);
    }
    let s = settings_in(&mut tx, t).await?;
    if !s.checkout_enabled {
        return Err(CheckoutError::NotConfigured);
    }
    let (pickup, delivery): (bool, bool) = sqlx::query_as(
        "SELECT pickup_available, delivery_available FROM business_profiles WHERE tenant_id = $1",
    )
    .bind(t.as_uuid())
    .fetch_one(&mut *tx)
    .await?;
    let ok = match i.fulfillment {
        Fulfillment::Pickup => pickup,
        Fulfillment::Delivery => delivery && i.delivery_address.is_some(),
    };
    if !ok {
        return Err(CheckoutError::FulfillmentUnavailable);
    }
    let ids: Vec<Uuid> = i.cart.iter().map(|l| l.variant_id).collect();
    let items = catalog(&mut tx, t, &ids).await?;
    let fee = match i.fulfillment {
        Fulfillment::Delivery => s.delivery_fee,
        Fulfillment::Pickup => Money::ZERO,
    };
    let q = quote(&i.cart, &items, fee)?;

    let customer = crate::crm_repo::create_in(&mut tx, t, &i.contact, i.account_id).await?;
    let id = i.order_id;
    let ttl = i64::from(s.reservation_ttl_minutes) * 60;
    sqlx::query(
        "INSERT INTO orders (id, tenant_id, code, customer_id, status, fulfillment, customer_name, customer_email,
             customer_phone, delivery_address, customer_note, subtotal, delivery_fee, total,
             access_token_hash, idempotency_key, request_hash, privacy_version, terms_version, consented_at, expires_at)
         VALUES ($1, $2, $3, $4, 'AWAITING_PAYMENT', $5, $6, $7, $8, $9, $10,
             $11::bigint::numeric / 100, $12::bigint::numeric / 100, $13::bigint::numeric / 100,
             $14, $15, $16, $17, $18, now(), now() + make_interval(secs => $19))",
    )
    .bind(id)
    .bind(t.as_uuid())
    .bind(order_code(id))
    .bind(customer)
    .bind(i.fulfillment.as_str())
    .bind(&i.contact.name)
    .bind(&i.contact.email)
    .bind(&i.contact.phone)
    .bind(&i.delivery_address)
    .bind(&i.note)
    .bind(q.subtotal_sen)
    .bind(q.delivery_fee_sen)
    .bind(q.total_sen)
    .bind(&i.access_token_hash[..])
    .bind(&i.idempotency_key)
    .bind(&i.request_hash[..])
    .bind(&i.privacy_version)
    .bind(&i.terms_version)
    .bind(ttl as f64)
    .execute(&mut *tx)
    .await?;
    for (pos, l) in q.lines.iter().enumerate() {
        sqlx::query(
            "INSERT INTO order_items (tenant_id, order_id, position, product_id, variant_id, product_name,
                 variant_name, sku, unit_price, quantity, line_total)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9::bigint::numeric / 100, $10, $11::bigint::numeric / 100)",
        )
        .bind(t.as_uuid())
        .bind(id)
        .bind(i16::try_from(pos).unwrap_or(i16::MAX))
        .bind(l.item.product_id)
        .bind(l.item.variant_id)
        .bind(&l.item.product_name)
        .bind(&l.item.variant_name)
        .bind(&l.item.sku)
        .bind(l.item.unit_price.sen())
        .bind(i32::try_from(l.quantity).unwrap_or(i32::MAX))
        .bind(l.line_total_sen)
        .execute(&mut *tx)
        .await?;
    }
    sqlx::query("INSERT INTO payments (tenant_id, order_id, amount) VALUES ($1, $2, $3::bigint::numeric / 100)")
        .bind(t.as_uuid())
        .bind(id)
        .bind(q.total_sen)
        .execute(&mut *tx)
        .await?;
    let lines: Vec<ReservationLine> = q
        .lines
        .iter()
        .map(|l| ReservationLine {
            variant_id: l.item.variant_id,
            quantity: l.quantity,
        })
        .collect();
    inventory_repo::reserve_in(&mut tx, t, &lines, &reference(id), ttl).await?;
    event_in(
        &mut tx,
        t,
        id,
        None,
        OrderStatus::AwaitingPayment,
        "customer",
        None,
        None,
    )
    .await?;
    crate::outbox::enqueue(
        &mut *tx,
        &OutboxEvent {
            tenant_id: Some(*t.as_uuid()),
            event_type: ORDER_CREATED_EVENT,
            payload: serde_json::json!({ "order_id": id }),
            idempotency_key: format!("order_created:{id}"),
        },
    )
    .await?;
    tx.commit().await?;
    Ok(Placed {
        order_id: id,
        replay: false,
    })
}

/// AC-ORD-006: kunci sama dua kali/paralel → tepat 1 order. Pemenang balapan insert;
/// yang kalah (23505 pada kunci idempotensi) membaca order pemenang.
pub async fn checkout(
    pool: &PgPool,
    t: TenantId,
    i: &CheckoutInput,
) -> Result<Placed, CheckoutError> {
    match place(pool, t, i).await {
        Err(CheckoutError::Db(e))
            if violation(&e).is_some_and(|(c, n)| c == "23505" && n == "uq_orders_idempotency") =>
        {
            let mut tx = tenant_tx(pool, t).await?;
            let p = find_by_key(&mut tx, t, &i.idempotency_key, &i.request_hash).await?;
            tx.commit().await?;
            p.ok_or(CheckoutError::Db(e))
        }
        other => other,
    }
}

pub fn reference(order: Uuid) -> String {
    format!("order:{order}")
}

#[allow(clippy::too_many_arguments)]
pub async fn event_in(
    tx: &mut Tx,
    t: TenantId,
    order: Uuid,
    from: Option<OrderStatus>,
    to: OrderStatus,
    actor_type: &str,
    actor: Option<Uuid>,
    reason: Option<&str>,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO order_events (tenant_id, order_id, from_status, to_status, actor_type, actor_user_id, reason)
         VALUES ($1, $2, $3, $4, $5, $6, $7)",
    )
    .bind(t.as_uuid())
    .bind(order)
    .bind(from.map(OrderStatus::as_str))
    .bind(to.as_str())
    .bind(actor_type)
    .bind(actor)
    .bind(reason)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

// ------------------------------------------------------------------ baca

fn order_cols() -> String {
    format!(
        "o.id, o.code, o.customer_id, o.status, o.fulfillment, o.customer_name, o.customer_email, o.customer_phone,
         o.delivery_address, o.customer_note, (o.subtotal * 100)::bigint AS subtotal_sen,
         (o.delivery_fee * 100)::bigint AS fee_sen, (o.total * 100)::bigint AS total_sen,
         {} AS created_at, {} AS expires_at",
        ts("o.created_at"),
        ts("o.expires_at")
    )
}

/// Detail order. `with_events` hanya untuk staf.
pub async fn detail_in(
    tx: &mut Tx,
    t: TenantId,
    filter: OrderKey<'_>,
    with_events: bool,
) -> Result<Option<Order>, sqlx::Error> {
    let (cond, id_bind, hash_bind): (&str, Option<Uuid>, Option<&[u8]>) = match filter {
        OrderKey::Id(id) => ("o.id = $2", Some(id), None),
        OrderKey::TokenHash(h) => ("o.access_token_hash = $3", None, Some(h)),
    };
    let Some(r) = sqlx::query(sqlx::AssertSqlSafe(format!(
        "SELECT {} FROM orders o WHERE o.tenant_id = $1 AND {cond}",
        order_cols()
    )))
    .bind(t.as_uuid())
    .bind(id_bind)
    .bind(hash_bind)
    .fetch_optional(&mut **tx)
    .await?
    else {
        return Ok(None);
    };
    let id: Uuid = r.get("id");
    let items = sqlx::query(
        "SELECT product_id, variant_id, product_name, variant_name, sku, (unit_price * 100)::bigint AS unit_sen,
                quantity, (line_total * 100)::bigint AS line_sen
         FROM order_items WHERE tenant_id = $1 AND order_id = $2 ORDER BY position",
    )
    .bind(t.as_uuid())
    .bind(id)
    .fetch_all(&mut **tx)
    .await?
    .iter()
    .map(|i| OrderItem {
        product_id: i.get("product_id"),
        variant_id: i.get("variant_id"),
        product_name: i.get("product_name"),
        variant_name: i.get("variant_name"),
        sku: i.get("sku"),
        unit_price: format_sen(i.get("unit_sen")),
        quantity: i64::from(i.get::<i32, _>("quantity")),
        line_total: format_sen(i.get("line_sen")),
    })
    .collect();
    let payment = sqlx::query(sqlx::AssertSqlSafe(format!(
        "SELECT p.id, p.method, p.status, (p.amount * 100)::bigint AS amount_sen,
                (p.amount_received * 100)::bigint AS received_sen, p.payer_reference, p.proof_object_id,
                p.verifier_note, p.rejection_reason, {} AS claimed_at, {} AS decided_at, p.decided_by,
                (SELECT COALESCE(sum(amount), 0) * 100 FROM refunds f WHERE f.tenant_id = p.tenant_id AND f.payment_id = p.id)::bigint AS refunded_sen
         FROM payments p WHERE p.tenant_id = $1 AND p.order_id = $2",
        ts("p.claimed_at"),
        ts("p.decided_at")
    )))
    .bind(t.as_uuid())
    .bind(id)
    .fetch_optional(&mut **tx)
    .await?
    .map(|p| Payment {
        id: p.get("id"),
        method: p.get("method"),
        status: p.get("status"),
        amount: format_sen(p.get("amount_sen")),
        amount_received: p.get::<Option<i64>, _>("received_sen").map(format_sen),
        payer_reference: p.get("payer_reference"),
        proof_object_id: p.get("proof_object_id"),
        verifier_note: p.get("verifier_note"),
        rejection_reason: p.get("rejection_reason"),
        claimed_at: p.get("claimed_at"),
        decided_at: p.get("decided_at"),
        decided_by: p.get("decided_by"),
        refunded: format_sen(p.get("refunded_sen")),
    });
    let events = if with_events {
        sqlx::query(sqlx::AssertSqlSafe(format!(
            "SELECT from_status, to_status, actor_type, actor_user_id, reason, {} AS created_at
             FROM order_events WHERE tenant_id = $1 AND order_id = $2 ORDER BY created_at, id",
            ts("created_at")
        )))
        .bind(t.as_uuid())
        .bind(id)
        .fetch_all(&mut **tx)
        .await?
        .iter()
        .map(|e| OrderEvent {
            from_status: e.get("from_status"),
            to_status: e.get("to_status"),
            actor_type: e.get("actor_type"),
            actor_user_id: e.get("actor_user_id"),
            reason: e.get("reason"),
            created_at: e.get("created_at"),
        })
        .collect()
    } else {
        vec![]
    };
    Ok(Some(Order {
        id,
        code: r.get("code"),
        customer_id: r.get("customer_id"),
        status: OrderStatus::parse(r.get("status")).unwrap_or(OrderStatus::Pending),
        fulfillment: Fulfillment::parse(r.get("fulfillment")).unwrap_or(Fulfillment::Pickup),
        customer_name: r.get("customer_name"),
        customer_email: r.get("customer_email"),
        customer_phone: r.get("customer_phone"),
        delivery_address: r.get("delivery_address"),
        customer_note: r.get("customer_note"),
        subtotal: format_sen(r.get("subtotal_sen")),
        delivery_fee: format_sen(r.get("fee_sen")),
        total: format_sen(r.get("total_sen")),
        total_sen: r.get("total_sen"),
        created_at: r.get("created_at"),
        expires_at: r.get("expires_at"),
        items,
        payment,
        events,
    }))
}

#[derive(Debug, Clone, Copy)]
pub enum OrderKey<'a> {
    Id(Uuid),
    TokenHash(&'a [u8]),
}

pub async fn detail(
    pool: &PgPool,
    t: TenantId,
    key: OrderKey<'_>,
    with_events: bool,
) -> Result<Option<Order>, sqlx::Error> {
    let mut tx = tenant_tx(pool, t).await?;
    let o = detail_in(&mut tx, t, key, with_events).await?;
    tx.commit().await?;
    Ok(o)
}

#[derive(Debug, Clone, Default)]
pub struct OrderFilter {
    pub status: Option<OrderStatus>,
    pub customer_id: Option<Uuid>,
    /// Hanya customer milik akun ini (riwayat customer)
    pub account_id: Option<Uuid>,
    pub limit: i64,
    pub offset: i64,
}

pub async fn list(
    pool: &PgPool,
    t: TenantId,
    f: &OrderFilter,
) -> Result<(Vec<OrderSummary>, i64), sqlx::Error> {
    let mut tx = tenant_tx(pool, t).await?;
    let w = "FROM orders o LEFT JOIN payments p ON p.tenant_id = o.tenant_id AND p.order_id = o.id
        WHERE o.tenant_id = $1 AND ($2::text IS NULL OR o.status = $2)
          AND ($3::uuid IS NULL OR o.customer_id = $3)
          AND ($4::uuid IS NULL OR o.customer_id IN
               (SELECT id FROM customers WHERE tenant_id = $1 AND account_id = $4))";
    let total: i64 = sqlx::query_scalar(sqlx::AssertSqlSafe(format!("SELECT count(*) {w}")))
        .bind(t.as_uuid())
        .bind(f.status.map(OrderStatus::as_str))
        .bind(f.customer_id)
        .bind(f.account_id)
        .fetch_one(&mut *tx)
        .await?;
    let rows = sqlx::query(sqlx::AssertSqlSafe(format!(
        "SELECT o.id, o.code, o.status, o.fulfillment, o.customer_id, o.customer_name,
                (o.total * 100)::bigint AS total_sen, p.status AS payment_status, {} AS created_at
         {w} ORDER BY o.created_at DESC, o.id LIMIT $5 OFFSET $6",
        ts("o.created_at")
    )))
    .bind(t.as_uuid())
    .bind(f.status.map(OrderStatus::as_str))
    .bind(f.customer_id)
    .bind(f.account_id)
    .bind(f.limit)
    .bind(f.offset)
    .fetch_all(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok((
        rows.iter()
            .map(|r| OrderSummary {
                id: r.get("id"),
                code: r.get("code"),
                status: r.get("status"),
                fulfillment: r.get("fulfillment"),
                customer_id: r.get("customer_id"),
                customer_name: r.get("customer_name"),
                total: format_sen(r.get("total_sen")),
                payment_status: r.get("payment_status"),
                created_at: r.get("created_at"),
            })
            .collect(),
        total,
    ))
}

/// Job worker: order AWAITING_PAYMENT yang kedaluwarsa dengan payment belum diklaim → EXPIRED,
/// reservasi dilepas (lintas tenant, SECURITY DEFINER).
pub async fn expire_orders(pool: &PgPool, batch: i32) -> Result<i32, sqlx::Error> {
    sqlx::query_scalar("SELECT app.expire_orders($1)")
        .bind(batch)
        .fetch_one(pool)
        .await
}
