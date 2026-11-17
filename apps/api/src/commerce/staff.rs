//! Staf: pengaturan toko & daftar/detail order.

use super::*;

// ------------------------------------------------------------------ staf: pengaturan

/// Pengaturan checkout toko.
#[utoipa::path(
    get, path = "/api/v1/tenants/{tenant_id}/commerce/settings", tag = "commerce",
    params(("tenant_id" = Uuid, Path)),
    responses((status = 200, description = "Pengaturan", body = SuccessEnvelope<SettingsView>))
)]
pub async fn get_settings(
    State(state): State<AppState>,
    ReqId(id): ReqId,
    Member(actor): Member,
) -> Result<Json<SuccessEnvelope<SettingsView>>, ApiError> {
    actor.require("tenant.read")?;
    let s = ensure_qris_payload(&state, actor.tenant_id).await?;
    Ok(Json(SuccessEnvelope::new(settings_view(s), &id)))
}

/// Ubah pengaturan; mengganti QRIS hanya Owner (`tenant.update`) dan diaudit (ADR-0004).
#[utoipa::path(
    put, path = "/api/v1/tenants/{tenant_id}/commerce/settings", tag = "commerce",
    params(("tenant_id" = Uuid, Path)), request_body = SettingsDto,
    responses(
        (status = 202, description = "Menunggu approval (kebijakan aktif)", body = SuccessEnvelope<crate::approvals::PendingApproval>),
        (status = 200, description = "Tersimpan", body = SuccessEnvelope<SettingsView>),
        (status = 422, description = "Validasi gagal / QRIS belum diatur", body = umkm_shared::http::ErrorEnvelope)
    )
)]
pub async fn put_settings(
    State(state): State<AppState>,
    ReqId(id): ReqId,
    ClientIp(ip): ClientIp,
    Member(actor): Member,
    Json(req): Json<SettingsDto>,
) -> Result<Response, ApiError> {
    actor.require("tenant.update")?;
    parse_settings(&req)?;
    // AC-APR-003: konfigurasi bisnis (termasuk QRIS) tidak bisa melewati kebijakan approval.
    match crate::approvals::gate(
        &state,
        &actor,
        EntityType::BusinessConfig,
        None,
        "commerce.settings",
        serde_json::json!({ "input": &req }),
        "Ubah pengaturan toko online",
        (Some(id.as_str()), Some(&ip)),
    )
    .await?
    {
        Gate::Pending(aid) => Ok(crate::approvals::pending_response(aid, &id)),
        Gate::Direct => {
            let s = apply_settings(&state, &actor, &req, (Some(id.as_str()), Some(&ip))).await?;
            Ok(Json(SuccessEnvelope::new(settings_view(s), &id)).into_response())
        }
    }
}

pub(crate) fn parse_settings(req: &SettingsDto) -> Result<Settings, ApiError> {
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
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_owned);
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
    Ok(Settings {
        qris_object_id: qris,
        qris_receiver_name: receiver,
        qris_static_payload: None,
        checkout_enabled: req.checkout_enabled,
        delivery_fee: fee.unwrap_or(Money::ZERO),
        order_approval_required: req.order_approval_required,
        reservation_ttl_minutes: req.reservation_ttl_minutes,
        separation_of_duties: req.separation_of_duties,
    })
}

pub(crate) async fn apply_settings(
    state: &AppState,
    actor: &Actor,
    req: &SettingsDto,
    ctx: (Option<&str>, Option<&str>),
) -> Result<Settings, ApiError> {
    let before = repo::settings(pool(state)?, actor.tenant_id)
        .await
        .map_err(internal)?;
    let mut s = parse_settings(req)?;
    s.qris_static_payload = match (s.qris_object_id, before.qris_object_id) {
        (None, _) => None,
        (Some(id), Some(prev)) if id == prev => {
            if let Some(p) = before.qris_static_payload.clone() {
                Some(p)
            } else {
                decode_uploaded_qris(state, actor.tenant_id, id).await
            }
        }
        (Some(id), _) => decode_uploaded_qris(state, actor.tenant_id, id).await,
    };
    repo::put_settings(pool(state)?, actor, &s, ctx)
        .await
        .map_err(|e| match e {
            SettingsError::QrisRequired => one("qris", "required_for_checkout"),
            SettingsError::InvalidImage => one("qris", "invalid_asset_ref"),
            SettingsError::Db(e) => internal(e),
        })?;
    Ok(s)
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
