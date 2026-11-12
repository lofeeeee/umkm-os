//! Inventory (P3-2, `PRD.md §4.5`). Baca `inventory.read`; mutasi & pengaturan `inventory.adjust`.
//! Reservasi tidak punya endpoint: hanya dibuat checkout (P3-5).

use axum::{
    Json, Router,
    extract::{Path, Query, State},
    http::StatusCode,
    routing::{get, put},
};
use serde::{Deserialize, Serialize};
use umkm_domain::{
    business::{FieldError, plain_text, push},
    inventory::{Level, MAX_QTY, MovementKind, REASON_MAX, REFERENCE_MAX, StockError},
};
use umkm_infrastructure::inventory_repo::{
    self as repo, InventoryError, LevelRow, Movement, Mutation,
};
use umkm_shared::http::{ErrorBody, ErrorCode, SuccessEnvelope};
use utoipa::ToSchema;
use uuid::Uuid;

use crate::{
    ApiError, AppState,
    auth::ClientIp,
    http_util::{Page, conflict, internal, not_found, pool},
    middleware::ReqId,
    session::Member,
    validation::{field_errors, one},
};

#[derive(Debug, Serialize, ToSchema)]
pub struct LevelView {
    pub on_hand: i64,
    pub reserved: i64,
    pub available: i64,
    pub allow_negative: bool,
    pub min_stock: Option<i64>,
    pub is_low: bool,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct StockRow {
    pub variant_id: Uuid,
    pub product_id: Uuid,
    pub product_name: String,
    pub variant_name: String,
    pub sku: String,
    #[serde(flatten)]
    pub level: LevelView,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct StockList {
    pub items: Vec<StockRow>,
    pub total: i64,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct MovementView {
    pub id: Uuid,
    /// `stock_in` | `stock_out` | `adjustment` | `sale`
    pub kind: String,
    pub delta: i64,
    pub on_hand_after: i64,
    pub reason: String,
    pub reference: Option<String>,
    pub actor_user_id: Option<Uuid>,
    pub created_at: String,
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct MovementInput {
    /// `stock_in` | `stock_out` | `adjustment` (stock opname: `quantity` = jumlah fisik)
    pub kind: String,
    pub quantity: i64,
    pub reason: String,
    pub reference: Option<String>,
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct SettingsInput {
    pub min_stock: Option<i64>,
    #[serde(default)]
    pub allow_negative: bool,
}

#[derive(Debug, Deserialize, utoipa::IntoParams)]
pub struct StockQuery {
    /// Hanya stok di bawah ambang
    #[serde(default)]
    pub low: bool,
    /// Cari nama produk / SKU
    pub q: Option<String>,
    pub limit: Option<i64>,
    pub offset: Option<i64>,
}

pub fn level_view(l: Level) -> LevelView {
    LevelView {
        on_hand: l.on_hand,
        reserved: l.reserved,
        available: l.available(),
        allow_negative: l.allow_negative,
        min_stock: l.min_stock,
        is_low: l.is_low(),
    }
}

fn row_view(r: LevelRow) -> StockRow {
    StockRow {
        variant_id: r.variant_id,
        product_id: r.product_id,
        product_name: r.product_name,
        variant_name: r.variant_name,
        sku: r.sku,
        level: level_view(r.level),
    }
}

fn movement_view(m: Movement) -> MovementView {
    MovementView {
        id: m.id,
        kind: m.kind,
        delta: m.delta,
        on_hand_after: m.on_hand_after,
        reason: m.reason,
        reference: m.reference,
        actor_user_id: m.actor_user_id,
        created_at: m.created_at,
    }
}

pub fn stock_err(e: InventoryError) -> ApiError {
    match e {
        InventoryError::NotFound => not_found(),
        InventoryError::Stock(StockError::Insufficient {
            available,
            requested,
        }) => ApiError(
            ErrorBody::new(ErrorCode::Conflict)
                .with_message("Stok tidak cukup")
                .with_details(serde_json::json!({
                    "reason": "insufficient_stock", "available": available, "requested": requested
                })),
        ),
        InventoryError::Stock(StockError::InvalidQuantity) => one("quantity", "invalid_quantity"),
        InventoryError::Stock(StockError::ReservedUnderflow) => internal("reserved underflow"),
        InventoryError::NegativeStockExists => conflict(
            "negative_stock_exists",
            "Stok saat ini negatif; tambah stok sebelum menonaktifkan stok negatif",
        ),
        InventoryError::Db(e) => internal(e),
    }
}

type Ok<T> = Result<Json<SuccessEnvelope<T>>, ApiError>;

/// Stok semua varian (produk non-arsip).
#[utoipa::path(
    get, path = "/api/v1/tenants/{tenant_id}/inventory", tag = "inventory",
    params(("tenant_id" = Uuid, Path), StockQuery),
    responses((status = 200, description = "Stok", body = SuccessEnvelope<StockList>))
)]
pub async fn list(
    State(state): State<AppState>,
    ReqId(id): ReqId,
    Member(actor): Member,
    Query(q): Query<StockQuery>,
) -> Ok<StockList> {
    actor.require("inventory.read")?;
    let (limit, offset) = Page {
        limit: q.limit,
        offset: q.offset,
    }
    .bounds();
    let search =
        q.q.map(|s| s.trim().chars().take(100).collect::<String>())
            .filter(|s| !s.is_empty());
    let (rows, total) = repo::list_levels(
        pool(&state)?,
        actor.tenant_id,
        q.low,
        search.as_deref(),
        limit,
        offset,
    )
    .await
    .map_err(internal)?;
    Ok(Json(SuccessEnvelope::new(
        StockList {
            items: rows.into_iter().map(row_view).collect(),
            total,
        },
        &id,
    )))
}

/// Riwayat mutasi satu varian (terbaru dulu, maks 100).
#[utoipa::path(
    get, path = "/api/v1/tenants/{tenant_id}/inventory/{variant_id}/movements", tag = "inventory",
    params(("tenant_id" = Uuid, Path), ("variant_id" = Uuid, Path), Page),
    responses((status = 200, description = "Movement", body = SuccessEnvelope<Vec<MovementView>>))
)]
pub async fn list_movements(
    State(state): State<AppState>,
    ReqId(id): ReqId,
    Member(actor): Member,
    Path((_, variant)): Path<(Uuid, Uuid)>,
    Query(page): Query<Page>,
) -> Ok<Vec<MovementView>> {
    actor.require("inventory.read")?;
    let p = pool(&state)?;
    if repo::level(p, actor.tenant_id, variant)
        .await
        .map_err(internal)?
        .is_none()
    {
        return Err(not_found());
    }
    let list = repo::movements(p, actor.tenant_id, variant, page.bounds().0)
        .await
        .map_err(internal)?;
    Ok(Json(SuccessEnvelope::new(
        list.into_iter().map(movement_view).collect(),
        &id,
    )))
}

/// Stock in / out / opname (AC-INV-001/002/004).
#[utoipa::path(
    post, path = "/api/v1/tenants/{tenant_id}/inventory/{variant_id}/movements", tag = "inventory",
    params(("tenant_id" = Uuid, Path), ("variant_id" = Uuid, Path)), request_body = MovementInput,
    responses(
        (status = 201, description = "Tercatat", body = SuccessEnvelope<LevelView>),
        (status = 409, description = "Stok tidak cukup", body = umkm_shared::http::ErrorEnvelope)
    )
)]
pub async fn create_movement(
    State(state): State<AppState>,
    ReqId(id): ReqId,
    Member(actor): Member,
    Path((_, variant)): Path<(Uuid, Uuid)>,
    Json(req): Json<MovementInput>,
) -> Result<(StatusCode, Json<SuccessEnvelope<LevelView>>), ApiError> {
    actor.require("inventory.adjust")?;
    let mut e: Vec<FieldError> = vec![];
    let kind = MovementKind::parse_manual(&req.kind);
    if kind.is_none() {
        push(&mut e, "kind", "invalid_kind");
    }
    let min = if kind == Some(MovementKind::Adjustment) {
        0
    } else {
        1
    };
    if !(min..=MAX_QTY).contains(&req.quantity) {
        push(&mut e, "quantity", "invalid_quantity");
    }
    let reason = req.reason.trim().to_owned();
    plain_text(&mut e, "reason", Some(&reason), REASON_MAX, true);
    let reference = req
        .reference
        .map(|s| s.trim().to_owned())
        .filter(|s| !s.is_empty());
    plain_text(
        &mut e,
        "reference",
        reference.as_deref(),
        REFERENCE_MAX,
        false,
    );
    if !e.is_empty() {
        return Err(field_errors(e.iter().map(|x| (x.path.as_str(), x.code))));
    }
    let m = match kind {
        Some(MovementKind::StockIn) => Mutation::In(req.quantity),
        Some(MovementKind::StockOut) => Mutation::Out(req.quantity),
        _ => Mutation::Count(req.quantity),
    };
    let l = repo::mutate(
        pool(&state)?,
        &actor,
        variant,
        m,
        &reason,
        reference.as_deref(),
    )
    .await
    .map_err(stock_err)?;
    Ok((
        StatusCode::CREATED,
        Json(SuccessEnvelope::new(level_view(l), &id)),
    ))
}

/// Ambang low-stock dan izin stok negatif (diaudit).
#[utoipa::path(
    put, path = "/api/v1/tenants/{tenant_id}/inventory/{variant_id}/settings", tag = "inventory",
    params(("tenant_id" = Uuid, Path), ("variant_id" = Uuid, Path)), request_body = SettingsInput,
    responses((status = 200, description = "Tersimpan", body = SuccessEnvelope<LevelView>))
)]
pub async fn put_settings(
    State(state): State<AppState>,
    ReqId(id): ReqId,
    ClientIp(ip): ClientIp,
    Member(actor): Member,
    Path((_, variant)): Path<(Uuid, Uuid)>,
    Json(req): Json<SettingsInput>,
) -> Ok<LevelView> {
    actor.require("inventory.adjust")?;
    if let Some(m) = req.min_stock
        && !(0..=MAX_QTY).contains(&m)
    {
        return Err(one("min_stock", "invalid_quantity"));
    }
    let l = repo::update_settings(
        pool(&state)?,
        &actor,
        variant,
        req.min_stock,
        req.allow_negative,
        (Some(id.as_str()), Some(&ip)),
    )
    .await
    .map_err(stock_err)?;
    Ok(Json(SuccessEnvelope::new(level_view(l), &id)))
}

pub fn routes() -> Router<AppState> {
    let t = "/api/v1/tenants/{tenant_id}/inventory";
    Router::new()
        .route(t, get(list))
        .route(
            &format!("{t}/{{variant_id}}/movements"),
            get(list_movements).post(create_movement),
        )
        .route(&format!("{t}/{{variant_id}}/settings"), put(put_settings))
}
