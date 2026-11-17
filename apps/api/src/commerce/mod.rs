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
    approvals::EntityType,
    auth::rate_rules,
    catalog::Money,
    commerce::{
        payment::PaymentError,
        pricing::{CartError, CartLine},
        state::{Fulfillment, OrderStatus},
    },
    crm,
    rbac::Actor,
};
use umkm_infrastructure::{
    auth::hash_token,
    commerce_repo::{
        self as repo, CheckoutError, CheckoutInput, Order, OrderFilter, OrderKey, OrderSummary,
        Settings, SettingsError,
    },
    inventory_repo::InventoryError,
    payment_repo::{self, FlowError},
    qris_decode,
    storage::ObjectStorage,
};
use umkm_shared::{
    http::{ErrorBody, ErrorCode, SuccessEnvelope},
    ids::TenantId,
};
use utoipa::ToSchema;
use uuid::Uuid;

use crate::{
    ApiError, AppState,
    approvals::Gate,
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

#[derive(Debug, Serialize, ToSchema)]
pub struct SettingsView {
    /// `asset:<uuid>` gambar QRIS merchant milik toko
    pub qris: Option<String>,
    pub qris_receiver_name: Option<String>,
    pub checkout_enabled: bool,
    /// Ongkir datar untuk delivery, desimal string
    pub delivery_fee: String,
    pub order_approval_required: bool,
    pub reservation_ttl_minutes: i32,
    pub separation_of_duties: bool,
    /// True bila payload QRIS berhasil dibaca dari gambar.
    pub qris_static_ready: bool,
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
#[into_params(parameter_in = Query)]
pub struct OrderQuery {
    pub status: Option<String>,
    pub customer_id: Option<Uuid>,
    pub limit: Option<i64>,
    pub offset: Option<i64>,
}

// ------------------------------------------------------------------ konversi

fn settings_view(s: Settings) -> SettingsView {
    SettingsView {
        qris: s.qris_object_id.map(asset_ref),
        qris_receiver_name: s.qris_receiver_name,
        checkout_enabled: s.checkout_enabled,
        delivery_fee: s.delivery_fee.format(),
        order_approval_required: s.order_approval_required,
        reservation_ttl_minutes: s.reservation_ttl_minutes,
        separation_of_duties: s.separation_of_duties,
        qris_static_ready: s.qris_static_payload.is_some(),
    }
}

async fn decode_uploaded_qris(state: &AppState, t: TenantId, id: Uuid) -> Option<String> {
    let Some(files) = state.files.as_ref() else {
        tracing::warn!(tenant = %t.as_uuid(), "object storage tidak tersedia; QRIS tidak didekode");
        return None;
    };
    let obj = match files.store.get(t, id).await {
        Ok(Some(o)) => o,
        Ok(None) => {
            tracing::warn!(tenant = %t.as_uuid(), asset = %id, "objek QRIS tidak ditemukan");
            return None;
        }
        Err(e) => {
            tracing::warn!(error = %e, "gagal membaca objek QRIS");
            return None;
        }
    };
    let bytes = obj.bytes;
    match tokio::task::spawn_blocking(move || qris_decode::decode_qris_image(&bytes)).await {
        Ok(Ok(payload)) => Some(payload),
        Ok(Err(e)) => {
            tracing::warn!(error = %e, "QRIS tidak terbaca dari gambar");
            None
        }
        Err(e) => {
            tracing::warn!(error = %e, "task decode QRIS gagal");
            None
        }
    }
}

/// Pastikan payload QRIS tersedia untuk gambar saat ini. Kegagalan decode tidak
/// menggagalkan response; manual checkout tetap memakai gambar.
pub(crate) async fn ensure_qris_payload(
    state: &AppState,
    t: TenantId,
) -> Result<Settings, ApiError> {
    let s = repo::settings(pool(state)?, t).await.map_err(internal)?;
    let Some(id) = s.qris_object_id else {
        return Ok(s);
    };
    if s.qris_static_payload.is_some() {
        return Ok(s);
    }
    let Some(payload) = decode_uploaded_qris(state, t, id).await else {
        return Ok(s);
    };
    let updated = repo::backfill_qris_payload(pool(state)?, t, id, &payload)
        .await
        .map_err(internal)?;
    let mut s = s;
    if updated {
        s.qris_static_payload = Some(payload);
    }
    Ok(s)
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
pub(crate) fn check_shop_origin(state: &AppState, h: &HeaderMap) -> Result<(), ApiError> {
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
            ErrorBody::new(ErrorCode::Forbidden).with_message("Asal permintaan tidak diizinkan"),
        ))
    }
}

pub(crate) async fn rate_limit(state: &AppState, ip: &str) -> Result<(), ApiError> {
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

mod payment;
mod public;
mod staff;

pub use payment::*;
pub use public::*;
pub use staff::*;

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
        .route(
            "/api/v1/public/orders/current/gopay-qris",
            post(current_gopay_qris),
        )
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
