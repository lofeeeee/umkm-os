//! Publik (tenant dari Host): konfigurasi checkout, checkout, status order, QRIS GoPay, riwayat pelanggan.

use super::*;

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
    // ADR-0016: langganan habis → website/katalog/checkout publik nonaktif.
    crate::billing::require_operational(&state, t).await?;
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
    // ADR-0016: langganan habis → website/katalog/checkout publik nonaktif.
    crate::billing::require_operational(&state, t).await?;
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

/// QRIS dinamis GoPay untuk order saat ini (P10). `available=false` bila toko belum menghubungkan
/// GoPay / QRIS statis belum terbaca dari gambar / order bukan menunggu bayar. Idempoten: buat-atau-ambil charge.
#[derive(Debug, Clone, serde::Serialize, utoipa::ToSchema)]
pub struct GopayQrisView {
    pub available: bool,
    /// Payload EMVCo QRIS dinamis (nominal terkunci) untuk dirender jadi QR.
    pub qris_payload: Option<String>,
    /// Nominal yang harus dibayar (rupiah, termasuk kode unik pencocokan).
    pub total_rupiah: Option<i64>,
    /// QR code SVG siap di-inline (render dari `qris_payload`).
    pub qr_svg: Option<String>,
}

/// QRIS dinamis GoPay untuk order pembeli (buat-atau-ambil).
#[utoipa::path(
    post, path = "/api/v1/public/orders/current/gopay-qris", tag = "public",
    params(("X-Order-Token" = String, Header)),
    responses((status = 200, description = "QRIS GoPay", body = SuccessEnvelope<GopayQrisView>))
)]
pub async fn current_gopay_qris(
    State(state): State<AppState>,
    ReqId(id): ReqId,
    HostTenant(t): HostTenant,
    headers: HeaderMap,
) -> Result<Response, ApiError> {
    use umkm_infrastructure::gopay_repo;
    let kr = state
        .auth
        .as_ref()
        .and_then(|a| a.keyring.as_ref())
        .ok_or_else(|| ApiError::new(umkm_shared::http::ErrorCode::ServiceUnavailable))?;
    let o = order_from_token(&state, t, &headers).await?;
    let unavailable = GopayQrisView {
        available: false,
        qris_payload: None,
        total_rupiah: None,
        qr_svg: None,
    };
    let fail = |v| Ok(no_store(Json(SuccessEnvelope::new(v, &id)).into_response()));

    if o.status != OrderStatus::AwaitingPayment {
        return fail(unavailable);
    }
    // Toko harus terhubung aktif + punya QRIS statis; nominal harus rupiah bulat.
    let Some(stored) = gopay_repo::get_session(pool(&state)?, t, kr)
        .await
        .map_err(internal)?
    else {
        return fail(unavailable);
    };
    let settings = repo::settings(pool(&state)?, t).await.map_err(internal)?;
    let (Some(static_qris), true) = (
        settings.qris_static_payload.as_deref(),
        stored.status == "active",
    ) else {
        return fail(unavailable);
    };
    if o.total_sen <= 0 || o.total_sen % 100 != 0 {
        return fail(unavailable);
    }
    let base = (o.total_sen / 100) as u64;

    // TTL selaras batas waktu order.
    let now = umkm_infrastructure::gopay::poller::now_ms();
    let exp = umkm_infrastructure::gopay::time::parse_iso8601_ms(&o.expires_at)
        .unwrap_or(now + 1_800_000);
    let ttl = ((exp - now) / 1000).clamp(60, 24 * 3600);

    let charge = match gopay_repo::get_charge(pool(&state)?, t, o.id)
        .await
        .map_err(internal)?
    {
        Some(c) if c.status == "pending" => c,
        _ => gopay_repo::create_charge(pool(&state)?, t, o.id, base, static_qris, ttl)
            .await
            .map_err(internal)?,
    };
    let qr_svg = umkm_infrastructure::gopay::qrsvg::to_svg(&charge.qris_payload);
    fail(GopayQrisView {
        available: true,
        qris_payload: Some(charge.qris_payload),
        total_rupiah: Some(charge.total_rupiah),
        qr_svg,
    })
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
