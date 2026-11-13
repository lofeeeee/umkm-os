//! Katalog produk (P3-1, `PRD.md §4.5`). Baca `products.read`; buat `products.create`;
//! ubah/aktifkan `products.update`; arsip `products.archive`. Publik: hanya produk `active`.

use std::collections::BTreeSet;

use axum::{
    Json, Router,
    extract::{Path, Query, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::{get, post, put},
};
use serde::{Deserialize, Serialize};
use umkm_domain::approvals::EntityType;
use umkm_domain::catalog::{
    self, OptionDef, ProductInput, ProductStatus, VariantInput, validate_category_name,
};
use umkm_infrastructure::catalog_repo::{
    self as repo, CatalogError, Category, Product, ProductFilter,
};
use umkm_shared::http::{ErrorCode, SuccessEnvelope};
use utoipa::ToSchema;
use uuid::Uuid;

use crate::{
    ApiError, AppState,
    approvals::Gate,
    auth::ClientIp,
    http_util::{
        Page, asset_ref, conflict, internal, not_found, parse_asset_ref, pool, public_json,
    },
    middleware::{HostTenant, ReqId},
    session::Member,
    validation::{field_errors, one},
    website::signed_assets,
};

// ------------------------------------------------------------------ DTO

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct OptionDto {
    pub name: String,
    pub values: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct VariantDto {
    /// Kosong = varian baru
    pub id: Option<Uuid>,
    pub sku: String,
    #[serde(default)]
    pub option_values: Vec<String>,
    /// Desimal string, mis. `"15000"` / `"15000.50"`
    pub price: String,
    pub promo_price: Option<String>,
    #[serde(default = "yes")]
    pub is_active: bool,
}

fn yes() -> bool {
    true
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct ProductInputDto {
    pub name: String,
    /// Kosong = dibuat dari nama
    pub slug: Option<String>,
    pub description: Option<String>,
    pub category_id: Option<Uuid>,
    #[serde(default = "yes")]
    pub available: bool,
    #[serde(default)]
    pub options: Vec<OptionDto>,
    pub variants: Vec<VariantDto>,
    /// `asset:<uuid>` milik tenant, urutan tampil
    #[serde(default)]
    pub images: Vec<String>,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct VariantView {
    pub id: Uuid,
    pub sku: String,
    pub name: String,
    pub option_values: Vec<String>,
    pub price: String,
    pub promo_price: Option<String>,
    pub is_active: bool,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct ProductView {
    pub id: Uuid,
    pub name: String,
    pub slug: String,
    pub description: Option<String>,
    pub category_id: Option<Uuid>,
    /// `draft` | `active` | `archived`
    pub status: String,
    pub available: bool,
    pub options: Vec<OptionDto>,
    pub variants: Vec<VariantView>,
    pub images: Vec<String>,
    pub updated_at: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct ProductList {
    pub items: Vec<ProductView>,
    pub total: i64,
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct StatusChange {
    /// `active` | `archived`
    pub status: String,
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct CategoryInput {
    pub name: String,
    pub parent_id: Option<Uuid>,
    #[serde(default)]
    pub position: i32,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct CategoryView {
    pub id: Uuid,
    pub parent_id: Option<Uuid>,
    pub name: String,
    pub position: i32,
}

#[derive(Debug, Deserialize, utoipa::IntoParams)]
pub struct ProductQuery {
    /// `draft` | `active` | `archived`
    pub status: Option<String>,
    /// Termasuk subkategori langsung
    pub category_id: Option<Uuid>,
    /// Cari nama atau SKU
    pub q: Option<String>,
    pub limit: Option<i64>,
    pub offset: Option<i64>,
}

/// Produk publik: tanpa status/updated_at internal; gambar sebagai signed URL.
#[derive(Debug, Serialize, ToSchema)]
pub struct PublicProduct {
    pub id: Uuid,
    pub name: String,
    pub slug: String,
    pub description: Option<String>,
    pub category_id: Option<Uuid>,
    pub available: bool,
    pub options: Vec<OptionDto>,
    pub variants: Vec<PublicVariant>,
    /// Signed URL (ADR-0009), urutan tampil
    pub images: Vec<String>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct PublicVariant {
    pub id: Uuid,
    pub name: String,
    pub option_values: Vec<String>,
    pub price: String,
    pub promo_price: Option<String>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct PublicProductList {
    pub items: Vec<PublicProduct>,
    pub total: i64,
}

#[derive(Debug, Deserialize, utoipa::IntoParams)]
pub struct PublicQuery {
    pub category_id: Option<Uuid>,
    pub limit: Option<i64>,
    pub offset: Option<i64>,
}

// ------------------------------------------------------------------ konversi

fn options_dto(o: Vec<OptionDef>) -> Vec<OptionDto> {
    o.into_iter()
        .map(|o| OptionDto {
            name: o.name,
            values: o.values,
        })
        .collect()
}

pub fn view(p: Product) -> ProductView {
    ProductView {
        id: p.id,
        name: p.name,
        slug: p.slug,
        description: p.description,
        category_id: p.category_id,
        status: p.status.as_str().into(),
        available: p.available,
        options: options_dto(p.options),
        variants: p
            .variants
            .into_iter()
            .map(|v| VariantView {
                id: v.id,
                sku: v.sku,
                name: v.name,
                option_values: v.option_values,
                price: v.price.format(),
                promo_price: v.promo_price.map(|m| m.format()),
                is_active: v.is_active,
            })
            .collect(),
        images: p.images.into_iter().map(asset_ref).collect(),
        updated_at: p.updated_at,
    }
}

fn category_view(c: Category) -> CategoryView {
    CategoryView {
        id: c.id,
        parent_id: c.parent_id,
        name: c.name,
        position: c.position,
    }
}

fn to_input(d: ProductInputDto) -> Result<ProductInput, ApiError> {
    let mut bad = vec![];
    let images = d
        .images
        .iter()
        .enumerate()
        .filter_map(|(i, s)| {
            let id = parse_asset_ref(s);
            if id.is_none() {
                bad.push(format!("images[{i}]"));
            }
            id
        })
        .collect();
    if !bad.is_empty() {
        return Err(field_errors(
            bad.iter().map(|p| (p.as_str(), "invalid_asset_ref")),
        ));
    }
    Ok(ProductInput {
        name: d.name,
        slug: d.slug,
        description: d.description,
        category_id: d.category_id,
        available: d.available,
        options: d
            .options
            .into_iter()
            .map(|o| OptionDef {
                name: o.name,
                values: o.values,
            })
            .collect(),
        variants: d
            .variants
            .into_iter()
            .map(|v| VariantInput {
                id: v.id,
                sku: v.sku,
                option_values: v.option_values,
                price: v.price,
                promo_price: v.promo_price,
                is_active: v.is_active,
            })
            .collect(),
        images,
    })
}

fn validate(d: ProductInputDto) -> Result<catalog::ProductDraft, ApiError> {
    catalog::validate_product(to_input(d)?)
        .map_err(|e| field_errors(e.iter().map(|x| (x.path.as_str(), x.code))))
}

fn repo_err(e: CatalogError) -> ApiError {
    match e {
        CatalogError::NotFound => not_found(),
        CatalogError::SkuTaken(sku) => ApiError(
            umkm_shared::http::ErrorBody::new(ErrorCode::Conflict)
                .with_message("SKU sudah dipakai produk lain")
                .with_details(serde_json::json!({ "reason": "sku_taken", "sku": sku })),
        ),
        CatalogError::SlugTaken => conflict("slug_taken", "Slug sudah dipakai produk lain"),
        CatalogError::CategoryNameTaken => {
            conflict("category_name_taken", "Nama kategori sudah dipakai")
        }
        CatalogError::InvalidCategory => one("category_id", "invalid_category"),
        CatalogError::InvalidParent => one("parent_id", "invalid_parent"),
        CatalogError::CategoryHasChildren => {
            conflict("category_has_children", "Hapus subkategori terlebih dahulu")
        }
        CatalogError::InvalidImage => one("images", "invalid_asset_ref"),
        CatalogError::InvalidVariant => one("variants", "unknown_variant"),
        CatalogError::VariantInUse => conflict(
            "variant_in_use",
            "Varian sudah dipakai stok/transaksi; nonaktifkan saja",
        ),
        CatalogError::InvalidTransition => {
            conflict("invalid_transition", "Perubahan status tidak diizinkan")
        }
        CatalogError::Db(e) => internal(e),
    }
}

type Ok<T> = Result<Json<SuccessEnvelope<T>>, ApiError>;

// ------------------------------------------------------------------ kategori

/// Semua kategori + subkategori tenant.
#[utoipa::path(
    get, path = "/api/v1/tenants/{tenant_id}/categories", tag = "catalog",
    params(("tenant_id" = Uuid, Path)),
    responses((status = 200, description = "Kategori", body = SuccessEnvelope<Vec<CategoryView>>))
)]
pub async fn list_categories(
    State(state): State<AppState>,
    ReqId(id): ReqId,
    Member(actor): Member,
) -> Ok<Vec<CategoryView>> {
    actor.require("products.read")?;
    let list = repo::list_categories(pool(&state)?, actor.tenant_id)
        .await
        .map_err(internal)?;
    Ok(Json(SuccessEnvelope::new(
        list.into_iter().map(category_view).collect(),
        &id,
    )))
}

/// Buat kategori; `parent_id` = subkategori (maks 2 tingkat, AC-CAT-002).
#[utoipa::path(
    post, path = "/api/v1/tenants/{tenant_id}/categories", tag = "catalog",
    params(("tenant_id" = Uuid, Path)), request_body = CategoryInput,
    responses((status = 201, description = "Dibuat", body = SuccessEnvelope<CategoryView>))
)]
pub async fn create_category(
    State(state): State<AppState>,
    ReqId(id): ReqId,
    ClientIp(ip): ClientIp,
    Member(actor): Member,
    Json(req): Json<CategoryInput>,
) -> Result<(StatusCode, Json<SuccessEnvelope<CategoryView>>), ApiError> {
    actor.require("products.update")?;
    let name = validate_category_name(&req.name)
        .map_err(|e| field_errors(e.iter().map(|x| (x.path.as_str(), x.code))))?;
    let c = repo::save_category(
        pool(&state)?,
        &actor,
        None,
        &name,
        req.parent_id,
        req.position,
        (Some(id.as_str()), Some(&ip)),
    )
    .await
    .map_err(repo_err)?;
    Ok((
        StatusCode::CREATED,
        Json(SuccessEnvelope::new(category_view(c), &id)),
    ))
}

/// Ubah nama/induk/urutan kategori.
#[utoipa::path(
    put, path = "/api/v1/tenants/{tenant_id}/categories/{category_id}", tag = "catalog",
    params(("tenant_id" = Uuid, Path), ("category_id" = Uuid, Path)), request_body = CategoryInput,
    responses((status = 200, description = "Tersimpan", body = SuccessEnvelope<CategoryView>))
)]
pub async fn update_category(
    State(state): State<AppState>,
    ReqId(id): ReqId,
    ClientIp(ip): ClientIp,
    Member(actor): Member,
    Path((_, cid)): Path<(Uuid, Uuid)>,
    Json(req): Json<CategoryInput>,
) -> Ok<CategoryView> {
    actor.require("products.update")?;
    let name = validate_category_name(&req.name)
        .map_err(|e| field_errors(e.iter().map(|x| (x.path.as_str(), x.code))))?;
    let c = repo::save_category(
        pool(&state)?,
        &actor,
        Some(cid),
        &name,
        req.parent_id,
        req.position,
        (Some(id.as_str()), Some(&ip)),
    )
    .await
    .map_err(repo_err)?;
    Ok(Json(SuccessEnvelope::new(category_view(c), &id)))
}

/// Hapus kategori; produk di dalamnya menjadi tanpa kategori.
#[utoipa::path(
    delete, path = "/api/v1/tenants/{tenant_id}/categories/{category_id}", tag = "catalog",
    params(("tenant_id" = Uuid, Path), ("category_id" = Uuid, Path)),
    responses((status = 204, description = "Dihapus"))
)]
pub async fn delete_category(
    State(state): State<AppState>,
    ReqId(id): ReqId,
    ClientIp(ip): ClientIp,
    Member(actor): Member,
    Path((_, cid)): Path<(Uuid, Uuid)>,
) -> Result<StatusCode, ApiError> {
    actor.require("products.update")?;
    repo::delete_category(pool(&state)?, &actor, cid, (Some(id.as_str()), Some(&ip)))
        .await
        .map_err(repo_err)?;
    Ok(StatusCode::NO_CONTENT)
}

// ------------------------------------------------------------------ produk

/// Daftar produk (filter status/kategori/pencarian).
#[utoipa::path(
    get, path = "/api/v1/tenants/{tenant_id}/products", tag = "catalog",
    params(("tenant_id" = Uuid, Path), ProductQuery),
    responses((status = 200, description = "Produk", body = SuccessEnvelope<ProductList>))
)]
pub async fn list_products(
    State(state): State<AppState>,
    ReqId(id): ReqId,
    Member(actor): Member,
    Query(q): Query<ProductQuery>,
) -> Ok<ProductList> {
    actor.require("products.read")?;
    let status = match q.status.as_deref() {
        None | Some("") => None,
        Some(s) => Some(ProductStatus::parse(s).ok_or_else(|| one("status", "invalid_status"))?),
    };
    let (limit, offset) = Page {
        limit: q.limit,
        offset: q.offset,
    }
    .bounds();
    let search =
        q.q.map(|s| s.trim().chars().take(100).collect::<String>())
            .filter(|s| !s.is_empty());
    let (list, total) = repo::list_products(
        pool(&state)?,
        actor.tenant_id,
        &ProductFilter {
            status,
            category_id: q.category_id,
            q: search,
            limit,
            offset,
        },
    )
    .await
    .map_err(internal)?;
    Ok(Json(SuccessEnvelope::new(
        ProductList {
            items: list.into_iter().map(view).collect(),
            total,
        },
        &id,
    )))
}

/// Buat produk (status awal `draft`).
#[utoipa::path(
    post, path = "/api/v1/tenants/{tenant_id}/products", tag = "catalog",
    params(("tenant_id" = Uuid, Path)), request_body = ProductInputDto,
    responses(
        (status = 201, description = "Dibuat", body = SuccessEnvelope<ProductView>),
        (status = 409, description = "SKU/slug bentrok", body = umkm_shared::http::ErrorEnvelope),
        (status = 422, description = "Validasi gagal", body = umkm_shared::http::ErrorEnvelope)
    )
)]
pub async fn create_product(
    State(state): State<AppState>,
    ReqId(id): ReqId,
    ClientIp(ip): ClientIp,
    Member(actor): Member,
    Json(req): Json<ProductInputDto>,
) -> Result<(StatusCode, Json<SuccessEnvelope<ProductView>>), ApiError> {
    actor.require("products.create")?;
    let d = validate(req)?;
    // P7-2: batas jumlah produk paket (produk archived tidak dihitung).
    let plan = crate::billing::plan_of(&state, actor.tenant_id).await?;
    let used = crate::billing::usage(&state, actor.tenant_id).await?;
    crate::billing::check_count("max_products", plan.plan.limits.max_products, used.products)?;
    let p = repo::create_product(pool(&state)?, &actor, &d, (Some(id.as_str()), Some(&ip)))
        .await
        .map_err(repo_err)?;
    Ok((
        StatusCode::CREATED,
        Json(SuccessEnvelope::new(view(p), &id)),
    ))
}

/// Satu produk.
#[utoipa::path(
    get, path = "/api/v1/tenants/{tenant_id}/products/{product_id}", tag = "catalog",
    params(("tenant_id" = Uuid, Path), ("product_id" = Uuid, Path)),
    responses((status = 200, description = "Produk", body = SuccessEnvelope<ProductView>))
)]
pub async fn get_product(
    State(state): State<AppState>,
    ReqId(id): ReqId,
    Member(actor): Member,
    Path((_, pid)): Path<(Uuid, Uuid)>,
) -> Ok<ProductView> {
    actor.require("products.read")?;
    let p = repo::get_product(pool(&state)?, actor.tenant_id, pid)
        .await
        .map_err(internal)?
        .ok_or_else(not_found)?;
    Ok(Json(SuccessEnvelope::new(view(p), &id)))
}

/// Ganti isi produk (status tidak berubah lewat endpoint ini).
#[utoipa::path(
    put, path = "/api/v1/tenants/{tenant_id}/products/{product_id}", tag = "catalog",
    params(("tenant_id" = Uuid, Path), ("product_id" = Uuid, Path)), request_body = ProductInputDto,
    responses(
        (status = 200, description = "Tersimpan", body = SuccessEnvelope<ProductView>),
        (status = 202, description = "Perubahan harga menunggu approval", body = SuccessEnvelope<crate::approvals::PendingApproval>)
    )
)]
pub async fn update_product(
    State(state): State<AppState>,
    ReqId(id): ReqId,
    ClientIp(ip): ClientIp,
    Member(actor): Member,
    Path((_, pid)): Path<(Uuid, Uuid)>,
    Json(req): Json<ProductInputDto>,
) -> Result<Response, ApiError> {
    actor.require("products.update")?;
    let d = validate(req.clone())?;
    let current = repo::get_product(pool(&state)?, actor.tenant_id, pid)
        .await
        .map_err(internal)?
        .ok_or_else(not_found)?;
    // AC-APR-003: perubahan harga (termasuk varian baru) tidak bisa melewati kebijakan price_change.
    let price_changed = d.variants.iter().any(|v| {
        v.id.is_none_or(|vid| {
            current
                .variants
                .iter()
                .find(|c| c.id == vid)
                .is_none_or(|c| c.price != v.price || c.promo_price != v.promo_price)
        })
    });
    if price_changed {
        let gate = crate::approvals::gate(
            &state,
            &actor,
            EntityType::PriceChange,
            Some(pid),
            "product.update",
            serde_json::json!({ "product_id": pid, "input": &req }),
            &format!("Ubah harga {}", current.name),
            (Some(id.as_str()), Some(&ip)),
        )
        .await?;
        if let Gate::Pending(aid) = gate {
            return Ok(crate::approvals::pending_response(aid, &id));
        }
    }
    let p = apply_update(&state, &actor, pid, req, (Some(id.as_str()), Some(&ip))).await?;
    Ok(Json(SuccessEnvelope::new(p, &id)).into_response())
}

/// Terapkan perubahan produk (langsung atau oleh executor approval).
pub(crate) async fn apply_update(
    state: &AppState,
    actor: &umkm_domain::rbac::Actor,
    pid: Uuid,
    req: ProductInputDto,
    ctx: (Option<&str>, Option<&str>),
) -> Result<ProductView, ApiError> {
    let d = validate(req)?;
    let p = repo::update_product(pool(state)?, actor, pid, &d, ctx)
        .await
        .map_err(repo_err)?;
    Ok(view(p))
}

/// Aktifkan (`products.update`) atau arsipkan (`products.archive`). AC-CAT-001.
#[utoipa::path(
    post, path = "/api/v1/tenants/{tenant_id}/products/{product_id}/status", tag = "catalog",
    params(("tenant_id" = Uuid, Path), ("product_id" = Uuid, Path)), request_body = StatusChange,
    responses(
        (status = 200, description = "Status berubah", body = SuccessEnvelope<ProductView>),
        (status = 409, description = "Transisi tidak diizinkan", body = umkm_shared::http::ErrorEnvelope)
    )
)]
pub async fn change_status(
    State(state): State<AppState>,
    ReqId(id): ReqId,
    ClientIp(ip): ClientIp,
    Member(actor): Member,
    Path((_, pid)): Path<(Uuid, Uuid)>,
    Json(req): Json<StatusChange>,
) -> Ok<ProductView> {
    let to = match ProductStatus::parse(&req.status) {
        Some(s @ ProductStatus::Active) => {
            actor.require("products.update")?;
            s
        }
        Some(s @ ProductStatus::Archived) => {
            actor.require("products.archive")?;
            s
        }
        _ => return Err(one("status", "invalid_status")),
    };
    let p = repo::set_status(
        pool(&state)?,
        &actor,
        pid,
        to,
        (Some(id.as_str()), Some(&ip)),
    )
    .await
    .map_err(repo_err)?;
    Ok(Json(SuccessEnvelope::new(view(p), &id)))
}

// ------------------------------------------------------------------ publik

async fn public_view(
    state: &AppState,
    t: umkm_shared::ids::TenantId,
    list: Vec<Product>,
) -> Result<Vec<PublicProduct>, ApiError> {
    let ids: BTreeSet<Uuid> = list.iter().flat_map(|p| p.images.iter().copied()).collect();
    let signed = signed_assets(state, t, ids).await?;
    Ok(list
        .into_iter()
        .map(|p| PublicProduct {
            id: p.id,
            name: p.name,
            slug: p.slug,
            description: p.description,
            category_id: p.category_id,
            available: p.available,
            options: options_dto(p.options),
            variants: p
                .variants
                .into_iter()
                .map(|v| PublicVariant {
                    id: v.id,
                    name: v.name,
                    option_values: v.option_values,
                    price: v.price.format(),
                    promo_price: v.promo_price.map(|m| m.format()),
                })
                .collect(),
            images: p
                .images
                .iter()
                .filter_map(|i| signed.get(&asset_ref(*i)).cloned())
                .collect(),
        })
        .collect())
}

/// Katalog publik toko (tenant dari Host). Hanya produk `active` (AC-CAT-001).
#[utoipa::path(
    get, path = "/api/v1/public/products", tag = "public",
    params(PublicQuery),
    responses((status = 200, description = "Produk aktif", body = SuccessEnvelope<PublicProductList>))
)]
pub async fn public_products(
    State(state): State<AppState>,
    ReqId(id): ReqId,
    HostTenant(t): HostTenant,
    Query(q): Query<PublicQuery>,
) -> Result<Response, ApiError> {
    // ADR-0016: langganan habis → website/katalog/checkout publik nonaktif.
    crate::billing::require_operational(&state, t).await?;
    let (limit, offset) = Page {
        limit: q.limit,
        offset: q.offset,
    }
    .bounds();
    let (list, total) = repo::public_products(pool(&state)?, t, q.category_id, limit, offset)
        .await
        .map_err(internal)?;
    let items = public_view(&state, t, list).await?;
    Ok(public_json(PublicProductList { items, total }, &id))
}

/// Satu produk publik berdasarkan slug.
#[utoipa::path(
    get, path = "/api/v1/public/products/{slug}", tag = "public",
    params(("slug" = String, Path)),
    responses(
        (status = 200, description = "Produk", body = SuccessEnvelope<PublicProduct>),
        (status = 404, description = "Tidak ada / bukan active", body = umkm_shared::http::ErrorEnvelope)
    )
)]
pub async fn public_product(
    State(state): State<AppState>,
    ReqId(id): ReqId,
    HostTenant(t): HostTenant,
    Path(slug): Path<String>,
) -> Result<Response, ApiError> {
    // ADR-0016: langganan habis → website/katalog/checkout publik nonaktif.
    crate::billing::require_operational(&state, t).await?;
    if !catalog::is_valid_slug(&slug) {
        return Err(not_found());
    }
    let p = repo::public_product(pool(&state)?, t, &slug)
        .await
        .map_err(internal)?
        .ok_or_else(not_found)?;
    let mut v = public_view(&state, t, vec![p]).await?;
    Ok(public_json(v.remove(0), &id))
}

/// Kategori publik toko.
#[utoipa::path(
    get, path = "/api/v1/public/categories", tag = "public",
    responses((status = 200, description = "Kategori", body = SuccessEnvelope<Vec<CategoryView>>))
)]
pub async fn public_categories(
    State(state): State<AppState>,
    ReqId(id): ReqId,
    HostTenant(t): HostTenant,
) -> Result<Response, ApiError> {
    // ADR-0016: langganan habis → website/katalog/checkout publik nonaktif.
    crate::billing::require_operational(&state, t).await?;
    let list = repo::list_categories(pool(&state)?, t)
        .await
        .map_err(internal)?;
    Ok(public_json(
        list.into_iter().map(category_view).collect::<Vec<_>>(),
        &id,
    ))
}

pub fn routes() -> Router<AppState> {
    let t = "/api/v1/tenants/{tenant_id}";
    Router::new()
        .route(
            &format!("{t}/categories"),
            get(list_categories).post(create_category),
        )
        .route(
            &format!("{t}/categories/{{category_id}}"),
            put(update_category).delete(delete_category),
        )
        .route(
            &format!("{t}/products"),
            get(list_products).post(create_product),
        )
        .route(
            &format!("{t}/products/{{product_id}}"),
            get(get_product).put(update_product),
        )
        .route(
            &format!("{t}/products/{{product_id}}/status"),
            post(change_status),
        )
        .route("/api/v1/public/products", get(public_products))
        .route("/api/v1/public/products/{slug}", get(public_product))
        .route("/api/v1/public/categories", get(public_categories))
}
