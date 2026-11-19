//! Website builder persistence (P2-2/P2-3/P2-4, `PRD.md §4.3`, ADR-0007, ADR-0014).
//!
//! - Draft disimpan di `website_pages.draft_content` dengan `draft_revision` (optimistic
//!   concurrency: dua editor tidak saling menimpa diam-diam).
//! - Publish = INSERT versi immutable + pindah pointer dalam SATU transaksi (ADR-0014 #5).
//! - Rollback = pindah pointer ke versi lama halaman yang sama (composite FK menjamin).
//! - Validasi isi dilakukan pemanggil (domain `validate_page`) SEBELUM fungsi tulis.

use serde_json::Value;
use sqlx::{PgPool, Postgres, Row, Transaction, types::Json};
use umkm_domain::{audit::AuditEvent, rbac::Actor};
use umkm_shared::ids::TenantId;
use uuid::Uuid;

pub mod actions {
    pub const TEMPLATE_SELECTED: &str = "website.template_selected";
    pub const DRAFT_SAVED: &str = "website.draft_saved";
    pub const PUBLISHED: &str = "website.publish";
    pub const ROLLED_BACK: &str = "website.rollback";
    pub const PAGE_CREATED: &str = "website.page_created";
    pub const PAGE_DELETED: &str = "website.page_deleted";
    pub const THEME_SAVED: &str = "website.theme_saved";
    pub const PAGES_REORDERED: &str = "website.pages_reordered";
}

pub const HOME: &str = "home";

#[derive(Debug, Clone, PartialEq)]
pub struct Template {
    pub id: Uuid,
    pub key: String,
    pub version: i32,
    pub name: String,
    pub category: String,
    pub default_content: Value,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PageSummary {
    pub slug: String,
    pub title: String,
    pub draft_revision: i32,
    pub published_version: Option<i32>,
}

pub type PageView = PageSummary;

#[derive(Debug, Clone, PartialEq)]
pub struct WebsiteState {
    pub template_key: String,
    pub template_version: i32,
    pub status: String,
    pub pages: Vec<PageSummary>,
    /// Draf tema situs (brand kit). `None` = belum pernah diatur (tema per halaman dipakai).
    pub theme: Option<Value>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Draft {
    pub content: Value,
    pub revision: i32,
    pub published_version: Option<i32>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VersionSummary {
    pub version: i32,
    pub created_at_unix: i64,
    pub published_by: Option<Uuid>,
    pub is_current: bool,
}

#[derive(Debug, thiserror::Error)]
pub enum WebsiteError {
    #[error("tidak ditemukan")]
    NotFound,
    #[error("draft telah berubah (revisi {current})")]
    StaleRevision { current: i32 },
    #[error("halaman sudah ada")]
    AlreadyExists,
    #[error("halaman home tidak dapat dihapus")]
    CannotDeleteHome,
    #[error("batas {0} halaman tercapai")]
    PageLimit(usize),
    #[error(transparent)]
    Db(#[from] sqlx::Error),
}

type Ctx<'a> = (Option<&'a str>, Option<&'a str>);

fn db_err(e: umkm_database::DbError) -> sqlx::Error {
    match e {
        umkm_database::DbError::Sqlx(e) => e,
        umkm_database::DbError::InvalidUrl => sqlx::Error::Configuration("invalid url".into()),
    }
}

async fn tx(pool: &PgPool, t: TenantId) -> Result<Transaction<'static, Postgres>, sqlx::Error> {
    umkm_database::begin_tenant_tx(pool, t)
        .await
        .map_err(db_err)
}

async fn audit(
    tx: &mut Transaction<'static, Postgres>,
    actor: &Actor,
    action: &'static str,
    resource: impl ToString,
    summary: Value,
    (request_id, ip): Ctx<'_>,
) -> Result<(), sqlx::Error> {
    crate::audit_repo::record(
        &mut **tx,
        &AuditEvent::new(action, "website_page")
            .by_user(actor.user_id)
            .in_tenant(actor.tenant_id)
            .resource(resource)
            .request(request_id, ip)
            .with_summary(summary),
    )
    .await
}

fn template(r: &sqlx::postgres::PgRow) -> Template {
    let Json(c): Json<Value> = r.get("default_content");
    Template {
        id: r.get("id"),
        key: r.get("key"),
        version: r.get("version"),
        name: r.get("name"),
        category: r.get("category"),
        default_content: c,
    }
}

/// Versi aktif terbaru per key (katalog global, hanya-baca).
pub async fn list_templates(pool: &PgPool) -> Result<Vec<Template>, sqlx::Error> {
    let rows = sqlx::query(
        "SELECT DISTINCT ON (key) id, key, version, name, category, default_content
         FROM website_templates WHERE status = 'active' ORDER BY key, version DESC",
    )
    .fetch_all(pool)
    .await?;
    Ok(rows.iter().map(template).collect())
}

pub async fn find_template(pool: &PgPool, key: &str) -> Result<Option<Template>, sqlx::Error> {
    let row = sqlx::query(
        "SELECT id, key, version, name, category, default_content FROM website_templates
         WHERE status = 'active' AND key = $1 ORDER BY version DESC LIMIT 1",
    )
    .bind(key)
    .fetch_optional(pool)
    .await?;
    Ok(row.as_ref().map(template))
}

pub async fn state(pool: &PgPool, t: TenantId) -> Result<Option<WebsiteState>, sqlx::Error> {
    let mut tx = tx(pool, t).await?;
    let s = state_in(&mut tx, t).await?;
    tx.commit().await?;
    Ok(s)
}

async fn state_in(
    tx: &mut Transaction<'static, Postgres>,
    t: TenantId,
) -> Result<Option<WebsiteState>, sqlx::Error> {
    let Some(w) = sqlx::query(
        "SELECT w.id, w.status, w.theme_draft, tpl.key, tpl.version FROM tenant_websites w
         JOIN website_templates tpl ON tpl.id = w.template_id WHERE w.tenant_id = $1",
    )
    .bind(t.as_uuid())
    .fetch_optional(&mut **tx)
    .await?
    else {
        return Ok(None);
    };
    let pages = sqlx::query(
        // `home` selalu lebih dulu: urutan ini dipakai juga oleh navigasi situs publik dan panel
        // halaman Studio, jadi beranda tidak boleh muncul di tengah karena urutan abjad slug.
        "SELECT p.slug, p.title, p.draft_revision, v.version AS published_version
         FROM website_pages p LEFT JOIN website_page_versions v ON v.id = p.published_version_id
         WHERE p.tenant_id = $1 AND p.website_id = $2
         ORDER BY (p.slug = 'home') DESC, p.nav_position NULLS LAST, p.slug",
    )
    .bind(t.as_uuid())
    .bind(w.get::<Uuid, _>("id"))
    .fetch_all(&mut **tx)
    .await?
    .into_iter()
    .map(|r| PageSummary {
        slug: r.get("slug"),
        title: r.get("title"),
        draft_revision: r.get("draft_revision"),
        published_version: r.get("published_version"),
    })
    .collect();
    Ok(Some(WebsiteState {
        template_key: w.get("key"),
        template_version: w.get("version"),
        status: w.get("status"),
        pages,
        theme: w
            .get::<Option<Json<Value>>, _>("theme_draft")
            .map(|Json(v)| v),
    }))
}

/// Pilih template (AC-WEB-003, `PRD.md §13.3` "Template dapat dipilih").
/// Website baru: buat website + halaman `home` dengan draft = konten default template.
/// Website ada: ganti template dan reset DRAFT `home` ke default template; versi terbit
/// tidak disentuh (publik tetap menampilkan versi terbit sampai publish berikutnya).
pub async fn select_template(
    pool: &PgPool,
    actor: &Actor,
    tpl: &Template,
    ctx: Ctx<'_>,
) -> Result<WebsiteState, sqlx::Error> {
    let mut tx = tx(pool, actor.tenant_id).await?;
    let tid = actor.tenant_id.as_uuid();
    let website: Uuid = sqlx::query_scalar(
        "INSERT INTO tenant_websites (tenant_id, template_id) VALUES ($1, $2)
         ON CONFLICT (tenant_id) DO UPDATE SET template_id = EXCLUDED.template_id
         RETURNING id",
    )
    .bind(tid)
    .bind(tpl.id)
    .fetch_one(&mut *tx)
    .await?;
    let page: Uuid = sqlx::query_scalar(
        "INSERT INTO website_pages (tenant_id, website_id, slug, title, draft_content, draft_updated_by)
         VALUES ($1, $2, $3, 'Beranda', $4, $5)
         ON CONFLICT (tenant_id, website_id, slug) DO UPDATE
           SET draft_content = EXCLUDED.draft_content, draft_updated_by = EXCLUDED.draft_updated_by,
               draft_revision = website_pages.draft_revision + 1
         RETURNING id",
    )
    .bind(tid)
    .bind(website)
    .bind(HOME)
    .bind(Json(&tpl.default_content))
    .bind(actor.user_id)
    .fetch_one(&mut *tx)
    .await?;
    audit(
        &mut tx,
        actor,
        actions::TEMPLATE_SELECTED,
        page,
        serde_json::json!({ "template": tpl.key, "version": tpl.version }),
        ctx,
    )
    .await?;
    let s = state_in(&mut tx, actor.tenant_id)
        .await?
        .ok_or(sqlx::Error::RowNotFound)?;
    tx.commit().await?;
    Ok(s)
}

pub async fn get_draft(
    pool: &PgPool,
    t: TenantId,
    slug: &str,
) -> Result<Option<Draft>, sqlx::Error> {
    let mut tx = tx(pool, t).await?;
    let row = sqlx::query(
        "SELECT p.draft_content, p.draft_revision, v.version AS published_version
         FROM website_pages p LEFT JOIN website_page_versions v ON v.id = p.published_version_id
         WHERE p.tenant_id = $1 AND p.slug = $2",
    )
    .bind(t.as_uuid())
    .bind(slug)
    .fetch_optional(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(row.map(|r| {
        let Json(content): Json<Value> = r.get("draft_content");
        Draft {
            content,
            revision: r.get("draft_revision"),
            published_version: r.get("published_version"),
        }
    }))
}

/// Template key website tenant (untuk mencocokkan `content_json.template`).
pub async fn website_template_key(
    pool: &PgPool,
    t: TenantId,
) -> Result<Option<String>, sqlx::Error> {
    Ok(state(pool, t).await?.map(|s| s.template_key))
}

/// Simpan draft yang SUDAH divalidasi. `base_revision` harus sama dengan revisi saat ini.
pub async fn save_draft(
    pool: &PgPool,
    actor: &Actor,
    slug: &str,
    content: &Value,
    base_revision: i32,
    ctx: Ctx<'_>,
) -> Result<i32, WebsiteError> {
    let mut tx = tx(pool, actor.tenant_id).await?;
    let row = sqlx::query(
        "SELECT id, draft_revision FROM website_pages WHERE tenant_id = $1 AND slug = $2 FOR UPDATE",
    )
    .bind(actor.tenant_id.as_uuid())
    .bind(slug)
    .fetch_optional(&mut *tx)
    .await?
    .ok_or(WebsiteError::NotFound)?;
    let (page, current): (Uuid, i32) = (row.get("id"), row.get("draft_revision"));
    if current != base_revision {
        return Err(WebsiteError::StaleRevision { current });
    }
    let rev: i32 = sqlx::query_scalar(
        "UPDATE website_pages SET draft_content = $3, draft_revision = draft_revision + 1,
            draft_updated_by = $4
         WHERE tenant_id = $1 AND id = $2 RETURNING draft_revision",
    )
    .bind(actor.tenant_id.as_uuid())
    .bind(page)
    .bind(Json(content))
    .bind(actor.user_id)
    .fetch_one(&mut *tx)
    .await?;
    audit(
        &mut tx,
        actor,
        actions::DRAFT_SAVED,
        page,
        serde_json::json!({ "slug": slug, "revision": rev }),
        ctx,
    )
    .await?;
    tx.commit().await?;
    Ok(rev)
}

/// Buat halaman baru pada website tenant.
/// `max_pages`: batas jumlah halaman (termasuk Beranda) menurut paket. Baris website dikunci supaya dua
/// permintaan bersamaan tidak bisa melewati batas.
pub async fn create_page(
    pool: &PgPool,
    actor: &Actor,
    slug: &str,
    title: &str,
    initial_content: &Value,
    max_pages: usize,
    ctx: Ctx<'_>,
) -> Result<PageView, WebsiteError> {
    let mut tx = tx(pool, actor.tenant_id).await?;
    let tid = actor.tenant_id.as_uuid();
    let website_id: Uuid =
        sqlx::query_scalar("SELECT id FROM tenant_websites WHERE tenant_id = $1 FOR UPDATE")
            .bind(tid)
            .fetch_optional(&mut *tx)
            .await?
            .ok_or(WebsiteError::NotFound)?;
    let pages: i64 = sqlx::query_scalar(
        // Halaman sistem (keranjang/pesanan/akun) tidak dihitung dalam batas paket.
        "SELECT count(*) FROM website_pages WHERE website_id = $1 AND slug NOT IN ('keranjang', 'pesanan', 'akun')",
    )
        .bind(website_id)
        .fetch_one(&mut *tx)
        .await?;
    if pages as usize >= max_pages {
        return Err(WebsiteError::PageLimit(max_pages));
    }

    let page_id: Uuid = match sqlx::query_scalar(
        "INSERT INTO website_pages (tenant_id, website_id, slug, title, draft_content, draft_updated_by)
         VALUES ($1, $2, $3, $4, $5, $6)
         RETURNING id",
    )
    .bind(tid)
    .bind(website_id)
    .bind(slug)
    .bind(title)
    .bind(Json(initial_content))
    .bind(actor.user_id)
    .fetch_one(&mut *tx)
    .await
    {
        Ok(id) => id,
        Err(e) if crate::pgx::sqlstate(&e).as_deref() == Some("23505") => {
            return Err(WebsiteError::AlreadyExists);
        }
        Err(e) => return Err(WebsiteError::Db(e)),
    };

    audit(
        &mut tx,
        actor,
        actions::PAGE_CREATED,
        page_id,
        serde_json::json!({ "slug": slug, "title": title }),
        ctx,
    )
    .await?;
    tx.commit().await?;

    Ok(PageView {
        slug: slug.to_string(),
        title: title.to_string(),
        draft_revision: 1,
        published_version: None,
    })
}

/// Hapus halaman website tenant berdasarkan slug. Halaman "home" tidak boleh dihapus.
pub async fn delete_page(
    pool: &PgPool,
    actor: &Actor,
    slug: &str,
    ctx: Ctx<'_>,
) -> Result<(), WebsiteError> {
    if slug == HOME || umkm_domain::website::is_system_slug(slug) {
        return Err(WebsiteError::CannotDeleteHome);
    }
    let mut tx = tx(pool, actor.tenant_id).await?;
    let tid = actor.tenant_id.as_uuid();
    let page_id: Option<Uuid> = sqlx::query_scalar(
        "DELETE FROM website_pages WHERE tenant_id = $1 AND slug = $2 RETURNING id",
    )
    .bind(tid)
    .bind(slug)
    .fetch_optional(&mut *tx)
    .await?;

    let Some(page_id) = page_id else {
        return Err(WebsiteError::NotFound);
    };

    audit(
        &mut tx,
        actor,
        actions::PAGE_DELETED,
        page_id,
        serde_json::json!({ "slug": slug }),
        ctx,
    )
    .await?;
    tx.commit().await?;
    Ok(())
}

/// Simpan urutan menu halaman (urutan `slugs` = urutan tampil). Slug yang tidak ada diabaikan;
/// halaman yang tidak disebut kembali ke urutan bawaan (setelah yang diurutkan). Beranda tetap pertama.
pub async fn reorder_pages(
    pool: &PgPool,
    actor: &Actor,
    slugs: &[String],
    ctx: Ctx<'_>,
) -> Result<(), WebsiteError> {
    let mut tx = tx(pool, actor.tenant_id).await?;
    let tid = actor.tenant_id.as_uuid();
    let site: Uuid = sqlx::query_scalar("SELECT id FROM tenant_websites WHERE tenant_id = $1")
        .bind(tid)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or(WebsiteError::NotFound)?;
    sqlx::query(
        "UPDATE website_pages p
         SET nav_position = o.pos
         FROM (SELECT s.slug, s.ord::int - 1 AS pos FROM unnest($2::text[]) WITH ORDINALITY AS s(slug, ord)) o
         WHERE p.tenant_id = $1 AND p.slug = o.slug",
    )
    .bind(tid)
    .bind(slugs)
    .execute(&mut *tx)
    .await?;
    sqlx::query(
        "UPDATE website_pages SET nav_position = NULL
         WHERE tenant_id = $1 AND NOT (slug = ANY($2::text[])) AND nav_position IS NOT NULL",
    )
    .bind(tid)
    .bind(slugs)
    .execute(&mut *tx)
    .await?;
    audit(
        &mut tx,
        actor,
        actions::PAGES_REORDERED,
        site,
        serde_json::json!({ "slugs": slugs }),
        ctx,
    )
    .await?;
    tx.commit().await?;
    Ok(())
}

/// Publish: `validate` dipanggil ulang atas draft di dalam transaksi (draft bisa berubah
/// sejak terakhir divalidasi; data lama di DB tidak dipercaya begitu saja).
pub async fn publish<F>(
    pool: &PgPool,
    actor: &Actor,
    slug: &str,
    expected_revision: Option<i32>,
    validate: F,
    ctx: Ctx<'_>,
) -> Result<Result<i32, Vec<(String, &'static str)>>, WebsiteError>
where
    F: FnOnce(&Value) -> Result<(), Vec<(String, &'static str)>>,
{
    let mut tx = tx(pool, actor.tenant_id).await?;
    let tid = actor.tenant_id.as_uuid();
    let row = sqlx::query(
        "SELECT id, website_id, draft_content, draft_revision FROM website_pages
         WHERE tenant_id = $1 AND slug = $2 FOR UPDATE",
    )
    .bind(tid)
    .bind(slug)
    .fetch_optional(&mut *tx)
    .await?
    .ok_or(WebsiteError::NotFound)?;
    let (page, website, rev): (Uuid, Uuid, i32) = (
        row.get("id"),
        row.get("website_id"),
        row.get("draft_revision"),
    );
    if let Some(exp) = expected_revision
        && exp != rev
    {
        return Err(WebsiteError::StaleRevision { current: rev });
    }
    let Json(content): Json<Value> = row.get("draft_content");
    if let Err(errs) = validate(&content) {
        return Ok(Err(errs));
    }
    let version: i32 = sqlx::query_scalar(
        "SELECT coalesce(max(version), 0) + 1 FROM website_page_versions WHERE tenant_id = $1 AND page_id = $2",
    )
    .bind(tid)
    .bind(page)
    .fetch_one(&mut *tx)
    .await?;
    let ver: Uuid = sqlx::query_scalar(
        "INSERT INTO website_page_versions (tenant_id, page_id, version, content_json, published_by)
         VALUES ($1, $2, $3, $4, $5) RETURNING id",
    )
    .bind(tid)
    .bind(page)
    .bind(version)
    .bind(Json(&content))
    .bind(actor.user_id)
    .fetch_one(&mut *tx)
    .await?;
    sqlx::query(
        "UPDATE website_pages SET published_version_id = $3 WHERE tenant_id = $1 AND id = $2",
    )
    .bind(tid)
    .bind(page)
    .bind(ver)
    .execute(&mut *tx)
    .await?;
    // Tema situs ikut terbit bersama halaman mana pun (brand kit berlaku untuk seluruh situs).
    sqlx::query("UPDATE tenant_websites SET status = 'published', theme_published = coalesce(theme_draft, theme_published) WHERE tenant_id = $1 AND id = $2")
        .bind(tid)
        .bind(website)
        .execute(&mut *tx)
        .await?;
    audit(
        &mut tx,
        actor,
        actions::PUBLISHED,
        page,
        serde_json::json!({ "slug": slug, "version": version, "draft_revision": rev }),
        ctx,
    )
    .await?;
    tx.commit().await?;
    Ok(Ok(version))
}

pub async fn list_versions(
    pool: &PgPool,
    t: TenantId,
    slug: &str,
) -> Result<Option<Vec<VersionSummary>>, sqlx::Error> {
    let mut tx = tx(pool, t).await?;
    let page: Option<(Uuid, Option<Uuid>)> = sqlx::query_as(
        "SELECT id, published_version_id FROM website_pages WHERE tenant_id = $1 AND slug = $2",
    )
    .bind(t.as_uuid())
    .bind(slug)
    .fetch_optional(&mut *tx)
    .await?;
    let Some((page, current)) = page else {
        return Ok(None);
    };
    let rows = sqlx::query(
        "SELECT id, version, published_by, extract(epoch FROM created_at)::bigint AS created_unix
         FROM website_page_versions WHERE tenant_id = $1 AND page_id = $2 ORDER BY version DESC LIMIT 200",
    )
    .bind(t.as_uuid())
    .bind(page)
    .fetch_all(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(Some(
        rows.into_iter()
            .map(|r| VersionSummary {
                is_current: Some(r.get::<Uuid, _>("id")) == current,
                version: r.get("version"),
                created_at_unix: r.get("created_unix"),
                published_by: r.get("published_by"),
            })
            .collect(),
    ))
}

pub async fn get_version(
    pool: &PgPool,
    t: TenantId,
    slug: &str,
    version: i32,
) -> Result<Option<Value>, sqlx::Error> {
    let mut tx = tx(pool, t).await?;
    let c: Option<Json<Value>> = sqlx::query_scalar(
        "SELECT v.content_json FROM website_page_versions v
         JOIN website_pages p ON p.tenant_id = v.tenant_id AND p.id = v.page_id
         WHERE v.tenant_id = $1 AND p.slug = $2 AND v.version = $3",
    )
    .bind(t.as_uuid())
    .bind(slug)
    .bind(version)
    .fetch_optional(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(c.map(|j| j.0))
}

/// Rollback ke versi `version` halaman yang sama (AC-WEB-014).
pub async fn rollback(
    pool: &PgPool,
    actor: &Actor,
    slug: &str,
    version: i32,
    ctx: Ctx<'_>,
) -> Result<(), WebsiteError> {
    let mut tx = tx(pool, actor.tenant_id).await?;
    let tid = actor.tenant_id.as_uuid();
    let row = sqlx::query(
        "SELECT p.id AS page, v.id AS ver, cur.version AS from_version
         FROM website_pages p
         JOIN website_page_versions v ON v.tenant_id = p.tenant_id AND v.page_id = p.id AND v.version = $3
         LEFT JOIN website_page_versions cur ON cur.id = p.published_version_id
         WHERE p.tenant_id = $1 AND p.slug = $2 FOR UPDATE OF p",
    )
    .bind(tid)
    .bind(slug)
    .bind(version)
    .fetch_optional(&mut *tx)
    .await?
    .ok_or(WebsiteError::NotFound)?;
    let (page, ver): (Uuid, Uuid) = (row.get("page"), row.get("ver"));
    let from: Option<i32> = row.get("from_version");
    sqlx::query(
        "UPDATE website_pages SET published_version_id = $3 WHERE tenant_id = $1 AND id = $2",
    )
    .bind(tid)
    .bind(page)
    .bind(ver)
    .execute(&mut *tx)
    .await?;
    audit(
        &mut tx,
        actor,
        actions::ROLLED_BACK,
        page,
        serde_json::json!({ "slug": slug, "from_version": from, "to_version": version }),
        ctx,
    )
    .await?;
    tx.commit().await?;
    Ok(())
}

/// Simpan draf tema situs (sudah divalidasi pemanggil). Diaudit; tidak memengaruhi situs sampai terbit.
pub async fn save_theme(
    pool: &PgPool,
    actor: &Actor,
    theme: &Value,
    ctx: Ctx<'_>,
) -> Result<(), WebsiteError> {
    let mut tx = tx(pool, actor.tenant_id).await?;
    let id: Uuid = sqlx::query_scalar(
        "UPDATE tenant_websites SET theme_draft = $2 WHERE tenant_id = $1 RETURNING id",
    )
    .bind(actor.tenant_id.as_uuid())
    .bind(Json(theme))
    .fetch_optional(&mut *tx)
    .await?
    .ok_or(WebsiteError::NotFound)?;
    audit(
        &mut tx,
        actor,
        actions::THEME_SAVED,
        id,
        serde_json::json!({}),
        ctx,
    )
    .await?;
    tx.commit().await?;
    Ok(())
}

/// Tema situs TERBIT (untuk situs publik).
pub async fn published_theme(pool: &PgPool, t: TenantId) -> Result<Option<Value>, sqlx::Error> {
    let mut tx = tx(pool, t).await?;
    let v: Option<Option<Json<Value>>> =
        sqlx::query_scalar("SELECT theme_published FROM tenant_websites WHERE tenant_id = $1")
            .bind(t.as_uuid())
            .fetch_optional(&mut *tx)
            .await?;
    tx.commit().await?;
    Ok(v.flatten().map(|Json(v)| v))
}

/// Konten TERBIT untuk situs publik (ADR-0014 #3). Draft tidak pernah dibaca di sini.
pub async fn published_content(
    pool: &PgPool,
    t: TenantId,
    slug: &str,
) -> Result<Option<(i32, Value)>, sqlx::Error> {
    let mut tx = tx(pool, t).await?;
    let row = sqlx::query(
        "SELECT v.version, v.content_json FROM website_pages p
         JOIN website_page_versions v ON v.id = p.published_version_id
         WHERE p.tenant_id = $1 AND v.tenant_id = $1 AND p.slug = $2",
    )
    .bind(t.as_uuid())
    .bind(slug)
    .fetch_optional(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(row.map(|r| {
        let Json(c): Json<Value> = r.get("content_json");
        (r.get("version"), c)
    }))
}

/// Ringkasan halaman TERBIT untuk navigasi situs publik (judul dipakai sebagai label tautan).
/// Hanya halaman yang sudah punya versi terbit — halaman draf tidak boleh membocorkan judulnya.
/// Satu halaman terbit: slug, judul, metadata artikel (bila halaman artikel), dan waktu terbit.
#[derive(Debug, Clone)]
pub struct PublishedPage {
    pub slug: String,
    pub title: String,
    pub article: Option<Value>,
    /// Tanggal versi terbit (`YYYY-MM-DD`, UTC) — untuk `lastmod` sitemap.
    pub published_at: String,
}

pub async fn published_pages(
    pool: &PgPool,
    t: TenantId,
) -> Result<Vec<PublishedPage>, sqlx::Error> {
    let mut tx = tx(pool, t).await?;
    let rows = sqlx::query(
        "SELECT p.slug, p.title, v.content_json->'article' AS article,
                to_char(v.created_at AT TIME ZONE 'UTC', 'YYYY-MM-DD') AS published_at
         FROM website_pages p JOIN website_page_versions v ON v.id = p.published_version_id
         WHERE p.tenant_id = $1 AND v.tenant_id = $1
           AND p.slug NOT IN ('keranjang', 'pesanan', 'akun')
         ORDER BY (p.slug = 'home') DESC, p.nav_position NULLS LAST, p.slug",
    )
    .bind(t.as_uuid())
    .fetch_all(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(rows
        .iter()
        .map(|r| PublishedPage {
            slug: r.get("slug"),
            title: r.get("title"),
            article: r.get::<Option<Json<Value>>, _>("article").map(|Json(v)| v),
            published_at: r.get("published_at"),
        })
        .collect())
}
