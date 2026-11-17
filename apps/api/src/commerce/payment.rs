//! Pembayaran (P3-7) & persetujuan order (P3-8): klaim, bukti, konfirmasi/tolak, refund, status.

use super::*;

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

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
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
                .with_message("Status pesanan tidak mengizinkan aksi ini")
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
                .with_message("Penyetuju tidak boleh pemohon (pemisahan tugas)")
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
    responses(
        (status = 200, description = "Refund tercatat", body = SuccessEnvelope<OrderView>),
        (status = 202, description = "Menunggu approval (kebijakan aktif)", body = SuccessEnvelope<crate::approvals::PendingApproval>),
    )
)]
pub async fn create_refund(
    State(state): State<AppState>,
    ReqId(id): ReqId,
    ClientIp(ip): ClientIp,
    Member(actor): Member,
    Path((_, oid)): Path<(Uuid, Uuid)>,
    Json(req): Json<RefundInput>,
) -> Result<Response, ApiError> {
    actor.require("payments.refund")?;
    let (amount, reason) = parse_refund(&req)?;
    match crate::approvals::gate(
        &state,
        &actor,
        EntityType::Refund,
        Some(oid),
        "refund.create",
        serde_json::json!({ "order_id": oid, "input": &req }),
        &format!("Refund {}: {reason}", amount.format()),
        (Some(id.as_str()), Some(&ip)),
    )
    .await?
    {
        Gate::Pending(aid) => Ok(crate::approvals::pending_response(aid, &id)),
        Gate::Direct => {
            apply_refund(&state, &actor, oid, req, (Some(id.as_str()), Some(&ip))).await?;
            Ok(reload(&state, actor.tenant_id, oid, &id)
                .await?
                .into_response())
        }
    }
}

fn parse_refund(req: &RefundInput) -> Result<(Money, String), ApiError> {
    let amount = Money::parse(req.amount.trim()).ok_or_else(|| one("amount", "invalid_price"))?;
    let reason = text(
        Some(req.reason.clone()),
        "reason",
        umkm_domain::commerce::payment::REASON_MAX,
        true,
    )?
    .unwrap_or_default();
    Ok((amount, reason))
}

pub(crate) async fn apply_refund(
    state: &AppState,
    actor: &Actor,
    oid: Uuid,
    req: RefundInput,
    ctx: (Option<&str>, Option<&str>),
) -> Result<(), ApiError> {
    let (amount, reason) = parse_refund(&req)?;
    payment_repo::refund(pool(state)?, actor, oid, amount, &reason, req.restock, ctx)
        .await
        .map_err(flow_err)?;
    Ok(())
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
        (status = 202, description = "Menunggu approval (kebijakan aktif)", body = SuccessEnvelope<crate::approvals::PendingApproval>),
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
) -> Result<Response, ApiError> {
    actor.require("orders.approve")?;
    let reason = text(req.reason, "reason", 500, false)?;
    decide_gated(&state, &actor, oid, true, reason, &id, &ip).await
}

/// AC-ORD-005: tolak order (`orders.reject`, alasan wajib); stok dikembalikan, lanjut refund.
#[utoipa::path(
    post, path = "/api/v1/tenants/{tenant_id}/orders/{order_id}/reject", tag = "commerce",
    params(("tenant_id" = Uuid, Path), ("order_id" = Uuid, Path)), request_body = ReasonInput,
    responses(
        (status = 200, description = "Ditolak", body = SuccessEnvelope<OrderView>),
        (status = 202, description = "Menunggu approval (kebijakan aktif)", body = SuccessEnvelope<crate::approvals::PendingApproval>),
    )
)]
pub async fn reject_order(
    State(state): State<AppState>,
    ReqId(id): ReqId,
    ClientIp(ip): ClientIp,
    Member(actor): Member,
    Path((_, oid)): Path<(Uuid, Uuid)>,
    Json(req): Json<ReasonInput>,
) -> Result<Response, ApiError> {
    actor.require("orders.reject")?;
    let reason = text(req.reason, "reason", 500, true)?;
    decide_gated(&state, &actor, oid, false, reason, &id, &ip).await
}

/// Keputusan order: bila kebijakan approval `order` aktif, keputusan menjadi permintaan approval.
async fn decide_gated(
    state: &AppState,
    actor: &Actor,
    oid: Uuid,
    approve: bool,
    reason: Option<String>,
    id: &umkm_shared::http::RequestId,
    ip: &str,
) -> Result<Response, ApiError> {
    let o = repo::detail(pool(state)?, actor.tenant_id, OrderKey::Id(oid), false)
        .await
        .map_err(internal)?
        .ok_or_else(not_found)?;
    if o.status != OrderStatus::ApprovalPending {
        return Err(flow_err(FlowError::WrongStatus(o.status.as_str())));
    }
    let action = if approve {
        "order.approve"
    } else {
        "order.reject"
    };
    match crate::approvals::gate(
        state,
        actor,
        EntityType::Order,
        Some(oid),
        action,
        serde_json::json!({ "order_id": oid, "reason": reason }),
        &format!(
            "{} pesanan {}",
            if approve { "Setujui" } else { "Tolak" },
            o.code
        ),
        (Some(id.as_str()), Some(ip)),
    )
    .await?
    {
        Gate::Pending(aid) => Ok(crate::approvals::pending_response(aid, id)),
        Gate::Direct => {
            apply_decision(
                state,
                actor,
                oid,
                approve,
                reason.as_deref(),
                (Some(id.as_str()), Some(ip)),
            )
            .await?;
            Ok(reload(state, actor.tenant_id, oid, id)
                .await?
                .into_response())
        }
    }
}

pub(crate) async fn apply_decision(
    state: &AppState,
    actor: &Actor,
    oid: Uuid,
    approve: bool,
    reason: Option<&str>,
    ctx: (Option<&str>, Option<&str>),
) -> Result<(), ApiError> {
    payment_repo::decide(pool(state)?, actor, oid, approve, reason, ctx)
        .await
        .map_err(flow_err)
}
