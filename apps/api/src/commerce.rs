//! Checkout & order (P3-5, `PRD.md §4.7`, `PRD.md §11.1`, ADR-0004/0005).
//!
//! Publik (tenant dari Host): konfigurasi checkout, checkout (header `Idempotency-Key` wajib),
//! status order dengan header `X-Order-Token` (token tidak pernah di URL query → tidak masuk log).
//! Staf: pengaturan toko (`tenant.update`; baca `tenant.read`), daftar/detail order (`orders.read`).
//!
//! DTO checkout sengaja TIDAK `deny_unknown_fields`: field harga/total dari klien diabaikan
//! (AC-ORD-002), harga selalu dihitung ulang server.

use std::collections::BTreeSet;

use axum::{
    Json, Router,
    extract::{Path, Query, State},
    http::{HeaderMap, HeaderValue, StatusCode, header},
    response::{IntoResponse, Response},
    routing::{get, post},
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use umkm_domain::{
    auth::rate_rules,
    catalog::Money,
    commerce::{
        payment::PaymentError,
        pricing::{CartError, CartLine},
        state::{Fulfillment, OrderStatus},
    },
    crm,
};
use umkm_infrastructure::{
    auth::hash_token,
    commerce_repo::{
        self as repo, CheckoutError, CheckoutInput, Order, OrderFilter, OrderKey, OrderSummary,
        Settings, SettingsError,
    },
    inventory_repo::InventoryError,
    payment_repo::{self, FlowError},
};
use umkm_shared::{
    http::{ErrorBody, ErrorCode, SuccessEnvelope},
    ids::TenantId,
};
use utoipa::ToSchema;
use uuid::Uuid;

use crate::{
    ApiError, AppState,
    auth::ClientIp,
    customer_auth::{CustomerAuth, MaybeCustomer},
    http_util::{Page, asset_ref, conflict, internal, not_found, parse_asset_ref, pool},
    middleware::{HostTenant, ReqId},
    session::Member,
    validation::{field_errors, one},
    website::signed_assets,
};

pub const ORDER_TOKEN_HEADER: &str = "x-order-token";
pub const IDEMPOTENCY_HEADER: &str = "idempotency-key";

// ------------------------------------------------------------------ DTO

#[derive(Debug, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct SettingsDto {
    /// `asset:<uuid>` gambar QRIS merchant milik toko
    pub qris: Option<String>,
    pub qris_receiver_name: Option<String>,
    #[serde(default)]
    pub checkout_enabled: bool,
    /// Ongkir datar untuk delivery, desimal string
    #[serde(default = "zero")]
    pub delivery_fee: String,
    #[serde(default)]
    pub order_approval_required: bool,
    #[serde(default = "ttl")]
    pub reservation_ttl_minutes: i32,
    /// Penyetuju order ≠ staf yang mengonfirmasi pembayaran (default aktif)
    #[serde(default = "yes")]
    pub separation_of_duties: bool,
}

fn yes() -> bool {
    true
}

fn zero() -> String {
    "0".into()
}

fn ttl() -> i32 {
    60
}

#[derive(Debug, Serialize, ToSchema)]
pub struct CheckoutConfig {
    pub checkout_enabled: bool,
    pub pickup_available: bool,
    pub delivery_available: bool,
    pub delivery_fee: String,
    pub payment_method: String,
    pub privacy_version: String,
    pub terms_version: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct CheckoutItem {
    pub variant_id: Uuid,
    pub quantity: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct CheckoutRequest {
    pub items: Vec<CheckoutItem>,
    /// `pickup` | `delivery`
    pub fulfillment: String,
    pub name: String,
    pub email: Option<String>,
    pub phone: Option<String>,
    pub delivery_address: Option<String>,
    pub note: Option<String>,
    /// D-12: wajib `true`
    pub accept_privacy: bool,
    /// D-12: wajib `true`
    pub accept_terms: bool,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct OrderItemView {
    pub product_name: String,
    pub variant_name: String,
    pub sku: String,
    pub unit_price: String,
    pub quantity: i64,
    pub line_total: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct PublicPayment {
    pub method: String,
    /// `pending` | `claimed` | `confirmed` | `rejected` | `expired`
    pub status: String,
    pub amount: String,
    /// Signed URL gambar QRIS toko (hanya saat menunggu pembayaran)
    pub qris_url: Option<String>,
    pub receiver_name: Option<String>,
    pub rejection_reason: Option<String>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct PublicOrder {
    pub code: String,
    pub status: String,
    pub fulfillment: String,
    pub name: Option<String>,
    pub delivery_address: Option<String>,
    pub items: Vec<OrderItemView>,
    pub subtotal: String,
    pub delivery_fee: String,
    pub total: String,
    pub created_at: String,
    /// Batas pembayaran
    pub expires_at: String,
    pub payment: Option<PublicPayment>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct CheckoutResult {
    pub order: PublicOrder,
    /// Simpan di klien; dikirim di header `X-Order-Token` untuk melihat status
    pub access_token: String,
    /// `true` bila request ulang dengan Idempotency-Key yang sama
    pub replay: bool,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct OrderSummaryView {
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

#[derive(Debug, Serialize, ToSchema)]
pub struct OrderList {
    pub items: Vec<OrderSummaryView>,
    pub total: i64,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct PaymentView {
    pub id: Uuid,
    pub method: String,
    pub status: String,
    pub amount: String,
    pub amount_received: Option<String>,
    pub payer_reference: Option<String>,
    /// Ada bukti bayar dari pembeli (`GET .../payment/proof`)
    pub has_proof: bool,
    pub verifier_note: Option<String>,
    pub rejection_reason: Option<String>,
    pub claimed_at: Option<String>,
    pub decided_at: Option<String>,
    pub decided_by: Option<Uuid>,
    pub refunded: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct OrderEventView {
    pub from_status: Option<String>,
    pub to_status: String,
    pub actor_type: String,
    pub actor_user_id: Option<Uuid>,
    pub reason: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct OrderView {
    pub id: Uuid,
    pub code: String,
    pub customer_id: Uuid,
    pub status: String,
    pub fulfillment: String,
    pub customer_name: Option<String>,
    pub customer_email: Option<String>,
    pub customer_phone: Option<String>,
    pub delivery_address: Option<String>,
    pub customer_note: Option<String>,
    pub items: Vec<OrderItemView>,
    pub subtotal: String,
    pub delivery_fee: String,
    pub total: String,
    pub created_at: String,
    pub expires_at: String,
    pub payment: Option<PaymentView>,
    pub events: Vec<OrderEventView>,
}

#[derive(Debug, Deserialize, utoipa::IntoParams)]
pub struct OrderQuery {
    pub status: Option<String>,
    pub customer_id: Option<Uuid>,
    pub limit: Option<i64>,
    pub offset: Option<i64>,
}

// ------------------------------------------------------------------ konversi

fn settings_dto(s: Settings) -> SettingsDto {
    SettingsDto {
        qris: s.qris_object_id.map(asset_ref),
        qris_receiver_name: s.qris_receiver_name,
        checkout_enabled: s.checkout_enabled,
        delivery_fee: s.delivery_fee.format(),
        order_approval_required: s.order_approval_required,
        reservation_ttl_minutes: s.reservation_ttl_minutes,
        separation_of_duties: s.separation_of_duties,
    }
}

fn items_view(o: &Order) -> Vec<OrderItemView> {
    o.items
        .iter()
        .map(|i| OrderItemView {
            product_name: i.product_name.clone(),
            variant_name: i.variant_name.clone(),
            sku: i.sku.clone(),
            unit_price: i.unit_price.clone(),
            quantity: i.quantity,
            line_total: i.line_total.clone(),
        })
        .collect()
}

pub fn summary_view(s: OrderSummary) -> OrderSummaryView {
    OrderSummaryView {
        id: s.id,
        code: s.code,
        status: s.status,
        fulfillment: s.fulfillment,
        customer_id: s.customer_id,
        customer_name: s.customer_name,
        total: s.total,
        payment_status: s.payment_status,
        created_at: s.created_at,
    }
}

async fn signed_one(
    state: &AppState,
    t: TenantId,
    id: Option<Uuid>,
) -> Result<Option<String>, ApiError> {
    let Some(id) = id else { return Ok(None) };
    let m = signed_assets(state, t, BTreeSet::from([id])).await?;
    Ok(m.get(&asset_ref(id)).cloned())
}

pub async fn public_view(state: &AppState, t: TenantId, o: Order) -> Result<PublicOrder, ApiError> {
    let awaiting = o.status == OrderStatus::AwaitingPayment;
    let s = if awaiting {
        Some(repo::settings(pool(state)?, t).await.map_err(internal)?)
    } else {
        None
    };
    let qris_url = signed_one(state, t, s.as_ref().and_then(|s| s.qris_object_id)).await?;
    let items = items_view(&o);
    Ok(PublicOrder {
        code: o.code,
        status: o.status.as_str().into(),
        fulfillment: o.fulfillment.as_str().into(),
        name: o.customer_name,
        delivery_address: o.delivery_address,
        items,
        subtotal: o.subtotal,
        delivery_fee: o.delivery_fee,
        total: o.total,
        created_at: o.created_at,
        expires_at: o.expires_at,
        payment: o.payment.map(|p| PublicPayment {
            method: p.method,
            status: p.status,
            amount: p.amount,
            qris_url: qris_url.clone(),
            receiver_name: s.and_then(|s| s.qris_receiver_name),
            rejection_reason: p.rejection_reason,
        }),
    })
}

pub fn staff_view(o: Order) -> Result<OrderView, ApiError> {
    let items = items_view(&o);
    Ok(OrderView {
        id: o.id,
        code: o.code,
        customer_id: o.customer_id,
        status: o.status.as_str().into(),
        fulfillment: o.fulfillment.as_str().into(),
        customer_name: o.customer_name,
        customer_email: o.customer_email,
        customer_phone: o.customer_phone,
        delivery_address: o.delivery_address,
        customer_note: o.customer_note,
        items,
        subtotal: o.subtotal,
        delivery_fee: o.delivery_fee,
        total: o.total,
        created_at: o.created_at,
        expires_at: o.expires_at,
        payment: o.payment.map(|p| PaymentView {
            id: p.id,
            method: p.method,
            status: p.status,
            amount: p.amount,
            amount_received: p.amount_received,
            payer_reference: p.payer_reference,
            has_proof: p.has_proof,
            verifier_note: p.verifier_note,
            rejection_reason: p.rejection_reason,
            claimed_at: p.claimed_at,
            decided_at: p.decided_at,
            decided_by: p.decided_by,
            refunded: p.refunded,
        }),
        events: o
            .events
            .into_iter()
            .map(|e| OrderEventView {
                from_status: e.from_status,
                to_status: e.to_status,
                actor_type: e.actor_type,
                actor_user_id: e.actor_user_id,
                reason: e.reason,
                created_at: e.created_at,
            })
            .collect(),
    })
}

fn no_store(mut res: Response) -> Response {
    res.headers_mut().insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static("private, no-store"),
    );
    res
}

fn auth(state: &AppState) -> Result<&crate::auth::AuthService, ApiError> {
    state
        .auth
        .as_deref()
        .ok_or_else(|| ApiError::new(ErrorCode::ServiceUnavailable))
}

fn signer(state: &AppState) -> Result<&umkm_infrastructure::storage::UrlSigner, ApiError> {
    state
        .files
        .as_deref()
        .map(|f| &f.signer)
        .ok_or_else(|| ApiError::new(ErrorCode::ServiceUnavailable))
}

/// Token akses order: HMAC dari id order (stabil untuk replay idempoten).
pub fn order_token(state: &AppState, t: TenantId, order: Uuid) -> Result<String, ApiError> {
    Ok(signer(state)?.derive_token("order-access", t, order))
}

/// Origin wajib situs toko itu sendiri (sama dengan akun customer).
fn check_shop_origin(state: &AppState, h: &HeaderMap) -> Result<(), ApiError> {
    let Some(origin) = h.get(header::ORIGIN).and_then(|o| o.to_str().ok()) else {
        return Ok(());
    };
    let host = h
        .get(header::HOST)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    let insecure = auth(state).map(|a| !a.secure_cookies).unwrap_or(false);
    if origin == format!("https://{host}") || (insecure && origin == format!("http://{host}")) {
        Ok(())
    } else {
        Err(ApiError(
            ErrorBody::new(ErrorCode::Forbidden).with_message("Origin not allowed"),
        ))
    }
}

async fn rate_limit(state: &AppState, ip: &str) -> Result<(), ApiError> {
    let a = auth(state)?;
    match a.limiter.check(&rate_rules::CHECKOUT_PER_IP, ip).await {
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

fn checkout_err(e: CheckoutError) -> ApiError {
    match e {
        CheckoutError::NotConfigured => conflict(
            "payment_not_configured",
            "Toko belum menerima pesanan online",
        ),
        CheckoutError::FulfillmentUnavailable => one("fulfillment", "fulfillment_unavailable"),
        CheckoutError::Cart(CartError::Empty) => one("items", "required"),
        CheckoutError::Cart(CartError::TooManyLines) => one("items", "too_many"),
        CheckoutError::Cart(CartError::InvalidQuantity(i)) => {
            field_errors([(format!("items[{i}].quantity").as_str(), "invalid_quantity")])
        }
        CheckoutError::Cart(CartError::Unavailable(i)) => field_errors([(
            format!("items[{i}].variant_id").as_str(),
            "product_unavailable",
        )]),
        CheckoutError::Cart(CartError::Overflow) => one("items", "total_too_large"),
        CheckoutError::Stock(e) => crate::inventory::stock_err(e),
        CheckoutError::IdempotencyMismatch => conflict(
            "idempotency_key_reused",
            "Idempotency-Key sudah dipakai untuk pesanan dengan isi berbeda",
        ),
        CheckoutError::Db(e) => internal(e),
    }
}

// ------------------------------------------------------------------ staf: pengaturan

/// Pengaturan checkout toko.
#[utoipa::path(
    get, path = "/api/v1/tenants/{tenant_id}/commerce/settings", tag = "commerce",
    params(("tenant_id" = Uuid, Path)),
    responses((status = 200, description = "Pengaturan", body = SuccessEnvelope<SettingsDto>))
)]
pub async fn get_settings(
    State(state): State<AppState>,
    ReqId(id): ReqId,
    Member(actor): Member,
) -> Result<Json<SuccessEnvelope<SettingsDto>>, ApiError> {
    actor.require("tenant.read")?;
    let s = repo::settings(pool(&state)?, actor.tenant_id)
        .await
        .map_err(internal)?;
    Ok(Json(SuccessEnvelope::new(settings_dto(s), &id)))
}

/// Ubah pengaturan; mengganti QRIS hanya Owner (`tenant.update`) dan diaudit (ADR-0004).
#[utoipa::path(
    put, path = "/api/v1/tenants/{tenant_id}/commerce/settings", tag = "commerce",
    params(("tenant_id" = Uuid, Path)), request_body = SettingsDto,
    responses(
        (status = 200, description = "Tersimpan", body = SuccessEnvelope<SettingsDto>),
        (status = 422, description = "Validasi gagal / QRIS belum diatur", body = umkm_shared::http::ErrorEnvelope)
    )
)]
pub async fn put_settings(
    State(state): State<AppState>,
    ReqId(id): ReqId,
    ClientIp(ip): ClientIp,
    Member(actor): Member,
    Json(req): Json<SettingsDto>,
) -> Result<Json<SuccessEnvelope<SettingsDto>>, ApiError> {
    actor.require("tenant.update")?;
    let mut e: Vec<(String, &str)> = vec![];
    let qris = match req.qris.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
        None => None,
        Some(s) => {
            let v = parse_asset_ref(s);
            if v.is_none() {
                e.push(("qris".into(), "invalid_asset_ref"));
            }
            v
        }
    };
    let receiver = req
        .qris_receiver_name
        .map(|s| s.trim().to_owned())
        .filter(|s| !s.is_empty());
    if receiver
        .as_ref()
        .is_some_and(|r| r.chars().count() > 80 || r.chars().any(char::is_control))
    {
        e.push(("qris_receiver_name".into(), "invalid"));
    }
    let fee = Money::parse(req.delivery_fee.trim()).filter(|m| m.sen() <= 10_000_000 * 100);
    if fee.is_none() {
        e.push(("delivery_fee".into(), "invalid_price"));
    }
    if !(15..=1440).contains(&req.reservation_ttl_minutes) {
        e.push(("reservation_ttl_minutes".into(), "out_of_range"));
    }
    if req.checkout_enabled && (qris.is_none() || receiver.is_none()) {
        e.push(("qris".into(), "required_for_checkout"));
    }
    if !e.is_empty() {
        return Err(field_errors(e.iter().map(|(p, c)| (p.as_str(), *c))));
    }
    let s = Settings {
        qris_object_id: qris,
        qris_receiver_name: receiver,
        checkout_enabled: req.checkout_enabled,
        delivery_fee: fee.unwrap_or(Money::ZERO),
        order_approval_required: req.order_approval_required,
        reservation_ttl_minutes: req.reservation_ttl_minutes,
        separation_of_duties: req.separation_of_duties,
    };
    repo::put_settings(pool(&state)?, &actor, &s, (Some(id.as_str()), Some(&ip)))
        .await
        .map_err(|e| match e {
            SettingsError::QrisRequired => one("qris", "required_for_checkout"),
            SettingsError::InvalidImage => one("qris", "invalid_asset_ref"),
            SettingsError::Db(e) => internal(e),
        })?;
    Ok(Json(SuccessEnvelope::new(settings_dto(s), &id)))
}

// ------------------------------------------------------------------ staf: order

/// Daftar order (filter status/customer).
#[utoipa::path(
    get, path = "/api/v1/tenants/{tenant_id}/orders", tag = "commerce",
    params(("tenant_id" = Uuid, Path), OrderQuery),
    responses((status = 200, description = "Order", body = SuccessEnvelope<OrderList>))
)]
pub async fn list_orders(
    State(state): State<AppState>,
    ReqId(id): ReqId,
    Member(actor): Member,
    Query(q): Query<OrderQuery>,
) -> Result<Json<SuccessEnvelope<OrderList>>, ApiError> {
    actor.require("orders.read")?;
    let status = match q.status.as_deref() {
        None | Some("") => None,
        Some(s) => Some(OrderStatus::parse(s).ok_or_else(|| one("status", "invalid_status"))?),
    };
    let (limit, offset) = Page {
        limit: q.limit,
        offset: q.offset,
    }
    .bounds();
    let (list, total) = repo::list(
        pool(&state)?,
        actor.tenant_id,
        &OrderFilter {
            status,
            customer_id: q.customer_id,
            account_id: None,
            limit,
            offset,
        },
    )
    .await
    .map_err(internal)?;
    Ok(Json(SuccessEnvelope::new(
        OrderList {
            items: list.into_iter().map(summary_view).collect(),
            total,
        },
        &id,
    )))
}

/// Detail order + pembayaran + riwayat status.
#[utoipa::path(
    get, path = "/api/v1/tenants/{tenant_id}/orders/{order_id}", tag = "commerce",
    params(("tenant_id" = Uuid, Path), ("order_id" = Uuid, Path)),
    responses((status = 200, description = "Order", body = SuccessEnvelope<OrderView>))
)]
pub async fn get_order(
    State(state): State<AppState>,
    ReqId(id): ReqId,
    Member(actor): Member,
    Path((_, oid)): Path<(Uuid, Uuid)>,
) -> Result<Json<SuccessEnvelope<OrderView>>, ApiError> {
    actor.require("orders.read")?;
    let o = repo::detail(pool(&state)?, actor.tenant_id, OrderKey::Id(oid), true)
        .await
        .map_err(internal)?
        .ok_or_else(not_found)?;
    Ok(Json(SuccessEnvelope::new(staff_view(o)?, &id)))
}

// ------------------------------------------------------------------ publik

/// Konfigurasi checkout toko (tanpa data rahasia).
#[utoipa::path(
    get, path = "/api/v1/public/checkout-config", tag = "public",
    responses((status = 200, description = "Konfigurasi", body = SuccessEnvelope<CheckoutConfig>))
)]
pub async fn checkout_config(
    State(state): State<AppState>,
    ReqId(id): ReqId,
    HostTenant(t): HostTenant,
) -> Result<Response, ApiError> {
    let p = pool(&state)?;
    let s = repo::settings(p, t).await.map_err(internal)?;
    let info = umkm_infrastructure::business_repo::load(p, t)
        .await
        .map_err(internal)?
        .ok_or_else(not_found)?;
    let legal = &auth(&state)?.legal;
    Ok(crate::http_util::public_json(
        CheckoutConfig {
            checkout_enabled: s.checkout_enabled,
            pickup_available: info.profile.pickup_available,
            delivery_available: info.profile.delivery_available,
            delivery_fee: s.delivery_fee.format(),
            payment_method: "qris_static".into(),
            privacy_version: legal.privacy_policy.clone(),
            terms_version: legal.terms_of_service.clone(),
        },
        &id,
    ))
}

/// Buat order dari cart (AC-ORD-001/002/006/010). Header `Idempotency-Key` wajib.
#[utoipa::path(
    post, path = "/api/v1/public/checkout", tag = "public", request_body = CheckoutRequest,
    params(("Idempotency-Key" = String, Header, description = "8..100 karakter [A-Za-z0-9_-], unik per pesanan")),
    responses(
        (status = 201, description = "Order dibuat; bayar via QRIS toko", body = SuccessEnvelope<CheckoutResult>),
        (status = 200, description = "Replay idempoten (order yang sama)", body = SuccessEnvelope<CheckoutResult>),
        (status = 409, description = "Stok habis / toko belum aktif / kunci dipakai ulang", body = umkm_shared::http::ErrorEnvelope),
        (status = 422, description = "Validasi gagal", body = umkm_shared::http::ErrorEnvelope)
    )
)]
pub async fn checkout(
    State(state): State<AppState>,
    ReqId(id): ReqId,
    ClientIp(ip): ClientIp,
    HostTenant(t): HostTenant,
    MaybeCustomer(customer): MaybeCustomer,
    headers: HeaderMap,
    Json(req): Json<CheckoutRequest>,
) -> Result<Response, ApiError> {
    check_shop_origin(&state, &headers)?;
    let key = headers
        .get(IDEMPOTENCY_HEADER)
        .and_then(|v| v.to_str().ok())
        .map(str::trim)
        .filter(|k| {
            (8..=100).contains(&k.len())
                && k.bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
        })
        .ok_or_else(|| one("Idempotency-Key", "required"))?
        .to_owned();
    // Hash isi request (setelah parse; field asing tidak ikut) untuk deteksi kunci dipakai ulang.
    let request_hash: [u8; 32] = Sha256::digest(serde_json::to_vec(&req).map_err(internal)?).into();
    rate_limit(&state, &ip).await?;

    let mut e = vec![];
    let fulfillment = Fulfillment::parse(&req.fulfillment);
    if fulfillment.is_none() {
        umkm_domain::business::push(&mut e, "fulfillment", "invalid_fulfillment");
    }
    let contact = crm::validate_contact(
        &mut e,
        &req.name,
        req.email.as_deref(),
        req.phone.as_deref(),
    );
    let address = req
        .delivery_address
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_owned);
    if fulfillment == Some(Fulfillment::Delivery) && address.is_none() {
        umkm_domain::business::push(&mut e, "delivery_address", "required");
    }
    umkm_domain::business::plain_text(&mut e, "delivery_address", address.as_deref(), 500, false);
    let note = req
        .note
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_owned);
    umkm_domain::business::plain_text(&mut e, "note", note.as_deref(), 500, false);
    if !req.accept_privacy {
        umkm_domain::business::push(&mut e, "accept_privacy", "required");
    }
    if !req.accept_terms {
        umkm_domain::business::push(&mut e, "accept_terms", "required");
    }
    let (Some(fulfillment), Some(contact), true) = (fulfillment, contact, e.is_empty()) else {
        return Err(field_errors(e.iter().map(|x| (x.path.as_str(), x.code))));
    };

    let order_id = Uuid::new_v4();
    let token = order_token(&state, t, order_id)?;
    let legal = &auth(&state)?.legal;
    let input = CheckoutInput {
        order_id,
        access_token_hash: hash_token(&token).0,
        idempotency_key: key,
        request_hash,
        cart: req
            .items
            .iter()
            .map(|i| CartLine {
                variant_id: i.variant_id,
                quantity: i.quantity,
            })
            .collect(),
        fulfillment,
        contact,
        delivery_address: if fulfillment == Fulfillment::Delivery {
            address
        } else {
            None
        },
        note,
        account_id: customer.map(|c| c.session.account_id),
        privacy_version: legal.privacy_policy.clone(),
        terms_version: legal.terms_of_service.clone(),
    };
    let placed = repo::checkout(pool(&state)?, t, &input)
        .await
        .map_err(checkout_err)?;
    let o = repo::detail(pool(&state)?, t, OrderKey::Id(placed.order_id), false)
        .await
        .map_err(internal)?
        .ok_or_else(|| internal("order hilang setelah checkout"))?;
    let body = CheckoutResult {
        order: public_view(&state, t, o).await?,
        access_token: order_token(&state, t, placed.order_id)?,
        replay: placed.replay,
    };
    let status = if placed.replay {
        StatusCode::OK
    } else {
        StatusCode::CREATED
    };
    Ok(no_store(
        (status, Json(SuccessEnvelope::new(body, &id))).into_response(),
    ))
}

/// Order dari token akses (AC-ORD-009). Token lain/toko lain → 404.
pub async fn order_from_token(
    state: &AppState,
    t: TenantId,
    headers: &HeaderMap,
) -> Result<Order, ApiError> {
    let token = headers
        .get(ORDER_TOKEN_HEADER)
        .and_then(|v| v.to_str().ok())
        .map(str::trim)
        .filter(|s| !s.is_empty() && s.len() <= 128)
        .ok_or_else(not_found)?;
    repo::detail(
        pool(state)?,
        t,
        OrderKey::TokenHash(&hash_token(token).0),
        false,
    )
    .await
    .map_err(internal)?
    .ok_or_else(not_found)
}

/// Status order untuk pembeli (header `X-Order-Token`).
#[utoipa::path(
    get, path = "/api/v1/public/orders/current", tag = "public",
    params(("X-Order-Token" = String, Header)),
    responses(
        (status = 200, description = "Order", body = SuccessEnvelope<PublicOrder>),
        (status = 404, description = "Token tidak valid", body = umkm_shared::http::ErrorEnvelope)
    )
)]
pub async fn current_order(
    State(state): State<AppState>,
    ReqId(id): ReqId,
    HostTenant(t): HostTenant,
    headers: HeaderMap,
) -> Result<Response, ApiError> {
    let o = order_from_token(&state, t, &headers).await?;
    let v = public_view(&state, t, o).await?;
    Ok(no_store(Json(SuccessEnvelope::new(v, &id)).into_response()))
}

/// Riwayat order customer login (hanya customer tertaut akun; AC-CRM-003).
#[utoipa::path(
    get, path = "/api/v1/public/customer/orders", tag = "public", params(Page),
    responses((status = 200, description = "Order", body = SuccessEnvelope<OrderList>))
)]
pub async fn customer_orders(
    State(state): State<AppState>,
    ReqId(id): ReqId,
    c: CustomerAuth,
    Query(p): Query<Page>,
) -> Result<Response, ApiError> {
    let (limit, offset) = p.bounds();
    let (list, total) = repo::list(
        pool(&state)?,
        c.tenant,
        &OrderFilter {
            account_id: Some(c.session.account_id),
            limit,
            offset,
            ..OrderFilter::default()
        },
    )
    .await
    .map_err(internal)?;
    let items = list
        .into_iter()
        .map(|s| OrderSummaryView {
            customer_name: None,
            ..summary_view(s)
        })
        .collect();
    Ok(no_store(
        Json(SuccessEnvelope::new(OrderList { items, total }, &id)).into_response(),
    ))
}

pub fn stock_conflict(e: InventoryError) -> ApiError {
    crate::inventory::stock_err(e)
}

// ------------------------------------------------------------------ P3-7 pembayaran / P3-8 approval

#[derive(Debug, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct ClaimInput {
    /// Mis. nama pemilik akun pembayar / 4 digit terakhir (opsional, ≤120)
    pub payer_reference: Option<String>,
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct ConfirmInput {
    /// Jumlah yang benar-benar diterima di aplikasi merchant
    pub amount_received: String,
    /// Wajib bila jumlah berbeda dari tagihan
    pub note: Option<String>,
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct ReasonInput {
    pub reason: Option<String>,
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct RefundInput {
    pub amount: String,
    pub reason: String,
    /// Kembalikan stok item order
    #[serde(default)]
    pub restock: bool,
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct StatusInput {
    /// `PROCESSING` | `READY_FOR_PICKUP` | `SHIPPED` | `COMPLETED` | `CANCELLED`
    pub status: String,
    pub reason: Option<String>,
}

fn flow_err(e: FlowError) -> ApiError {
    match e {
        FlowError::NotFound => not_found(),
        FlowError::WrongStatus(s) => ApiError(
            ErrorBody::new(ErrorCode::Conflict)
                .with_message("Status order tidak mengizinkan aksi ini")
                .with_details(serde_json::json!({ "reason": "invalid_order_status", "status": s })),
        ),
        FlowError::Payment(PaymentError::AlreadyDecided) => {
            conflict("payment_already_decided", "Pembayaran sudah diputus")
        }
        FlowError::Payment(PaymentError::NoteRequiredForMismatch) => {
            one("note", "required_for_amount_mismatch")
        }
        FlowError::Payment(PaymentError::RefundExceedsBalance) => {
            one("amount", "exceeds_refundable")
        }
        FlowError::Payment(PaymentError::InvalidAmount) => one("amount", "invalid_price"),
        FlowError::SelfApproval => ApiError(
            ErrorBody::new(ErrorCode::Forbidden)
                .with_message("Penyetuju tidak boleh pemohon (separation of duties)")
                .with_details(serde_json::json!({ "reason": "separation_of_duties" })),
        ),
        FlowError::Upload(e) => crate::upload::upload_error(e),
        FlowError::Stock(e) => crate::inventory::stock_err(e),
        FlowError::Db(e) => internal(e),
    }
}

fn text(
    v: Option<String>,
    path: &str,
    max: usize,
    required: bool,
) -> Result<Option<String>, ApiError> {
    let v = v.map(|s| s.trim().to_owned()).filter(|s| !s.is_empty());
    let mut e = vec![];
    umkm_domain::business::plain_text(&mut e, path, v.as_deref(), max, required);
    if e.is_empty() {
        Ok(v)
    } else {
        Err(field_errors(e.iter().map(|x| (x.path.as_str(), x.code))))
    }
}

async fn reload(
    state: &AppState,
    t: TenantId,
    order: Uuid,
    id: &umkm_shared::http::RequestId,
) -> Result<Json<SuccessEnvelope<OrderView>>, ApiError> {
    let o = repo::detail(pool(state)?, t, OrderKey::Id(order), true)
        .await
        .map_err(internal)?
        .ok_or_else(not_found)?;
    Ok(Json(SuccessEnvelope::new(staff_view(o)?, id)))
}

fn token_hash(headers: &HeaderMap) -> Result<[u8; 32], ApiError> {
    let token = headers
        .get(ORDER_TOKEN_HEADER)
        .and_then(|v| v.to_str().ok())
        .map(str::trim)
        .filter(|s| !s.is_empty() && s.len() <= 128)
        .ok_or_else(not_found)?;
    Ok(hash_token(token).0)
}

/// Pembeli menandai sudah membayar via QRIS (bukan bukti; staf tetap memverifikasi).
#[utoipa::path(
    post, path = "/api/v1/public/orders/current/payment-claim", tag = "public",
    params(("X-Order-Token" = String, Header)), request_body = ClaimInput,
    responses(
        (status = 200, description = "Tercatat; menunggu verifikasi toko", body = SuccessEnvelope<PublicOrder>),
        (status = 409, description = "Order tidak menunggu pembayaran", body = umkm_shared::http::ErrorEnvelope)
    )
)]
pub async fn claim_payment(
    State(state): State<AppState>,
    ReqId(id): ReqId,
    ClientIp(ip): ClientIp,
    HostTenant(t): HostTenant,
    headers: HeaderMap,
    Json(req): Json<ClaimInput>,
) -> Result<Response, ApiError> {
    check_shop_origin(&state, &headers)?;
    rate_limit(&state, &ip).await?;
    let reference = text(
        req.payer_reference,
        "payer_reference",
        umkm_domain::commerce::payment::REFERENCE_MAX,
        false,
    )?;
    let h = token_hash(&headers)?;
    payment_repo::claim(pool(&state)?, t, &h, reference.as_deref())
        .await
        .map_err(flow_err)?;
    let o = order_from_token(&state, t, &headers).await?;
    Ok(no_store(
        Json(SuccessEnvelope::new(public_view(&state, t, o).await?, &id)).into_response(),
    ))
}

/// Unggah bukti bayar (PNG/JPEG/WebP; isi diperiksa server; menggantikan bukti lama).
#[utoipa::path(
    post, path = "/api/v1/public/orders/current/payment-proof", tag = "public",
    params(("X-Order-Token" = String, Header)),
    request_body(content = Vec<u8>, content_type = "image/png"),
    responses(
        (status = 204, description = "Tersimpan"),
        (status = 415, description = "Bukan gambar yang diizinkan", body = umkm_shared::http::ErrorEnvelope)
    )
)]
pub async fn upload_proof(
    State(state): State<AppState>,
    ClientIp(ip): ClientIp,
    HostTenant(t): HostTenant,
    headers: HeaderMap,
    body: axum::body::Bytes,
) -> Result<StatusCode, ApiError> {
    check_shop_origin(&state, &headers)?;
    rate_limit(&state, &ip).await?;
    let h = token_hash(&headers)?;
    let max = state
        .files
        .as_deref()
        .map(|f| f.store.limits().max_object_bytes)
        .unwrap_or(2 * 1024 * 1024);
    payment_repo::put_proof(pool(&state)?, t, &h, &body, max)
        .await
        .map_err(flow_err)?;
    Ok(StatusCode::NO_CONTENT)
}

/// Bukti bayar pembeli untuk staf (`orders.read`).
#[utoipa::path(
    get, path = "/api/v1/tenants/{tenant_id}/orders/{order_id}/payment/proof", tag = "commerce",
    params(("tenant_id" = Uuid, Path), ("order_id" = Uuid, Path)),
    responses((status = 200, description = "Gambar", content_type = "image/png"), (status = 404, description = "Tidak ada"))
)]
pub async fn get_proof(
    State(state): State<AppState>,
    Member(actor): Member,
    Path((_, oid)): Path<(Uuid, Uuid)>,
) -> Result<Response, ApiError> {
    actor.require("orders.read")?;
    let (ct, bytes) = payment_repo::proof(pool(&state)?, actor.tenant_id, oid)
        .await
        .map_err(internal)?
        .ok_or_else(not_found)?;
    let mut res = bytes.into_response();
    let h = res.headers_mut();
    h.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_str(&ct).map_err(internal)?,
    );
    h.insert(
        "x-content-type-options",
        HeaderValue::from_static("nosniff"),
    );
    h.insert(
        header::CONTENT_SECURITY_POLICY,
        HeaderValue::from_static("default-src 'none'; sandbox"),
    );
    h.insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static("private, no-store"),
    );
    Ok(res)
}

/// AC-ORD-007: konfirmasi pembayaran (`payments.verify`), diproses sekali.
#[utoipa::path(
    post, path = "/api/v1/tenants/{tenant_id}/orders/{order_id}/payment/confirm", tag = "commerce",
    params(("tenant_id" = Uuid, Path), ("order_id" = Uuid, Path)), request_body = ConfirmInput,
    responses(
        (status = 200, description = "Dikonfirmasi; order PAID → PROCESSING/APPROVAL_PENDING", body = SuccessEnvelope<OrderView>),
        (status = 409, description = "Sudah diputus / order tidak menunggu pembayaran", body = umkm_shared::http::ErrorEnvelope)
    )
)]
pub async fn confirm_payment(
    State(state): State<AppState>,
    ReqId(id): ReqId,
    ClientIp(ip): ClientIp,
    Member(actor): Member,
    Path((_, oid)): Path<(Uuid, Uuid)>,
    Json(req): Json<ConfirmInput>,
) -> Result<Json<SuccessEnvelope<OrderView>>, ApiError> {
    actor.require("payments.verify")?;
    let received = Money::parse(req.amount_received.trim())
        .ok_or_else(|| one("amount_received", "invalid_price"))?;
    let note = text(
        req.note,
        "note",
        umkm_domain::commerce::payment::NOTE_MAX,
        false,
    )?;
    payment_repo::confirm(
        pool(&state)?,
        &actor,
        oid,
        received,
        note.as_deref(),
        (Some(id.as_str()), Some(&ip)),
    )
    .await
    .map_err(flow_err)?;
    reload(&state, actor.tenant_id, oid, &id).await
}

/// Tolak klaim pembayaran (alasan wajib); order dibatalkan, stok dilepas.
#[utoipa::path(
    post, path = "/api/v1/tenants/{tenant_id}/orders/{order_id}/payment/reject", tag = "commerce",
    params(("tenant_id" = Uuid, Path), ("order_id" = Uuid, Path)), request_body = ReasonInput,
    responses((status = 200, description = "Ditolak", body = SuccessEnvelope<OrderView>))
)]
pub async fn reject_payment(
    State(state): State<AppState>,
    ReqId(id): ReqId,
    ClientIp(ip): ClientIp,
    Member(actor): Member,
    Path((_, oid)): Path<(Uuid, Uuid)>,
    Json(req): Json<ReasonInput>,
) -> Result<Json<SuccessEnvelope<OrderView>>, ApiError> {
    actor.require("payments.verify")?;
    let reason = text(
        req.reason,
        "reason",
        umkm_domain::commerce::payment::REASON_MAX,
        true,
    )?
    .unwrap_or_default();
    payment_repo::reject_payment(
        pool(&state)?,
        &actor,
        oid,
        &reason,
        (Some(id.as_str()), Some(&ip)),
    )
    .await
    .map_err(flow_err)?;
    reload(&state, actor.tenant_id, oid, &id).await
}

/// AC-ORD-008: catat refund (`payments.refund`, Owner). Lunas → REFUNDED.
#[utoipa::path(
    post, path = "/api/v1/tenants/{tenant_id}/orders/{order_id}/refunds", tag = "commerce",
    params(("tenant_id" = Uuid, Path), ("order_id" = Uuid, Path)), request_body = RefundInput,
    responses((status = 200, description = "Refund tercatat", body = SuccessEnvelope<OrderView>))
)]
pub async fn create_refund(
    State(state): State<AppState>,
    ReqId(id): ReqId,
    ClientIp(ip): ClientIp,
    Member(actor): Member,
    Path((_, oid)): Path<(Uuid, Uuid)>,
    Json(req): Json<RefundInput>,
) -> Result<Json<SuccessEnvelope<OrderView>>, ApiError> {
    actor.require("payments.refund")?;
    let amount = Money::parse(req.amount.trim()).ok_or_else(|| one("amount", "invalid_price"))?;
    let reason = text(
        Some(req.reason),
        "reason",
        umkm_domain::commerce::payment::REASON_MAX,
        true,
    )?
    .unwrap_or_default();
    payment_repo::refund(
        pool(&state)?,
        &actor,
        oid,
        amount,
        &reason,
        req.restock,
        (Some(id.as_str()), Some(&ip)),
    )
    .await
    .map_err(flow_err)?;
    reload(&state, actor.tenant_id, oid, &id).await
}

/// Ubah status manual: pemrosesan/pengiriman/selesai (`orders.approve`) atau batal order
/// belum dibayar (`orders.cancel`). AC-ORD-004.
#[utoipa::path(
    post, path = "/api/v1/tenants/{tenant_id}/orders/{order_id}/status", tag = "commerce",
    params(("tenant_id" = Uuid, Path), ("order_id" = Uuid, Path)), request_body = StatusInput,
    responses(
        (status = 200, description = "Status berubah", body = SuccessEnvelope<OrderView>),
        (status = 409, description = "Transisi tidak diizinkan", body = umkm_shared::http::ErrorEnvelope)
    )
)]
pub async fn change_status(
    State(state): State<AppState>,
    ReqId(id): ReqId,
    ClientIp(ip): ClientIp,
    Member(actor): Member,
    Path((_, oid)): Path<(Uuid, Uuid)>,
    Json(req): Json<StatusInput>,
) -> Result<Json<SuccessEnvelope<OrderView>>, ApiError> {
    let to = OrderStatus::parse(&req.status)
        .filter(|s| s.manual_target())
        .ok_or_else(|| one("status", "invalid_status"))?;
    actor.require(if to == OrderStatus::Cancelled {
        "orders.cancel"
    } else {
        "orders.approve"
    })?;
    let reason = text(req.reason, "reason", 500, false)?;
    payment_repo::set_status(
        pool(&state)?,
        &actor,
        oid,
        to,
        reason.as_deref(),
        (Some(id.as_str()), Some(&ip)),
    )
    .await
    .map_err(flow_err)?;
    reload(&state, actor.tenant_id, oid, &id).await
}

/// AC-ORD-005: setujui order (`orders.approve`); SoD: bukan staf yang mengonfirmasi pembayaran.
#[utoipa::path(
    post, path = "/api/v1/tenants/{tenant_id}/orders/{order_id}/approve", tag = "commerce",
    params(("tenant_id" = Uuid, Path), ("order_id" = Uuid, Path)), request_body = ReasonInput,
    responses(
        (status = 200, description = "Disetujui", body = SuccessEnvelope<OrderView>),
        (status = 403, description = "Separation of duties", body = umkm_shared::http::ErrorEnvelope)
    )
)]
pub async fn approve_order(
    State(state): State<AppState>,
    ReqId(id): ReqId,
    ClientIp(ip): ClientIp,
    Member(actor): Member,
    Path((_, oid)): Path<(Uuid, Uuid)>,
    Json(req): Json<ReasonInput>,
) -> Result<Json<SuccessEnvelope<OrderView>>, ApiError> {
    actor.require("orders.approve")?;
    let reason = text(req.reason, "reason", 500, false)?;
    payment_repo::decide(
        pool(&state)?,
        &actor,
        oid,
        true,
        reason.as_deref(),
        (Some(id.as_str()), Some(&ip)),
    )
    .await
    .map_err(flow_err)?;
    reload(&state, actor.tenant_id, oid, &id).await
}

/// AC-ORD-005: tolak order (`orders.reject`, alasan wajib); stok dikembalikan, lanjut refund.
#[utoipa::path(
    post, path = "/api/v1/tenants/{tenant_id}/orders/{order_id}/reject", tag = "commerce",
    params(("tenant_id" = Uuid, Path), ("order_id" = Uuid, Path)), request_body = ReasonInput,
    responses((status = 200, description = "Ditolak", body = SuccessEnvelope<OrderView>))
)]
pub async fn reject_order(
    State(state): State<AppState>,
    ReqId(id): ReqId,
    ClientIp(ip): ClientIp,
    Member(actor): Member,
    Path((_, oid)): Path<(Uuid, Uuid)>,
    Json(req): Json<ReasonInput>,
) -> Result<Json<SuccessEnvelope<OrderView>>, ApiError> {
    actor.require("orders.reject")?;
    let reason = text(req.reason, "reason", 500, true)?;
    payment_repo::decide(
        pool(&state)?,
        &actor,
        oid,
        false,
        reason.as_deref(),
        (Some(id.as_str()), Some(&ip)),
    )
    .await
    .map_err(flow_err)?;
    reload(&state, actor.tenant_id, oid, &id).await
}

pub fn routes() -> Router<AppState> {
    let t = "/api/v1/tenants/{tenant_id}";
    Router::new()
        .route(
            &format!("{t}/commerce/settings"),
            get(get_settings).put(put_settings),
        )
        .route(&format!("{t}/orders"), get(list_orders))
        .route(&format!("{t}/orders/{{order_id}}"), get(get_order))
        .route("/api/v1/public/checkout-config", get(checkout_config))
        .route("/api/v1/public/checkout", post(checkout))
        .route("/api/v1/public/orders/current", get(current_order))
        .route("/api/v1/public/customer/orders", get(customer_orders))
        .route(
            "/api/v1/public/orders/current/payment-claim",
            post(claim_payment),
        )
        .route(
            "/api/v1/public/orders/current/payment-proof",
            post(upload_proof),
        )
        .route(
            &format!("{t}/orders/{{order_id}}/payment/proof"),
            get(get_proof),
        )
        .route(
            &format!("{t}/orders/{{order_id}}/payment/confirm"),
            post(confirm_payment),
        )
        .route(
            &format!("{t}/orders/{{order_id}}/payment/reject"),
            post(reject_payment),
        )
        .route(
            &format!("{t}/orders/{{order_id}}/refunds"),
            post(create_refund),
        )
        .route(
            &format!("{t}/orders/{{order_id}}/status"),
            post(change_status),
        )
        .route(
            &format!("{t}/orders/{{order_id}}/approve"),
            post(approve_order),
        )
        .route(
            &format!("{t}/orders/{{order_id}}/reject"),
            post(reject_order),
        )
}
