//! Katalog produk (P3-1, `PRD.md §4.5`). Semua query di transaksi ber-tenant-context (RLS)
//! dengan filter `tenant_id`. Validasi isi dilakukan domain (`umkm_domain::catalog`); di sini
//! keunikan, kepemilikan (composite FK), dan audit.

// SQL dinamis hanya merangkai konstanta modul ini; nilai selalu lewat bind.
use sqlx::{AssertSqlSafe, PgPool, Row};
use umkm_domain::{
    audit::AuditEvent,
    catalog::{Money, OptionDef, ProductDraft, ProductStatus, Variant},
    rbac::Actor,
};
use umkm_shared::ids::TenantId;
use uuid::Uuid;

use crate::pgx::{Tx, tenant_tx, violation};

pub const PRODUCT_CREATED: &str = "product.created";
pub const PRODUCT_UPDATED: &str = "product.updated";
pub const PRODUCT_STATUS_CHANGED: &str = "product.status_changed";
pub const CATEGORY_CHANGED: &str = "category.changed";

pub type Ctx<'a> = (Option<&'a str>, Option<&'a str>);

#[derive(Debug, thiserror::Error)]
pub enum CatalogError {
    #[error("tidak ditemukan")]
    NotFound,
    #[error("SKU sudah dipakai: {0}")]
    SkuTaken(String),
    #[error("slug sudah dipakai")]
    SlugTaken,
    #[error("nama kategori sudah dipakai")]
    CategoryNameTaken,
    #[error("kategori tidak valid")]
    InvalidCategory,
    #[error("kategori induk tidak valid")]
    InvalidParent,
    #[error("kategori masih memiliki subkategori")]
    CategoryHasChildren,
    #[error("gambar bukan milik tenant")]
    InvalidImage,
    #[error("varian bukan milik produk")]
    InvalidVariant,
    #[error("varian masih dipakai transaksi/stok")]
    VariantInUse,
    #[error("transisi status tidak diizinkan")]
    InvalidTransition,
    #[error(transparent)]
    Db(#[from] sqlx::Error),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Category {
    pub id: Uuid,
    pub parent_id: Option<Uuid>,
    pub name: String,
    pub position: i32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredVariant {
    pub id: Uuid,
    pub sku: String,
    pub name: String,
    pub option_values: Vec<String>,
    pub price: Money,
    pub promo_price: Option<Money>,
    pub is_active: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Product {
    pub id: Uuid,
    pub name: String,
    pub slug: String,
    pub description: Option<String>,
    pub category_id: Option<Uuid>,
    pub status: ProductStatus,
    pub available: bool,
    pub options: Vec<OptionDef>,
    pub variants: Vec<StoredVariant>,
    pub images: Vec<Uuid>,
    pub updated_at: String,
}

#[derive(Debug, Clone, Default)]
pub struct ProductFilter {
    pub status: Option<ProductStatus>,
    pub category_id: Option<Uuid>,
    /// Pencarian nama/SKU (case-insensitive, substring).
    pub q: Option<String>,
    pub limit: i64,
    pub offset: i64,
}

#[derive(serde::Serialize, serde::Deserialize)]
struct OptionJson {
    name: String,
    values: Vec<String>,
}

fn map_write(e: sqlx::Error, _draft: Option<&ProductDraft>) -> CatalogError {
    match violation(&e) {
        Some((s, c)) if s == "23505" && c == "uq_products_slug" => CatalogError::SlugTaken,
        // Detail key (nilai SKU) dihilangkan PostgreSQL pada tabel ber-RLS;
        // SKU yang bentrok dicari ulang oleh `resolve_sku`.
        Some((s, c)) if s == "23505" && c == "uq_product_variants_sku" => {
            CatalogError::SkuTaken(String::new())
        }
        Some((s, c)) if s == "23503" && c == "fk_products_category" => {
            CatalogError::InvalidCategory
        }
        Some((s, c)) if s == "23503" && c == "fk_product_images_object" => {
            CatalogError::InvalidImage
        }
        Some((s, _)) if s == "23503" => CatalogError::VariantInUse,
        _ => CatalogError::Db(e),
    }
}

async fn audit(
    tx: &mut Tx,
    actor: &Actor,
    action: &'static str,
    resource_type: &'static str,
    id: Uuid,
    summary: serde_json::Value,
    (request_id, ip): Ctx<'_>,
) -> Result<(), sqlx::Error> {
    crate::audit_repo::record(
        &mut **tx,
        &AuditEvent::new(action, resource_type)
            .by_user(actor.user_id)
            .in_tenant(actor.tenant_id)
            .resource(id)
            .request(request_id, ip)
            .with_summary(summary),
    )
    .await
}

// ------------------------------------------------------------------ kategori

/// Batas pengaman daftar kategori (bukan paginasi: UI menampilkan semuanya sebagai pohon/pilihan).
pub const MAX_CATEGORIES_LISTED: i64 = 500;

pub async fn list_categories(pool: &PgPool, t: TenantId) -> Result<Vec<Category>, sqlx::Error> {
    let mut tx = tenant_tx(pool, t).await?;
    let rows = sqlx::query(
        "SELECT id, parent_id, name, position FROM product_categories
         WHERE tenant_id = $1 ORDER BY position, lower(name) LIMIT $2",
    )
    .bind(t.as_uuid())
    .bind(MAX_CATEGORIES_LISTED)
    .fetch_all(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(rows
        .into_iter()
        .map(|r| Category {
            id: r.get("id"),
            parent_id: r.get("parent_id"),
            name: r.get("name"),
            position: r.get("position"),
        })
        .collect())
}

fn map_category(e: sqlx::Error) -> CatalogError {
    match violation(&e) {
        Some((s, c)) if s == "23505" && c == "uq_product_categories_name" => {
            CatalogError::CategoryNameTaken
        }
        Some((s, c)) if s == "23503" && c == "fk_product_categories_parent" => {
            CatalogError::InvalidParent
        }
        Some((s, c))
            if s == "23514"
                && (c == "ck_product_categories_depth"
                    || c == "ck_product_categories_not_self") =>
        {
            CatalogError::InvalidParent
        }
        _ => CatalogError::Db(e),
    }
}

/// Buat (id `None`) atau ubah kategori. Parent tenant lain → `InvalidParent` (composite FK).
pub async fn save_category(
    pool: &PgPool,
    actor: &Actor,
    id: Option<Uuid>,
    name: &str,
    parent_id: Option<Uuid>,
    position: i32,
    ctx: Ctx<'_>,
) -> Result<Category, CatalogError> {
    let mut tx = tenant_tx(pool, actor.tenant_id).await?;
    let tid = actor.tenant_id.as_uuid();
    let r = match id {
        None => {
            sqlx::query(
                "INSERT INTO product_categories (tenant_id, parent_id, name, position)
             VALUES ($1, $2, $3, $4) RETURNING id",
            )
            .bind(tid)
            .bind(parent_id)
            .bind(name)
            .bind(position)
            .fetch_optional(&mut *tx)
            .await
        }
        Some(id) => {
            sqlx::query(
                "UPDATE product_categories SET parent_id = $3, name = $4, position = $5
             WHERE tenant_id = $1 AND id = $2 RETURNING id",
            )
            .bind(tid)
            .bind(id)
            .bind(parent_id)
            .bind(name)
            .bind(position)
            .fetch_optional(&mut *tx)
            .await
        }
    }
    .map_err(map_category)?
    .ok_or(CatalogError::NotFound)?;
    let cid: Uuid = r.get("id");
    audit(
        &mut tx,
        actor,
        CATEGORY_CHANGED,
        "product_category",
        cid,
        serde_json::json!({ "op": if id.is_some() { "update" } else { "create" }, "parent_id": parent_id }),
        ctx,
    )
    .await?;
    tx.commit().await?;
    Ok(Category {
        id: cid,
        parent_id,
        name: name.to_owned(),
        position,
    })
}

pub async fn delete_category(
    pool: &PgPool,
    actor: &Actor,
    id: Uuid,
    ctx: Ctx<'_>,
) -> Result<(), CatalogError> {
    let mut tx = tenant_tx(pool, actor.tenant_id).await?;
    let n = sqlx::query("DELETE FROM product_categories WHERE tenant_id = $1 AND id = $2")
        .bind(actor.tenant_id.as_uuid())
        .bind(id)
        .execute(&mut *tx)
        .await
        .map_err(|e| match violation(&e) {
            Some((s, _)) if s == "23503" => CatalogError::CategoryHasChildren,
            _ => CatalogError::Db(e),
        })?
        .rows_affected();
    if n == 0 {
        return Err(CatalogError::NotFound);
    }
    audit(
        &mut tx,
        actor,
        CATEGORY_CHANGED,
        "product_category",
        id,
        serde_json::json!({ "op": "delete" }),
        ctx,
    )
    .await?;
    tx.commit().await?;
    Ok(())
}

// ------------------------------------------------------------------ produk

const PRODUCT_COLS: &str = "id, name, slug, description, category_id, status, available, options,
    to_char(updated_at AT TIME ZONE 'UTC', 'YYYY-MM-DD\"T\"HH24:MI:SS.US\"Z\"') AS updated_at";

const VARIANT_COLS: &str = "id, product_id, sku, name, option_values,
    (price * 100)::bigint AS price_sen, (promo_price * 100)::bigint AS promo_sen, is_active";

fn money(sen: i64) -> Money {
    Money::from_sen(sen).unwrap_or(Money::ZERO)
}

fn variant_of(r: &sqlx::postgres::PgRow) -> StoredVariant {
    let vals: sqlx::types::Json<Vec<String>> = r.get("option_values");
    StoredVariant {
        id: r.get("id"),
        sku: r.get("sku"),
        name: r.get("name"),
        option_values: vals.0,
        price: money(r.get("price_sen")),
        promo_price: r.get::<Option<i64>, _>("promo_sen").map(money),
        is_active: r.get("is_active"),
    }
}

fn product_of(r: &sqlx::postgres::PgRow) -> Product {
    let opts: sqlx::types::Json<Vec<OptionJson>> = r.get("options");
    Product {
        id: r.get("id"),
        name: r.get("name"),
        slug: r.get("slug"),
        description: r.get("description"),
        category_id: r.get("category_id"),
        status: ProductStatus::parse(r.get("status")).unwrap_or(ProductStatus::Draft),
        available: r.get("available"),
        options: opts
            .0
            .into_iter()
            .map(|o| OptionDef {
                name: o.name,
                values: o.values,
            })
            .collect(),
        variants: vec![],
        images: vec![],
        updated_at: r.get("updated_at"),
    }
}

/// Isi varian + gambar untuk daftar produk (satu query per tabel, bukan N+1).
async fn hydrate(tx: &mut Tx, t: TenantId, products: &mut [Product]) -> Result<(), sqlx::Error> {
    if products.is_empty() {
        return Ok(());
    }
    let ids: Vec<Uuid> = products.iter().map(|p| p.id).collect();
    let vs = sqlx::query(AssertSqlSafe(format!(
        "SELECT {VARIANT_COLS} FROM product_variants
         WHERE tenant_id = $1 AND product_id = ANY($2) ORDER BY product_id, position"
    )))
    .bind(t.as_uuid())
    .bind(&ids)
    .fetch_all(&mut **tx)
    .await?;
    let imgs = sqlx::query(
        "SELECT product_id, object_id FROM product_images
         WHERE tenant_id = $1 AND product_id = ANY($2) ORDER BY product_id, position",
    )
    .bind(t.as_uuid())
    .bind(&ids)
    .fetch_all(&mut **tx)
    .await?;
    for p in products.iter_mut() {
        p.variants = vs
            .iter()
            .filter(|r| r.get::<Uuid, _>("product_id") == p.id)
            .map(variant_of)
            .collect();
        p.images = imgs
            .iter()
            .filter(|r| r.get::<Uuid, _>("product_id") == p.id)
            .map(|r| r.get("object_id"))
            .collect();
    }
    Ok(())
}

pub async fn list_products(
    pool: &PgPool,
    t: TenantId,
    f: &ProductFilter,
) -> Result<(Vec<Product>, i64), sqlx::Error> {
    let mut tx = tenant_tx(pool, t).await?;
    let q = f.q.as_deref().map(|q| {
        format!(
            "%{}%",
            q.replace('\\', "\\\\")
                .replace('%', "\\%")
                .replace('_', "\\_")
        )
    });
    let where_ = "WHERE p.tenant_id = $1
          AND ($2::text IS NULL OR p.status = $2)
          AND ($3::uuid IS NULL OR p.category_id = $3
               OR p.category_id IN (SELECT id FROM product_categories WHERE tenant_id = $1 AND parent_id = $3))
          AND ($4::text IS NULL OR p.name ILIKE $4
               OR EXISTS (SELECT 1 FROM product_variants v WHERE v.tenant_id = $1 AND v.product_id = p.id AND v.sku ILIKE $4))";
    let total: i64 = sqlx::query_scalar(AssertSqlSafe(format!(
        "SELECT count(*) FROM products p {where_}"
    )))
    .bind(t.as_uuid())
    .bind(f.status.map(ProductStatus::as_str))
    .bind(f.category_id)
    .bind(&q)
    .fetch_one(&mut *tx)
    .await?;
    let rows = sqlx::query(AssertSqlSafe(format!(
        "SELECT {PRODUCT_COLS} FROM products p {where_}
         ORDER BY p.created_at DESC, p.id LIMIT $5 OFFSET $6"
    )))
    .bind(t.as_uuid())
    .bind(f.status.map(ProductStatus::as_str))
    .bind(f.category_id)
    .bind(&q)
    .bind(f.limit)
    .bind(f.offset)
    .fetch_all(&mut *tx)
    .await?;
    let mut list: Vec<Product> = rows.iter().map(product_of).collect();
    hydrate(&mut tx, t, &mut list).await?;
    tx.commit().await?;
    Ok((list, total))
}

pub async fn get_product(
    pool: &PgPool,
    t: TenantId,
    id: Uuid,
) -> Result<Option<Product>, sqlx::Error> {
    let mut tx = tenant_tx(pool, t).await?;
    let p = get_in(&mut tx, t, id).await?;
    tx.commit().await?;
    Ok(p)
}

async fn get_in(tx: &mut Tx, t: TenantId, id: Uuid) -> Result<Option<Product>, sqlx::Error> {
    let Some(r) = sqlx::query(AssertSqlSafe(format!(
        "SELECT {PRODUCT_COLS} FROM products WHERE tenant_id = $1 AND id = $2"
    )))
    .bind(t.as_uuid())
    .bind(id)
    .fetch_optional(&mut **tx)
    .await?
    else {
        return Ok(None);
    };
    let mut v = [product_of(&r)];
    hydrate(tx, t, &mut v).await?;
    let [p] = v;
    Ok(Some(p))
}

fn options_json(d: &ProductDraft) -> sqlx::types::Json<Vec<OptionJson>> {
    sqlx::types::Json(
        d.options
            .iter()
            .map(|o| OptionJson {
                name: o.name.clone(),
                values: o.values.clone(),
            })
            .collect(),
    )
}

async fn write_children(
    tx: &mut Tx,
    t: TenantId,
    product_id: Uuid,
    d: &ProductDraft,
    is_new: bool,
) -> Result<(), CatalogError> {
    let tid = t.as_uuid();
    let keep: Vec<Uuid> = d.variants.iter().filter_map(|v| v.id).collect();
    if is_new && !keep.is_empty() {
        return Err(CatalogError::InvalidVariant);
    }
    if !is_new {
        sqlx::query(
            "DELETE FROM product_variants WHERE tenant_id = $1 AND product_id = $2 AND id <> ALL($3)",
        )
        .bind(tid)
        .bind(product_id)
        .bind(&keep)
        .execute(&mut **tx)
        .await
        .map_err(|e| map_write(e, Some(d)))?;
    }
    for (pos, v) in d.variants.iter().enumerate() {
        let pos = i16::try_from(pos).unwrap_or(i16::MAX);
        let promo = v.promo_price.map(Money::sen);
        let Variant {
            id,
            sku,
            name,
            option_values,
            price,
            is_active,
            ..
        } = v;
        let vals = sqlx::types::Json(option_values);
        let r = match id {
            Some(vid) => sqlx::query(
                "UPDATE product_variants SET sku = $4, name = $5, option_values = $6,
                     price = $7::bigint::numeric / 100, promo_price = $8::bigint::numeric / 100,
                     is_active = $9, position = $10
                 WHERE tenant_id = $1 AND product_id = $2 AND id = $3",
            )
            .bind(tid)
            .bind(product_id)
            .bind(vid),
            None => sqlx::query(
                "INSERT INTO product_variants
                     (tenant_id, product_id, id, sku, name, option_values, price, promo_price, is_active, position)
                 VALUES ($1, $2, COALESCE($3, gen_random_uuid()), $4, $5, $6,
                         $7::bigint::numeric / 100, $8::bigint::numeric / 100, $9, $10)",
            )
            .bind(tid)
            .bind(product_id)
            .bind(None::<Uuid>),
        }
        .bind(sku)
        .bind(name)
        .bind(vals)
        .bind(price.sen())
        .bind(promo)
        .bind(is_active)
        .bind(pos)
        .execute(&mut **tx)
        .await
        .map_err(|e| map_write(e, Some(d)))?;
        if r.rows_affected() == 0 {
            return Err(CatalogError::InvalidVariant);
        }
    }
    sqlx::query("DELETE FROM product_images WHERE tenant_id = $1 AND product_id = $2")
        .bind(tid)
        .bind(product_id)
        .execute(&mut **tx)
        .await?;
    for (pos, img) in d.images.iter().enumerate() {
        sqlx::query(
            "INSERT INTO product_images (tenant_id, product_id, position, object_id) VALUES ($1, $2, $3, $4)",
        )
        .bind(tid)
        .bind(product_id)
        .bind(i16::try_from(pos).unwrap_or(i16::MAX))
        .bind(img)
        .execute(&mut **tx)
        .await
        .map_err(|e| map_write(e, Some(d)))?;
    }
    Ok(())
}

/// Commit dengan pemetaan error constraint deferred (SKU, posisi).
async fn commit(tx: Tx, d: &ProductDraft) -> Result<(), CatalogError> {
    tx.commit().await.map_err(|e| map_write(e, Some(d)))
}

/// Produk baru selalu `draft`.
pub async fn create_product(
    pool: &PgPool,
    actor: &Actor,
    d: &ProductDraft,
    ctx: Ctx<'_>,
) -> Result<Product, CatalogError> {
    let r = create_inner(pool, actor, d, ctx).await;
    resolve_sku(pool, actor.tenant_id, None, d, r).await
}

async fn create_inner(
    pool: &PgPool,
    actor: &Actor,
    d: &ProductDraft,
    ctx: Ctx<'_>,
) -> Result<Product, CatalogError> {
    let t = actor.tenant_id;
    let mut tx = tenant_tx(pool, t).await?;
    let id: Uuid = sqlx::query_scalar(
        "INSERT INTO products (tenant_id, category_id, name, slug, description, available, options)
         VALUES ($1, $2, $3, $4, $5, $6, $7) RETURNING id",
    )
    .bind(t.as_uuid())
    .bind(d.category_id)
    .bind(&d.name)
    .bind(&d.slug)
    .bind(&d.description)
    .bind(d.available)
    .bind(options_json(d))
    .fetch_one(&mut *tx)
    .await
    .map_err(|e| map_write(e, Some(d)))?;
    write_children(&mut tx, t, id, d, true).await?;
    audit(
        &mut tx,
        actor,
        PRODUCT_CREATED,
        "product",
        id,
        serde_json::json!({ "variants": d.variants.len(), "images": d.images.len() }),
        ctx,
    )
    .await?;
    let p = get_in(&mut tx, t, id)
        .await?
        .ok_or(CatalogError::NotFound)?;
    commit(tx, d).await?;
    Ok(p)
}

/// Ganti isi produk (status tidak berubah). Varian tanpa `id` = baru; varian lama yang
/// tidak dikirim dihapus (ditolak bila masih dirujuk stok/transaksi).
pub async fn update_product(
    pool: &PgPool,
    actor: &Actor,
    id: Uuid,
    d: &ProductDraft,
    ctx: Ctx<'_>,
) -> Result<Product, CatalogError> {
    let r = update_inner(pool, actor, id, d, ctx).await;
    resolve_sku(pool, actor.tenant_id, Some(id), d, r).await
}

async fn update_inner(
    pool: &PgPool,
    actor: &Actor,
    id: Uuid,
    d: &ProductDraft,
    ctx: Ctx<'_>,
) -> Result<Product, CatalogError> {
    let t = actor.tenant_id;
    let mut tx = tenant_tx(pool, t).await?;
    let n = sqlx::query(
        "UPDATE products SET category_id = $3, name = $4, slug = $5, description = $6,
             available = $7, options = $8, updated_at = now()
         WHERE tenant_id = $1 AND id = $2",
    )
    .bind(t.as_uuid())
    .bind(id)
    .bind(d.category_id)
    .bind(&d.name)
    .bind(&d.slug)
    .bind(&d.description)
    .bind(d.available)
    .bind(options_json(d))
    .execute(&mut *tx)
    .await
    .map_err(|e| map_write(e, Some(d)))?
    .rows_affected();
    if n == 0 {
        return Err(CatalogError::NotFound);
    }
    write_children(&mut tx, t, id, d, false).await?;
    audit(
        &mut tx,
        actor,
        PRODUCT_UPDATED,
        "product",
        id,
        serde_json::json!({ "variants": d.variants.len(), "images": d.images.len() }),
        ctx,
    )
    .await?;
    let p = get_in(&mut tx, t, id)
        .await?
        .ok_or(CatalogError::NotFound)?;
    commit(tx, d).await?;
    Ok(p)
}

/// Isi SKU yang bentrok (milik produk lain di tenant yang sama) setelah transaksi gagal.
async fn resolve_sku(
    pool: &PgPool,
    t: TenantId,
    product_id: Option<Uuid>,
    d: &ProductDraft,
    r: Result<Product, CatalogError>,
) -> Result<Product, CatalogError> {
    match r {
        Err(CatalogError::SkuTaken(s)) if s.is_empty() => {
            let skus: Vec<&str> = d.variants.iter().map(|v| v.sku.as_str()).collect();
            let mut tx = tenant_tx(pool, t).await?;
            let found: Option<String> = sqlx::query_scalar(
                "SELECT sku FROM product_variants
                 WHERE tenant_id = $1 AND sku = ANY($2) AND ($3::uuid IS NULL OR product_id <> $3)
                 ORDER BY sku LIMIT 1",
            )
            .bind(t.as_uuid())
            .bind(&skus)
            .bind(product_id)
            .fetch_optional(&mut *tx)
            .await?;
            tx.commit().await?;
            Err(CatalogError::SkuTaken(found.unwrap_or_default()))
        }
        other => other,
    }
}

/// AC-CAT-001. Row lock agar dua transisi paralel tidak sama-sama lolos.
pub async fn set_status(
    pool: &PgPool,
    actor: &Actor,
    id: Uuid,
    to: ProductStatus,
    ctx: Ctx<'_>,
) -> Result<Product, CatalogError> {
    let t = actor.tenant_id;
    let mut tx = tenant_tx(pool, t).await?;
    let from: String = sqlx::query_scalar(
        "SELECT status FROM products WHERE tenant_id = $1 AND id = $2 FOR UPDATE",
    )
    .bind(t.as_uuid())
    .bind(id)
    .fetch_optional(&mut *tx)
    .await?
    .ok_or(CatalogError::NotFound)?;
    let from = ProductStatus::parse(&from).ok_or(CatalogError::InvalidTransition)?;
    if !from.can_transition(to) {
        return Err(CatalogError::InvalidTransition);
    }
    sqlx::query(
        "UPDATE products SET status = $3, updated_at = now() WHERE tenant_id = $1 AND id = $2",
    )
    .bind(t.as_uuid())
    .bind(id)
    .bind(to.as_str())
    .execute(&mut *tx)
    .await?;
    audit(
        &mut tx,
        actor,
        PRODUCT_STATUS_CHANGED,
        "product",
        id,
        serde_json::json!({ "from": from.as_str(), "to": to.as_str() }),
        ctx,
    )
    .await?;
    let p = get_in(&mut tx, t, id)
        .await?
        .ok_or(CatalogError::NotFound)?;
    tx.commit().await?;
    Ok(p)
}

// ------------------------------------------------------------------ publik

/// Katalog publik: hanya produk `active`, varian aktif (AC-CAT-001).
pub async fn public_products(
    pool: &PgPool,
    t: TenantId,
    category_id: Option<Uuid>,
    limit: i64,
    offset: i64,
) -> Result<(Vec<Product>, i64), sqlx::Error> {
    let (mut list, total) = list_products(
        pool,
        t,
        &ProductFilter {
            status: Some(ProductStatus::Active),
            category_id,
            q: None,
            limit,
            offset,
        },
    )
    .await?;
    for p in &mut list {
        p.variants.retain(|v| v.is_active);
    }
    Ok((list, total))
}

pub async fn public_product(
    pool: &PgPool,
    t: TenantId,
    slug: &str,
) -> Result<Option<Product>, sqlx::Error> {
    let mut tx = tenant_tx(pool, t).await?;
    let id: Option<Uuid> = sqlx::query_scalar(
        "SELECT id FROM products WHERE tenant_id = $1 AND slug = $2 AND status = 'active'",
    )
    .bind(t.as_uuid())
    .bind(slug)
    .fetch_optional(&mut *tx)
    .await?;
    let p = match id {
        Some(id) => get_in(&mut tx, t, id).await?,
        None => None,
    };
    tx.commit().await?;
    Ok(p.map(|mut p| {
        p.variants.retain(|v| v.is_active);
        p
    }))
}
