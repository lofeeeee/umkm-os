//! P2-1: skema website diuji sebagai ROLE APLIKASI (RLS aktif).
//! AC-WEB-003 (template berversi), AC-WEB-004/005 (seed lolos validator), AC-WEB-013 (versi immutable).

use sqlx::{PgPool, postgres::PgPoolOptions, types::Json};
use umkm_database::{MIGRATOR, begin_tenant_tx};
use umkm_domain::website::validate_page;
use umkm_shared::ids::TenantId;
use uuid::Uuid;

async fn app_pool(pool: &PgPool) -> PgPool {
    PgPoolOptions::new()
        .max_connections(3)
        .after_connect(|c, _| {
            Box::pin(async move {
                sqlx::query("SET ROLE umkm_app_role")
                    .execute(c)
                    .await
                    .map(|_| ())
            })
        })
        .connect_with(pool.connect_options().as_ref().clone())
        .await
        .unwrap()
}

async fn tenant(pool: &PgPool, slug: &str) -> TenantId {
    let id: Uuid =
        sqlx::query_scalar("INSERT INTO tenants (name, slug) VALUES ($1, $1) RETURNING id")
            .bind(slug)
            .fetch_one(pool)
            .await
            .unwrap();
    TenantId::from_uuid(id)
}

/// Website + halaman `home` + satu versi terbit, sebagai role aplikasi. Return (page, version).
async fn site(app: &PgPool, t: TenantId) -> (Uuid, Uuid) {
    let mut tx = begin_tenant_tx(app, t).await.unwrap();
    let tpl: Uuid = sqlx::query_scalar(
        "SELECT id FROM website_templates WHERE key = 'modern_store' AND version = 1",
    )
    .fetch_one(&mut *tx)
    .await
    .unwrap();
    let web: Uuid = sqlx::query_scalar(
        "INSERT INTO tenant_websites (tenant_id, template_id) VALUES ($1, $2) RETURNING id",
    )
    .bind(t.as_uuid())
    .bind(tpl)
    .fetch_one(&mut *tx)
    .await
    .unwrap();
    let page: Uuid = sqlx::query_scalar(
        "INSERT INTO website_pages (tenant_id, website_id, slug, title, draft_content)
         VALUES ($1, $2, 'home', 'Beranda', '{}') RETURNING id",
    )
    .bind(t.as_uuid())
    .bind(web)
    .fetch_one(&mut *tx)
    .await
    .unwrap();
    let ver: Uuid = sqlx::query_scalar(
        "INSERT INTO website_page_versions (tenant_id, page_id, version, content_json)
         VALUES ($1, $2, 1, '{}') RETURNING id",
    )
    .bind(t.as_uuid())
    .bind(page)
    .fetch_one(&mut *tx)
    .await
    .unwrap();
    sqlx::query("UPDATE website_pages SET published_version_id = $1 WHERE id = $2")
        .bind(ver)
        .bind(page)
        .execute(&mut *tx)
        .await
        .unwrap();
    tx.commit().await.unwrap();
    (page, ver)
}

fn sqlstate(e: &sqlx::Error) -> String {
    e.as_database_error()
        .and_then(|d| d.code())
        .map(|c| c.into_owned())
        .unwrap_or_default()
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn seeded_templates_pass_the_rust_validator(pool: PgPool) {
    let app = app_pool(&pool).await;
    let rows: Vec<(String, i32, Json<serde_json::Value>)> = sqlx::query_as(
        "SELECT key, version, default_content FROM website_templates WHERE status = 'active'",
    )
    .fetch_all(&app)
    .await
    .unwrap();
    assert!(rows.len() >= 2);
    for (key, _, Json(content)) in rows {
        let ok = validate_page(&content).unwrap_or_else(|e| panic!("{key}: {e:?}"));
        assert_eq!(ok.template, key);
    }
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn templates_are_read_only_for_the_app_role(pool: PgPool) {
    let app = app_pool(&pool).await;
    for sql in [
        "UPDATE website_templates SET name = 'x'",
        "DELETE FROM website_templates",
        "INSERT INTO website_templates (key, version, name, category, default_content) VALUES ('x', 1, 'x', 'x', '{}')",
    ] {
        let e = sqlx::query(sql).execute(&app).await.unwrap_err();
        assert_eq!(sqlstate(&e), "42501", "{sql}");
    }
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn new_template_version_does_not_move_existing_websites(pool: PgPool) {
    // AC-WEB-003
    let app = app_pool(&pool).await;
    let a = tenant(&pool, "toko-a").await;
    site(&app, a).await;
    sqlx::query(
        "INSERT INTO website_templates (key, version, name, category, default_content)
         SELECT key, 2, name, category, default_content FROM website_templates WHERE key = 'modern_store'",
    )
    .execute(&pool)
    .await
    .unwrap();
    let mut tx = begin_tenant_tx(&app, a).await.unwrap();
    let v: i32 = sqlx::query_scalar(
        "SELECT t.version FROM tenant_websites w JOIN website_templates t ON t.id = w.template_id",
    )
    .fetch_one(&mut *tx)
    .await
    .unwrap();
    assert_eq!(v, 1);
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn published_versions_are_immutable_for_the_app_role(pool: PgPool) {
    // AC-WEB-013
    let app = app_pool(&pool).await;
    let a = tenant(&pool, "toko-a").await;
    let (_, ver) = site(&app, a).await;
    for sql in [
        "UPDATE website_page_versions SET content_json = '{\"x\":1}' WHERE id = $1",
        "DELETE FROM website_page_versions WHERE id = $1",
    ] {
        let mut tx = begin_tenant_tx(&app, a).await.unwrap();
        let e = sqlx::query(sqlx::AssertSqlSafe(sql))
            .bind(ver)
            .execute(&mut *tx)
            .await
            .unwrap_err();
        assert_eq!(sqlstate(&e), "42501", "{sql}");
    }
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn pointer_cannot_target_another_page_or_tenant(pool: PgPool) {
    let app = app_pool(&pool).await;
    let a = tenant(&pool, "toko-a").await;
    let b = tenant(&pool, "toko-b").await;
    let (page_a, _) = site(&app, a).await;
    let (_, ver_b) = site(&app, b).await;
    // Sebagai owner (tanpa RLS) agar yang menolak adalah composite FK, bukan RLS.
    let e = sqlx::query("UPDATE website_pages SET published_version_id = $1 WHERE id = $2")
        .bind(ver_b)
        .bind(page_a)
        .execute(&pool)
        .await
        .unwrap_err();
    assert_eq!(sqlstate(&e), "23503");
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn tenant_b_sees_no_website_rows_of_tenant_a(pool: PgPool) {
    let app = app_pool(&pool).await;
    let a = tenant(&pool, "toko-a").await;
    let b = tenant(&pool, "toko-b").await;
    let (page_a, _) = site(&app, a).await;
    for table in ["tenant_websites", "website_pages", "website_page_versions"] {
        let mut tx = begin_tenant_tx(&app, b).await.unwrap();
        let n: i64 =
            sqlx::query_scalar(sqlx::AssertSqlSafe(format!("SELECT count(*) FROM {table}")))
                .fetch_one(&mut *tx)
                .await
                .unwrap();
        assert_eq!(n, 0, "{table}");
    }
    // Tulis lintas tenant: 0 baris berubah, dan INSERT dengan tenant_id A ditolak WITH CHECK.
    let mut tx = begin_tenant_tx(&app, b).await.unwrap();
    let r = sqlx::query("UPDATE website_pages SET title = 'x' WHERE id = $1")
        .bind(page_a)
        .execute(&mut *tx)
        .await
        .unwrap();
    assert_eq!(r.rows_affected(), 0);
    let e = sqlx::query(
        "INSERT INTO website_page_versions (tenant_id, page_id, version, content_json) VALUES ($1, $2, 9, '{}')",
    )
    .bind(a.as_uuid())
    .bind(page_a)
    .execute(&mut *tx)
    .await
    .unwrap_err();
    assert_eq!(sqlstate(&e), "42501");
}
