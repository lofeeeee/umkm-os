//! Website builder HTTP (P2-2/P2-3/P2-4/P2-7; ADR-0007, ADR-0014).
//!
//! Dashboard (`/api/v1/tenants/{tenant_id}/website/...`): sesi + membership + RBAC.
//! Publik (`/api/v1/public/site`): tenant HANYA dari Host (P1-4), hanya versi terbit,
//! tanpa sesi; section `enabled=false` dibuang di server.

use std::collections::{BTreeMap, BTreeSet};

use axum::{
    Json, Router,
    extract::{Path, Query, State},
    http::{HeaderValue, header},
    response::{IntoResponse, Response},
    routing::{delete, get, post},
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use umkm_application::website::editor::{Errors, check_assets, check_document};
use umkm_domain::website::is_valid_page_slug;
use umkm_infrastructure::{
    business_repo,
    storage::ObjectStorage,
    website_repo::{self as repo, WebsiteError},
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
    business::{BusinessView, StatusView, status_view, to_view},
    middleware::{HostTenant, ReqId},
    session::Member,
    upload::now_unix,
    validation::field_errors,
};

// ------------------------------------------------------------------ DTO

#[derive(Debug, Serialize, ToSchema)]
pub struct TemplateView {
    pub key: String,
    pub version: i32,
    pub name: String,
    pub category: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct TemplatePreview {
    pub key: String,
    pub version: i32,
    #[schema(value_type = Object)]
    pub content: Value,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct PageView {
    pub slug: String,
    pub title: String,
    pub draft_revision: i32,
    pub published_version: Option<i32>,
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct CreatePage {
    pub slug: String,
    pub title: String,
}

impl CreatePage {
    pub fn validate(&self) -> Result<(), ApiError> {
        slug_ok(&self.slug)?;
        let t = self.title.trim();
        if t.is_empty() || t.len() > 120 {
            return Err(crate::validation::one("title", "invalid_length"));
        }
        Ok(())
    }
}

#[derive(Debug, Serialize, ToSchema)]
pub struct WebsiteView {
    pub template_key: String,
    pub template_version: i32,
    /// `draft` | `published`
    pub status: String,
    pub pages: Vec<PageView>,
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct SelectTemplate {
    pub template_key: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct DraftView {
    #[schema(value_type = Object)]
    pub content: Value,
    pub revision: i32,
    pub published_version: Option<i32>,
    /// `asset:<uuid>` -> signed path (relatif ke origin API), untuk preview.
    pub assets: BTreeMap<String, String>,
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct SaveDraft {
    #[schema(value_type = Object)]
    pub content: Value,
    /// Revisi draft yang diedit (optimistic concurrency).
    pub base_revision: i32,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct SavedDraft {
    pub revision: i32,
}

#[derive(Debug, Deserialize, ToSchema, Default)]
#[serde(deny_unknown_fields)]
pub struct PublishRequest {
    /// Bila diisi, publish gagal 409 jika draft sudah berubah sejak revisi ini.
    pub revision: Option<i32>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct Published {
    pub version: i32,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct VersionView {
    pub version: i32,
    pub created_at_unix: i64,
    pub published_by: Option<Uuid>,
    pub is_current: bool,
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct RollbackRequest {
    pub version: i32,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct PublicPage {
    pub slug: String,
    pub version: i32,
    /// Hanya section `enabled = true`.
    #[schema(value_type = Object)]
    pub content: Value,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct PublicSite {
    pub page: PublicPage,
    pub business: BusinessView,
    pub status: StatusView,
    pub assets: BTreeMap<String, String>,
}

// ------------------------------------------------------------------ helper

fn internal(e: sqlx::Error) -> ApiError {
    tracing::error!(error = %e, "website query failed");
    ApiError::new(ErrorCode::InternalError)
}

fn not_found() -> ApiError {
    ApiError::new(ErrorCode::ResourceNotFound)
}

fn pool(state: &AppState) -> Result<&sqlx::PgPool, ApiError> {
    state
        .auth
        .as_ref()
        .map(|a| &a.pool)
        .or(state.db.as_ref())
        .ok_or_else(|| ApiError::new(ErrorCode::ServiceUnavailable))
}

fn files(state: &AppState) -> Result<&crate::state::FileService, ApiError> {
    state
        .files
        .as_deref()
        .ok_or_else(|| ApiError::new(ErrorCode::ServiceUnavailable))
}

fn reject(errs: Errors) -> ApiError {
    field_errors(errs.iter().map(|(p, c)| (p.as_str(), *c)))
}

fn repo_err(e: WebsiteError) -> ApiError {
    match e {
        WebsiteError::NotFound => not_found(),
        WebsiteError::StaleRevision { current } => ApiError(
            ErrorBody::new(ErrorCode::Conflict)
                .with_message("Draft sudah diubah orang lain; muat ulang")
                .with_details(serde_json::json!({ "current_revision": current })),
        ),
        WebsiteError::AlreadyExists => ApiError(
            ErrorBody::new(ErrorCode::Conflict).with_message("Halaman dengan slug ini sudah ada"),
        ),
        WebsiteError::CannotDeleteHome => ApiError(
            ErrorBody::new(ErrorCode::BadRequest).with_message("Halaman home tidak dapat dihapus"),
        ),
        WebsiteError::Db(e) => internal(e),
    }
}

fn slug_ok(slug: &str) -> Result<(), ApiError> {
    if is_valid_page_slug(slug) {
        Ok(())
    } else {
        Err(not_found())
    }
}

/// Semua `asset:<uuid>` yang muncul sebagai string di dokumen.
fn asset_ids(v: &Value, out: &mut BTreeSet<Uuid>) {
    match v {
        Value::String(s) => {
            if let Some(id) = s
                .strip_prefix("asset:")
                .and_then(|x| Uuid::parse_str(x).ok())
            {
                out.insert(id);
            }
        }
        Value::Array(a) => a.iter().for_each(|x| asset_ids(x, out)),
        Value::Object(m) => m.values().for_each(|x| asset_ids(x, out)),
        _ => {}
    }
}

/// Signed path untuk aset yang BENAR-BENAR milik tenant (yang lain diabaikan).
pub(crate) async fn signed_assets(
    state: &AppState,
    t: TenantId,
    ids: BTreeSet<Uuid>,
) -> Result<BTreeMap<String, String>, ApiError> {
    let f = files(state)?;
    let list: Vec<Uuid> = ids.into_iter().collect();
    let owned = f.store.existing(t, &list).await.map_err(|e| {
        tracing::error!(error = %e, "asset lookup failed");
        ApiError::new(ErrorCode::InternalError)
    })?;
    let now = now_unix();
    Ok(owned
        .into_iter()
        .map(|id| (format!("asset:{id}"), f.signer.sign(t, id, now)))
        .collect())
}

/// Validasi lengkap dokumen untuk tenant: skema, template, kepemilikan aset.
async fn validate_full(
    state: &AppState,
    t: TenantId,
    template: &str,
    doc: &Value,
) -> Result<(), Errors> {
    let v = check_document(doc, template)?;
    let ids: Vec<Uuid> = v.asset_refs.iter().copied().collect();
    let owned: BTreeSet<Uuid> = match files(state) {
        Ok(f) => f
            .store
            .existing(t, &ids)
            .await
            .unwrap_or_default()
            .into_iter()
            .collect(),
        Err(_) => BTreeSet::new(),
    };
    check_assets(&v, &owned)
}

async fn template_of(state: &AppState, t: TenantId) -> Result<String, ApiError> {
    repo::website_template_key(pool(state)?, t)
        .await
        .map_err(internal)?
        .ok_or_else(not_found)
}

fn website_view(s: repo::WebsiteState) -> WebsiteView {
    WebsiteView {
        template_key: s.template_key,
        template_version: s.template_version,
        status: s.status,
        pages: s
            .pages
            .into_iter()
            .map(|p| PageView {
                slug: p.slug,
                title: p.title,
                draft_revision: p.draft_revision,
                published_version: p.published_version,
            })
            .collect(),
    }
}

// ------------------------------------------------------------------ template (P2-2)

/// Katalog template aktif (versi terbaru per key; AC-WEB-001).
#[utoipa::path(
    get, path = "/api/v1/tenants/{tenant_id}/website/templates", tag = "website",
    params(("tenant_id" = Uuid, Path)),
    responses((status = 200, description = "Template aktif", body = SuccessEnvelope<Vec<TemplateView>>))
)]
pub async fn list_templates(
    State(state): State<AppState>,
    ReqId(id): ReqId,
    Member(actor): Member,
) -> Result<Json<SuccessEnvelope<Vec<TemplateView>>>, ApiError> {
    actor.require("website.read")?;
    let list = repo::list_templates(pool(&state)?)
        .await
        .map_err(internal)?
        .into_iter()
        .map(|t| TemplateView {
            key: t.key,
            version: t.version,
            name: t.name,
            category: t.category,
        })
        .collect();
    Ok(Json(SuccessEnvelope::new(list, &id)))
}

#[derive(Deserialize)]
pub struct TemplatePath {
    #[allow(dead_code)]
    tenant_id: Uuid,
    key: String,
}

/// Konten contoh template; tidak membuat/mengubah website tenant (AC-WEB-002).
#[utoipa::path(
    get, path = "/api/v1/tenants/{tenant_id}/website/templates/{key}/preview", tag = "website",
    params(("tenant_id" = Uuid, Path), ("key" = String, Path)),
    responses((status = 200, description = "Konten default template", body = SuccessEnvelope<TemplatePreview>))
)]
pub async fn preview_template(
    State(state): State<AppState>,
    ReqId(id): ReqId,
    Member(actor): Member,
    Path(p): Path<TemplatePath>,
) -> Result<Json<SuccessEnvelope<TemplatePreview>>, ApiError> {
    actor.require("website.read")?;
    let t = repo::find_template(pool(&state)?, &p.key)
        .await
        .map_err(internal)?
        .ok_or_else(not_found)?;
    Ok(Json(SuccessEnvelope::new(
        TemplatePreview {
            key: t.key,
            version: t.version,
            content: t.default_content,
        },
        &id,
    )))
}

/// Status website tenant (template + halaman).
#[utoipa::path(
    get, path = "/api/v1/tenants/{tenant_id}/website", tag = "website",
    params(("tenant_id" = Uuid, Path)),
    responses(
        (status = 200, description = "Website", body = SuccessEnvelope<WebsiteView>),
        (status = 404, description = "Belum memilih template", body = umkm_shared::http::ErrorEnvelope)
    )
)]
pub async fn get_website(
    State(state): State<AppState>,
    ReqId(id): ReqId,
    Member(actor): Member,
) -> Result<Json<SuccessEnvelope<WebsiteView>>, ApiError> {
    actor.require("website.read")?;
    let s = repo::state(pool(&state)?, actor.tenant_id)
        .await
        .map_err(internal)?
        .ok_or_else(not_found)?;
    Ok(Json(SuccessEnvelope::new(website_view(s), &id)))
}

/// Pilih template. Draft `home` direset ke konten template; versi terbit tidak berubah.
#[utoipa::path(
    put, path = "/api/v1/tenants/{tenant_id}/website", tag = "website",
    params(("tenant_id" = Uuid, Path)), request_body = SelectTemplate,
    responses((status = 200, description = "Template terpilih", body = SuccessEnvelope<WebsiteView>))
)]
pub async fn select_template(
    State(state): State<AppState>,
    ReqId(id): ReqId,
    ClientIp(ip): ClientIp,
    Member(actor): Member,
    Json(req): Json<SelectTemplate>,
) -> Result<Json<SuccessEnvelope<WebsiteView>>, ApiError> {
    actor.require("website.edit")?;
    let p = pool(&state)?;
    let tpl = repo::find_template(p, &req.template_key)
        .await
        .map_err(internal)?
        .ok_or_else(|| field_errors([("template_key", "unknown_template")]))?;
    let s = repo::select_template(p, &actor, &tpl, (Some(id.as_str()), Some(&ip)))
        .await
        .map_err(internal)?;
    Ok(Json(SuccessEnvelope::new(website_view(s), &id)))
}

// ------------------------------------------------------------------ draft (P2-3)

#[derive(Deserialize)]
pub struct PagePath {
    #[allow(dead_code)]
    tenant_id: Uuid,
    slug: String,
}

/// Buat halaman baru pada website tenant.
#[utoipa::path(
    post, path = "/api/v1/tenants/{tenant_id}/website/pages", tag = "website",
    params(("tenant_id" = Uuid, Path)), request_body = CreatePage,
    responses(
        (status = 201, description = "Halaman dibuat", body = SuccessEnvelope<PageView>),
        (status = 403, description = "Halaman kustom memerlukan paket Pro", body = umkm_shared::http::ErrorEnvelope),
        (status = 409, description = "Slug sudah ada", body = umkm_shared::http::ErrorEnvelope)
    )
)]
pub async fn create_page(
    State(state): State<AppState>,
    ReqId(id): ReqId,
    ClientIp(ip): ClientIp,
    Member(actor): Member,
    Json(req): Json<CreatePage>,
) -> Result<(axum::http::StatusCode, Json<SuccessEnvelope<PageView>>), ApiError> {
    actor.require("website.edit")?;
    req.validate()?;

    let plan = crate::billing::plan_of(&state, actor.tenant_id).await?;
    if plan.plan.code != "pro" && !["home", "about", "contact"].contains(&req.slug.as_str()) {
        return Err(ApiError(
            ErrorBody::new(ErrorCode::Forbidden)
                .with_message("Halaman kustom memerlukan paket Pro"),
        ));
    }

    let tpl_key = template_of(&state, actor.tenant_id).await?;
    let initial_content = if let Ok(Some(tpl)) = repo::find_template(pool(&state)?, &tpl_key).await
    {
        tpl.default_content
    } else {
        serde_json::json!({
            "schema_version": 1,
            "template": tpl_key,
            "theme": { "primary": "#2563EB", "font": "inter" },
            "sections": []
        })
    };

    let p = repo::create_page(
        pool(&state)?,
        &actor,
        &req.slug,
        req.title.trim(),
        &initial_content,
        (Some(id.as_str()), Some(&ip)),
    )
    .await
    .map_err(repo_err)?;

    let view = PageView {
        slug: p.slug,
        title: p.title,
        draft_revision: p.draft_revision,
        published_version: p.published_version,
    };

    Ok((
        axum::http::StatusCode::CREATED,
        Json(SuccessEnvelope::new(view, &id)),
    ))
}

/// Hapus halaman website tenant.
#[utoipa::path(
    delete, path = "/api/v1/tenants/{tenant_id}/website/pages/{slug}", tag = "website",
    params(("tenant_id" = Uuid, Path), ("slug" = String, Path)),
    responses(
        (status = 200, description = "Halaman dihapus"),
        (status = 400, description = "Halaman home tidak dapat dihapus", body = umkm_shared::http::ErrorEnvelope),
        (status = 404, description = "Halaman tidak ditemukan", body = umkm_shared::http::ErrorEnvelope)
    )
)]
pub async fn delete_page(
    State(state): State<AppState>,
    ReqId(id): ReqId,
    ClientIp(ip): ClientIp,
    Member(actor): Member,
    Path(p): Path<PagePath>,
) -> Result<Json<SuccessEnvelope<()>>, ApiError> {
    actor.require("website.edit")?;
    if p.slug == repo::HOME || p.slug == "home" {
        return Err(ApiError(
            ErrorBody::new(ErrorCode::BadRequest).with_message("Halaman home tidak dapat dihapus"),
        ));
    }
    repo::delete_page(
        pool(&state)?,
        &actor,
        &p.slug,
        (Some(id.as_str()), Some(&ip)),
    )
    .await
    .map_err(repo_err)?;

    Ok(Json(SuccessEnvelope::new((), &id)))
}

/// Draft halaman (untuk editor dan preview di dashboard; AC-WEB-010/011).
#[utoipa::path(
    get, path = "/api/v1/tenants/{tenant_id}/website/pages/{slug}/draft", tag = "website",
    params(("tenant_id" = Uuid, Path), ("slug" = String, Path)),
    responses((status = 200, description = "Draft", body = SuccessEnvelope<DraftView>))
)]
pub async fn get_draft(
    State(state): State<AppState>,
    ReqId(id): ReqId,
    Member(actor): Member,
    Path(p): Path<PagePath>,
) -> Result<Json<SuccessEnvelope<DraftView>>, ApiError> {
    actor.require("website.read")?;
    slug_ok(&p.slug)?;
    let d = repo::get_draft(pool(&state)?, actor.tenant_id, &p.slug)
        .await
        .map_err(internal)?
        .ok_or_else(not_found)?;
    let mut ids = BTreeSet::new();
    asset_ids(&d.content, &mut ids);
    let assets = signed_assets(&state, actor.tenant_id, ids).await?;
    Ok(Json(SuccessEnvelope::new(
        DraftView {
            content: d.content,
            revision: d.revision,
            published_version: d.published_version,
            assets,
        },
        &id,
    )))
}

/// Simpan draft tervalidasi (AC-WEB-004/005/007/009/010). 409 bila revisi basi.
#[utoipa::path(
    put, path = "/api/v1/tenants/{tenant_id}/website/pages/{slug}/draft", tag = "website",
    params(("tenant_id" = Uuid, Path), ("slug" = String, Path)), request_body = SaveDraft,
    responses(
        (status = 200, description = "Tersimpan", body = SuccessEnvelope<SavedDraft>),
        (status = 409, description = "Revisi basi", body = umkm_shared::http::ErrorEnvelope),
        (status = 422, description = "Dokumen tidak valid", body = umkm_shared::http::ErrorEnvelope)
    )
)]
pub async fn save_draft(
    State(state): State<AppState>,
    ReqId(id): ReqId,
    ClientIp(ip): ClientIp,
    Member(actor): Member,
    Path(p): Path<PagePath>,
    Json(req): Json<SaveDraft>,
) -> Result<Json<SuccessEnvelope<SavedDraft>>, ApiError> {
    actor.require("website.edit")?;
    slug_ok(&p.slug)?;
    let template = template_of(&state, actor.tenant_id).await?;
    validate_full(&state, actor.tenant_id, &template, &req.content)
        .await
        .map_err(reject)?;
    let rev = repo::save_draft(
        pool(&state)?,
        &actor,
        &p.slug,
        &req.content,
        req.base_revision,
        (Some(id.as_str()), Some(&ip)),
    )
    .await
    .map_err(repo_err)?;
    Ok(Json(SuccessEnvelope::new(
        SavedDraft { revision: rev },
        &id,
    )))
}

// ------------------------------------------------------------------ publish (P2-4)

/// Terbitkan draft sebagai versi immutable baru (AC-WEB-012). Butuh `website.publish`.
#[utoipa::path(
    post, path = "/api/v1/tenants/{tenant_id}/website/pages/{slug}/publish", tag = "website",
    params(("tenant_id" = Uuid, Path), ("slug" = String, Path)), request_body = PublishRequest,
    responses(
        (status = 200, description = "Versi baru", body = SuccessEnvelope<Published>),
        (status = 409, description = "Draft berubah sejak revisi", body = umkm_shared::http::ErrorEnvelope),
        (status = 422, description = "Draft tidak valid", body = umkm_shared::http::ErrorEnvelope)
    )
)]
pub async fn publish(
    State(state): State<AppState>,
    ReqId(id): ReqId,
    ClientIp(ip): ClientIp,
    Member(actor): Member,
    Path(p): Path<PagePath>,
    body: Option<Json<PublishRequest>>,
) -> Result<Json<SuccessEnvelope<Published>>, ApiError> {
    actor.require("website.publish")?;
    slug_ok(&p.slug)?;
    let req = body.map(|b| b.0).unwrap_or_default();
    let template = template_of(&state, actor.tenant_id).await?;
    // Validasi aset di luar transaksi publish (I/O terpisah), lalu skema+template di dalam
    // transaksi atas draft yang dikunci. Aset tidak dapat hilang di antaranya: objek yang
    // dirujuk draft tidak dapat dihapus (P2-6).
    let draft = repo::get_draft(pool(&state)?, actor.tenant_id, &p.slug)
        .await
        .map_err(internal)?
        .ok_or_else(not_found)?;
    validate_full(&state, actor.tenant_id, &template, &draft.content)
        .await
        .map_err(reject)?;
    let out = repo::publish(
        pool(&state)?,
        &actor,
        &p.slug,
        req.revision,
        |doc| check_document(doc, &template).map(|_| ()),
        (Some(id.as_str()), Some(&ip)),
    )
    .await
    .map_err(repo_err)?
    .map_err(reject)?;
    tracing::info!(tenant_id = %actor.tenant_id, slug = %p.slug, version = out, "website published");
    Ok(Json(SuccessEnvelope::new(Published { version: out }, &id)))
}

/// Riwayat versi terbit (terbaru dulu).
#[utoipa::path(
    get, path = "/api/v1/tenants/{tenant_id}/website/pages/{slug}/versions", tag = "website",
    params(("tenant_id" = Uuid, Path), ("slug" = String, Path)),
    responses((status = 200, description = "Versi", body = SuccessEnvelope<Vec<VersionView>>))
)]
pub async fn list_versions(
    State(state): State<AppState>,
    ReqId(id): ReqId,
    Member(actor): Member,
    Path(p): Path<PagePath>,
) -> Result<Json<SuccessEnvelope<Vec<VersionView>>>, ApiError> {
    actor.require("website.read")?;
    slug_ok(&p.slug)?;
    let list = repo::list_versions(pool(&state)?, actor.tenant_id, &p.slug)
        .await
        .map_err(internal)?
        .ok_or_else(not_found)?
        .into_iter()
        .map(|v| VersionView {
            version: v.version,
            created_at_unix: v.created_at_unix,
            published_by: v.published_by,
            is_current: v.is_current,
        })
        .collect();
    Ok(Json(SuccessEnvelope::new(list, &id)))
}

#[derive(Deserialize)]
pub struct VersionPath {
    #[allow(dead_code)]
    tenant_id: Uuid,
    slug: String,
    version: i32,
}

/// Isi satu versi terbit (preview sebelum rollback).
#[utoipa::path(
    get, path = "/api/v1/tenants/{tenant_id}/website/pages/{slug}/versions/{version}", tag = "website",
    params(("tenant_id" = Uuid, Path), ("slug" = String, Path), ("version" = i32, Path)),
    responses((status = 200, description = "Konten versi", body = SuccessEnvelope<DraftView>))
)]
pub async fn get_version(
    State(state): State<AppState>,
    ReqId(id): ReqId,
    Member(actor): Member,
    Path(p): Path<VersionPath>,
) -> Result<Json<SuccessEnvelope<DraftView>>, ApiError> {
    actor.require("website.read")?;
    slug_ok(&p.slug)?;
    let content = repo::get_version(pool(&state)?, actor.tenant_id, &p.slug, p.version)
        .await
        .map_err(internal)?
        .ok_or_else(not_found)?;
    let mut ids = BTreeSet::new();
    asset_ids(&content, &mut ids);
    let assets = signed_assets(&state, actor.tenant_id, ids).await?;
    Ok(Json(SuccessEnvelope::new(
        DraftView {
            content,
            revision: 0,
            published_version: Some(p.version),
            assets,
        },
        &id,
    )))
}

/// Kembalikan publik ke versi terpilih (AC-WEB-014). Butuh `website.publish`.
#[utoipa::path(
    post, path = "/api/v1/tenants/{tenant_id}/website/pages/{slug}/rollback", tag = "website",
    params(("tenant_id" = Uuid, Path), ("slug" = String, Path)), request_body = RollbackRequest,
    responses(
        (status = 200, description = "Versi aktif", body = SuccessEnvelope<Published>),
        (status = 404, description = "Versi tidak ada untuk halaman ini", body = umkm_shared::http::ErrorEnvelope)
    )
)]
pub async fn rollback(
    State(state): State<AppState>,
    ReqId(id): ReqId,
    ClientIp(ip): ClientIp,
    Member(actor): Member,
    Path(p): Path<PagePath>,
    Json(req): Json<RollbackRequest>,
) -> Result<Json<SuccessEnvelope<Published>>, ApiError> {
    actor.require("website.publish")?;
    slug_ok(&p.slug)?;
    repo::rollback(
        pool(&state)?,
        &actor,
        &p.slug,
        req.version,
        (Some(id.as_str()), Some(&ip)),
    )
    .await
    .map_err(repo_err)?;
    Ok(Json(SuccessEnvelope::new(
        Published {
            version: req.version,
        },
        &id,
    )))
}

// ------------------------------------------------------------------ publik (P2-7)

#[derive(Debug, Deserialize)]
pub struct SiteQuery {
    page: Option<String>,
}

/// Hanya section aktif; versi terbit sudah tervalidasi saat publish.
fn strip_disabled(mut content: Value) -> Value {
    if let Some(Value::Array(items)) = content.get_mut("sections") {
        items.retain(|s| s.get("enabled").and_then(Value::as_bool) == Some(true));
    }
    content
}

/// Situs publik tenant dari Host (ADR-0014). Tanpa sesi; cache publik 60 detik.
#[utoipa::path(
    get, path = "/api/v1/public/site", tag = "public",
    params(("page" = Option<String>, Query, description = "Slug halaman, default `home`")),
    responses(
        (status = 200, description = "Versi terbit + data bisnis", body = SuccessEnvelope<PublicSite>),
        (status = 404, description = "Host tidak dikenal/nonaktif atau belum terbit", body = umkm_shared::http::ErrorEnvelope)
    )
)]
pub async fn public_site(
    State(state): State<AppState>,
    ReqId(id): ReqId,
    HostTenant(t): HostTenant,
    Query(q): Query<SiteQuery>,
) -> Result<Response, ApiError> {
    // ADR-0016: langganan habis → website/katalog/checkout publik nonaktif.
    crate::billing::require_operational(&state, t).await?;
    let slug = q.page.unwrap_or_else(|| repo::HOME.into());
    slug_ok(&slug)?;
    let p = pool(&state)?;
    let (version, content) = repo::published_content(p, t, &slug)
        .await
        .map_err(internal)?
        .ok_or_else(not_found)?;
    let mut content = strip_disabled(content);
    // ADR-0019 (P9-11): blok plugin Pro tidak dirender untuk toko tanpa Pro (draf & versi tetap utuh).
    let has_pro = umkm_infrastructure::billing_repo::current(p, t)
        .await
        .map(|c| c.plan.code == "pro")
        .unwrap_or(false);
    umkm_domain::website::schema_v2::hide_unentitled_blocks(
        &mut content,
        has_pro,
        umkm_domain::website::schema_v2::plugin_plan,
    );
    let info = business_repo::load(p, t)
        .await
        .map_err(internal)?
        .ok_or_else(not_found)?;
    let status = business_repo::status(p, t, None)
        .await
        .map_err(internal)?
        .ok_or_else(not_found)?;
    let mut ids = BTreeSet::new();
    asset_ids(&content, &mut ids);
    ids.extend(info.profile.logo_object_id);
    ids.extend(info.profile.cover_object_id);
    let assets = signed_assets(&state, t, ids).await?;
    let body = PublicSite {
        page: PublicPage {
            slug,
            version,
            content,
        },
        business: to_view(info),
        status: status_view(status),
        assets,
    };
    let mut res = Json(SuccessEnvelope::new(body, &id)).into_response();
    res.headers_mut().insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static("public, max-age=60"),
    );
    res.headers_mut()
        .insert(header::VARY, HeaderValue::from_static("Host"));
    Ok(res)
}

pub fn routes() -> Router<AppState> {
    let t = "/api/v1/tenants/{tenant_id}/website";
    Router::new()
        .route(t, get(get_website).put(select_template))
        .route(&format!("{t}/templates"), get(list_templates))
        .route(
            &format!("{t}/templates/{{key}}/preview"),
            get(preview_template),
        )
        .route(&format!("{t}/pages"), post(create_page))
        .route(&format!("{t}/pages/{{slug}}"), delete(delete_page))
        .route(
            &format!("{t}/pages/{{slug}}/draft"),
            get(get_draft).put(save_draft),
        )
        .route(&format!("{t}/pages/{{slug}}/publish"), post(publish))
        .route(&format!("{t}/pages/{{slug}}/versions"), get(list_versions))
        .route(
            &format!("{t}/pages/{{slug}}/versions/{{version}}"),
            get(get_version),
        )
        .route(&format!("{t}/pages/{{slug}}/rollback"), post(rollback))
        .route("/api/v1/public/site", get(public_site))
}
