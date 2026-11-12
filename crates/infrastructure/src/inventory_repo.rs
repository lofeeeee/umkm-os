//! Inventory ledger + reservasi (P3-2, `PRD.md §4.5`, `PRD.md §11.1`).
//!
//! Setiap mutasi: kunci baris `inventory_levels` (`FOR UPDATE`, urut `variant_id` agar
//! tidak deadlock), hitung level baru di domain, tulis level + movement + event low-stock
//! di transaksi yang sama. Fungsi `*_in` menerima transaksi pemanggil (dipakai checkout).

use sqlx::{PgPool, Row};
use umkm_application::notify::OutboxEvent;
use umkm_domain::{
    audit::AuditEvent,
    inventory::{Level, MovementKind, StockError, crossed_low},
    rbac::Actor,
};
use umkm_shared::ids::TenantId;
use uuid::Uuid;

use crate::pgx::{Tx, tenant_tx, violation};

pub const INVENTORY_SETTINGS_CHANGED: &str = "inventory.settings_changed";

#[derive(Debug, thiserror::Error)]
pub enum InventoryError {
    #[error("varian tidak ditemukan")]
    NotFound,
    #[error(transparent)]
    Stock(#[from] StockError),
    #[error("masih ada stok negatif; tidak dapat menonaktifkan stok negatif")]
    NegativeStockExists,
    #[error(transparent)]
    Db(#[from] sqlx::Error),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LevelRow {
    pub variant_id: Uuid,
    pub product_id: Uuid,
    pub product_name: String,
    pub variant_name: String,
    pub sku: String,
    pub level: Level,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Movement {
    pub id: Uuid,
    pub kind: String,
    pub delta: i64,
    pub on_hand_after: i64,
    pub reason: String,
    pub reference: Option<String>,
    pub actor_user_id: Option<Uuid>,
    pub created_at: String,
}

/// Mutasi manual dari dashboard.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Mutation {
    /// `quantity > 0` masuk
    In(i64),
    /// `quantity > 0` keluar
    Out(i64),
    /// Stock opname: jumlah fisik
    Count(i64),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReservationLine {
    pub variant_id: Uuid,
    pub quantity: i64,
}

fn level_of(r: &sqlx::postgres::PgRow) -> Level {
    Level {
        on_hand: i64::from(r.get::<i32, _>("on_hand")),
        reserved: i64::from(r.get::<i32, _>("reserved")),
        allow_negative: r.get("allow_negative"),
        min_stock: r.get::<Option<i32>, _>("min_stock").map(i64::from),
    }
}

fn i32_of(v: i64) -> Result<i32, InventoryError> {
    i32::try_from(v).map_err(|_| InventoryError::Stock(StockError::InvalidQuantity))
}

async fn lock(tx: &mut Tx, t: TenantId, variant: Uuid) -> Result<Level, InventoryError> {
    let r = sqlx::query(
        "SELECT on_hand, reserved, allow_negative, min_stock FROM inventory_levels
         WHERE tenant_id = $1 AND variant_id = $2 FOR UPDATE",
    )
    .bind(t.as_uuid())
    .bind(variant)
    .fetch_optional(&mut **tx)
    .await?
    .ok_or(InventoryError::NotFound)?;
    Ok(level_of(&r))
}

async fn store(tx: &mut Tx, t: TenantId, variant: Uuid, l: Level) -> Result<(), InventoryError> {
    sqlx::query(
        "UPDATE inventory_levels SET on_hand = $3, reserved = $4, allow_negative = $5,
             min_stock = $6, updated_at = now()
         WHERE tenant_id = $1 AND variant_id = $2",
    )
    .bind(t.as_uuid())
    .bind(variant)
    .bind(i32_of(l.on_hand)?)
    .bind(i32_of(l.reserved)?)
    .bind(l.allow_negative)
    .bind(l.min_stock.map(i32_of).transpose()?)
    .execute(&mut **tx)
    .await
    .map_err(|e| match violation(&e) {
        Some((s, c)) if s == "23514" && c == "ck_inventory_levels_available" => {
            InventoryError::NegativeStockExists
        }
        _ => InventoryError::Db(e),
    })?;
    Ok(())
}

#[allow(clippy::too_many_arguments)]
async fn movement(
    tx: &mut Tx,
    t: TenantId,
    variant: Uuid,
    kind: MovementKind,
    delta: i64,
    after: Level,
    reason: &str,
    reference: Option<&str>,
    actor: Option<Uuid>,
) -> Result<Uuid, InventoryError> {
    Ok(sqlx::query_scalar(
        "INSERT INTO inventory_movements
             (tenant_id, variant_id, kind, delta, on_hand_after, reason, reference, actor_user_id)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8) RETURNING id",
    )
    .bind(t.as_uuid())
    .bind(variant)
    .bind(kind.as_str())
    .bind(i32_of(delta)?)
    .bind(i32_of(after.on_hand)?)
    .bind(reason)
    .bind(reference)
    .bind(actor)
    .fetch_one(&mut **tx)
    .await?)
}

async fn low_stock(
    tx: &mut Tx,
    t: TenantId,
    variant: Uuid,
    before: Level,
    after: Level,
    cause: Uuid,
) -> Result<(), sqlx::Error> {
    if let (true, Some(min)) = (crossed_low(before, after), after.min_stock) {
        crate::outbox::enqueue(
            &mut **tx,
            &OutboxEvent::inventory_low_stock(*t.as_uuid(), variant, after.available(), min, cause),
        )
        .await?;
    }
    Ok(())
}

// ------------------------------------------------------------------ baca

pub async fn list_levels(
    pool: &PgPool,
    t: TenantId,
    low_only: bool,
    q: Option<&str>,
    limit: i64,
    offset: i64,
) -> Result<(Vec<LevelRow>, i64), sqlx::Error> {
    let mut tx = tenant_tx(pool, t).await?;
    let q = q.map(|q| {
        format!(
            "%{}%",
            q.replace('\\', "\\\\")
                .replace('%', "\\%")
                .replace('_', "\\_")
        )
    });
    let base = "FROM inventory_levels l
         JOIN product_variants v ON v.tenant_id = l.tenant_id AND v.id = l.variant_id
         JOIN products p ON p.tenant_id = v.tenant_id AND p.id = v.product_id
         WHERE l.tenant_id = $1 AND p.status <> 'archived'
           AND (NOT $2 OR (l.min_stock IS NOT NULL AND l.on_hand - l.reserved < l.min_stock))
           AND ($3::text IS NULL OR p.name ILIKE $3 OR v.sku ILIKE $3)";
    let total: i64 = sqlx::query_scalar(sqlx::AssertSqlSafe(format!("SELECT count(*) {base}")))
        .bind(t.as_uuid())
        .bind(low_only)
        .bind(&q)
        .fetch_one(&mut *tx)
        .await?;
    let rows = sqlx::query(sqlx::AssertSqlSafe(format!(
        "SELECT l.variant_id, v.product_id, p.name AS product_name, v.name AS variant_name, v.sku,
                l.on_hand, l.reserved, l.allow_negative, l.min_stock
         {base} ORDER BY p.name, v.position LIMIT $4 OFFSET $5"
    )))
    .bind(t.as_uuid())
    .bind(low_only)
    .bind(&q)
    .bind(limit)
    .bind(offset)
    .fetch_all(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok((
        rows.iter()
            .map(|r| LevelRow {
                variant_id: r.get("variant_id"),
                product_id: r.get("product_id"),
                product_name: r.get("product_name"),
                variant_name: r.get("variant_name"),
                sku: r.get("sku"),
                level: level_of(r),
            })
            .collect(),
        total,
    ))
}

pub async fn movements(
    pool: &PgPool,
    t: TenantId,
    variant: Uuid,
    limit: i64,
) -> Result<Vec<Movement>, sqlx::Error> {
    let mut tx = tenant_tx(pool, t).await?;
    let rows = sqlx::query(
        "SELECT id, kind, delta, on_hand_after, reason, reference, actor_user_id,
                to_char(created_at AT TIME ZONE 'UTC', 'YYYY-MM-DD\"T\"HH24:MI:SS.US\"Z\"') AS created_at
         FROM inventory_movements WHERE tenant_id = $1 AND variant_id = $2
         ORDER BY created_at DESC, id LIMIT $3",
    )
    .bind(t.as_uuid())
    .bind(variant)
    .bind(limit)
    .fetch_all(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(rows
        .iter()
        .map(|r| Movement {
            id: r.get("id"),
            kind: r.get("kind"),
            delta: i64::from(r.get::<i32, _>("delta")),
            on_hand_after: i64::from(r.get::<i32, _>("on_hand_after")),
            reason: r.get("reason"),
            reference: r.get("reference"),
            actor_user_id: r.get("actor_user_id"),
            created_at: r.get("created_at"),
        })
        .collect())
}

pub async fn level(
    pool: &PgPool,
    t: TenantId,
    variant: Uuid,
) -> Result<Option<Level>, sqlx::Error> {
    let mut tx = tenant_tx(pool, t).await?;
    let r = sqlx::query(
        "SELECT on_hand, reserved, allow_negative, min_stock FROM inventory_levels
         WHERE tenant_id = $1 AND variant_id = $2",
    )
    .bind(t.as_uuid())
    .bind(variant)
    .fetch_optional(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(r.as_ref().map(level_of))
}

// ------------------------------------------------------------------ mutasi manual

/// Stock in/out/opname oleh staf (AC-INV-001/002/004).
pub async fn mutate(
    pool: &PgPool,
    actor: &Actor,
    variant: Uuid,
    m: Mutation,
    reason: &str,
    reference: Option<&str>,
) -> Result<Level, InventoryError> {
    let t = actor.tenant_id;
    let mut tx = tenant_tx(pool, t).await?;
    let before = lock(&mut tx, t, variant).await?;
    let (after, delta, kind) = match m {
        Mutation::In(q) if q > 0 => (before.apply(q)?, q, MovementKind::StockIn),
        Mutation::Out(q) if q > 0 => (before.apply(-q)?, -q, MovementKind::StockOut),
        Mutation::Count(c) => {
            let (after, d) = before.set_count(c)?;
            (after, d, MovementKind::Adjustment)
        }
        _ => return Err(StockError::InvalidQuantity.into()),
    };
    store(&mut tx, t, variant, after).await?;
    let id = movement(
        &mut tx,
        t,
        variant,
        kind,
        delta,
        after,
        reason,
        reference,
        Some(actor.user_id),
    )
    .await?;
    low_stock(&mut tx, t, variant, before, after, id).await?;
    crate::audit_repo::record(
        &mut *tx,
        &AuditEvent::new(umkm_domain::audit::actions::INVENTORY_ADJUSTED, "inventory_level")
            .by_user(actor.user_id)
            .in_tenant(t)
            .resource(variant)
            .with_summary(serde_json::json!({ "kind": kind.as_str(), "delta": delta, "on_hand_after": after.on_hand, "reason": reason })),
    )
    .await?;
    tx.commit().await?;
    Ok(after)
}

/// Ambang low-stock + policy stok negatif (diaudit: memengaruhi penjualan).
pub async fn update_settings(
    pool: &PgPool,
    actor: &Actor,
    variant: Uuid,
    min_stock: Option<i64>,
    allow_negative: bool,
    (request_id, ip): (Option<&str>, Option<&str>),
) -> Result<Level, InventoryError> {
    let t = actor.tenant_id;
    let mut tx = tenant_tx(pool, t).await?;
    let before = lock(&mut tx, t, variant).await?;
    let after = Level {
        min_stock,
        allow_negative,
        ..before
    };
    store(&mut tx, t, variant, after).await?;
    crate::audit_repo::record(
        &mut *tx,
        &AuditEvent::new(INVENTORY_SETTINGS_CHANGED, "inventory_level")
            .by_user(actor.user_id)
            .in_tenant(t)
            .resource(variant)
            .request(request_id, ip)
            .with_summary(serde_json::json!({
                "before": { "min_stock": before.min_stock, "allow_negative": before.allow_negative },
                "after": { "min_stock": min_stock, "allow_negative": allow_negative },
            })),
    )
    .await?;
    let cause: Uuid = sqlx::query_scalar("SELECT gen_random_uuid()")
        .fetch_one(&mut *tx)
        .await?;
    low_stock(&mut tx, t, variant, before, after, cause).await?;
    tx.commit().await?;
    Ok(after)
}

/// Mutasi stok di transaksi pemanggil (restock order ditolak/refund). `delta` bertanda.
#[allow(clippy::too_many_arguments)]
pub async fn movement_in(
    tx: &mut Tx,
    t: TenantId,
    variant: Uuid,
    kind: MovementKind,
    delta: i64,
    reason: &str,
    reference: Option<&str>,
    actor: Option<Uuid>,
) -> Result<Level, InventoryError> {
    let before = lock(tx, t, variant).await?;
    let after = before.apply(delta)?;
    store(tx, t, variant, after).await?;
    let id = movement(tx, t, variant, kind, delta, after, reason, reference, actor).await?;
    low_stock(tx, t, variant, before, after, id).await?;
    Ok(after)
}

// ------------------------------------------------------------------ reservasi (checkout)

/// Reservasi semua baris atau tidak sama sekali (AC-INV-003/005). Baris digabung per varian
/// dan dikunci berurutan `variant_id` → dua checkout paralel tidak saling deadlock.
pub async fn reserve_in(
    tx: &mut Tx,
    t: TenantId,
    lines: &[ReservationLine],
    reference: &str,
    ttl_secs: i64,
) -> Result<Vec<Uuid>, InventoryError> {
    let mut merged: std::collections::BTreeMap<Uuid, i64> = Default::default();
    for l in lines {
        *merged.entry(l.variant_id).or_default() += l.quantity;
    }
    let mut ids = vec![];
    for (variant, qty) in merged {
        let before = lock(tx, t, variant).await?;
        let after = before.reserve(qty)?;
        store(tx, t, variant, after).await?;
        let id: Uuid = sqlx::query_scalar(
            "INSERT INTO inventory_reservations (tenant_id, variant_id, quantity, reference, expires_at)
             VALUES ($1, $2, $3, $4, now() + make_interval(secs => $5)) RETURNING id",
        )
        .bind(t.as_uuid())
        .bind(variant)
        .bind(i32_of(qty)?)
        .bind(reference)
        .bind(ttl_secs as f64)
        .fetch_one(&mut **tx)
        .await?;
        low_stock(tx, t, variant, before, after, id).await?;
        ids.push(id);
    }
    Ok(ids)
}

/// Reservasi aktif suatu referensi, terkunci, urut varian.
async fn active_for(
    tx: &mut Tx,
    t: TenantId,
    reference: &str,
) -> Result<Vec<(Uuid, Uuid, i64)>, sqlx::Error> {
    let rows = sqlx::query(
        "SELECT id, variant_id, quantity FROM inventory_reservations
         WHERE tenant_id = $1 AND reference = $2 AND status = 'active'
         ORDER BY variant_id FOR UPDATE",
    )
    .bind(t.as_uuid())
    .bind(reference)
    .fetch_all(&mut **tx)
    .await?;
    Ok(rows
        .iter()
        .map(|r| {
            (
                r.get("id"),
                r.get("variant_id"),
                i64::from(r.get::<i32, _>("quantity")),
            )
        })
        .collect())
}

/// Lepas reservasi (order batal/ditolak). Mengembalikan jumlah reservasi yang dilepas;
/// dipanggil dua kali = 0 (idempoten).
pub async fn release_in(
    tx: &mut Tx,
    t: TenantId,
    reference: &str,
) -> Result<usize, InventoryError> {
    let list = active_for(tx, t, reference).await?;
    for (id, variant, qty) in &list {
        let l = lock(tx, t, *variant).await?.release(*qty)?;
        store(tx, t, *variant, l).await?;
        sqlx::query(
            "UPDATE inventory_reservations SET status = 'released', resolved_at = now()
             WHERE tenant_id = $1 AND id = $2",
        )
        .bind(t.as_uuid())
        .bind(id)
        .execute(&mut **tx)
        .await?;
    }
    Ok(list.len())
}

/// Reservasi menjadi penjualan: stok fisik turun + movement `sale` (idempoten).
pub async fn commit_in(
    tx: &mut Tx,
    t: TenantId,
    reference: &str,
    actor: Option<Uuid>,
) -> Result<usize, InventoryError> {
    let list = active_for(tx, t, reference).await?;
    for (id, variant, qty) in &list {
        let before = lock(tx, t, *variant).await?;
        let after = before.commit(*qty)?;
        store(tx, t, *variant, after).await?;
        movement(
            tx,
            t,
            *variant,
            MovementKind::Sale,
            -qty,
            after,
            "Penjualan",
            Some(reference),
            actor,
        )
        .await?;
        sqlx::query(
            "UPDATE inventory_reservations SET status = 'committed', resolved_at = now()
             WHERE tenant_id = $1 AND id = $2",
        )
        .bind(t.as_uuid())
        .bind(id)
        .execute(&mut **tx)
        .await?;
    }
    Ok(list.len())
}

/// Versi berdiri sendiri (uji, dan dipakai sebelum order ada).
pub async fn reserve(
    pool: &PgPool,
    t: TenantId,
    lines: &[ReservationLine],
    reference: &str,
    ttl_secs: i64,
) -> Result<Vec<Uuid>, InventoryError> {
    let mut tx = tenant_tx(pool, t).await?;
    let ids = reserve_in(&mut tx, t, lines, reference, ttl_secs).await?;
    tx.commit().await?;
    Ok(ids)
}

pub async fn release(pool: &PgPool, t: TenantId, reference: &str) -> Result<usize, InventoryError> {
    let mut tx = tenant_tx(pool, t).await?;
    let n = release_in(&mut tx, t, reference).await?;
    tx.commit().await?;
    Ok(n)
}

pub async fn commit(
    pool: &PgPool,
    t: TenantId,
    reference: &str,
    actor: Option<Uuid>,
) -> Result<usize, InventoryError> {
    let mut tx = tenant_tx(pool, t).await?;
    let n = commit_in(&mut tx, t, reference, actor).await?;
    tx.commit().await?;
    Ok(n)
}

/// Job worker: lepas reservasi kedaluwarsa lintas tenant (SECURITY DEFINER).
pub async fn expire_reservations(pool: &PgPool, batch: i32) -> Result<i32, sqlx::Error> {
    sqlx::query_scalar("SELECT app.expire_reservations($1)")
        .bind(batch)
        .fetch_one(pool)
        .await
}
