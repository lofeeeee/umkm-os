//! P9-5 (ADR-0018): `<head>` situs toko per Host, robots.txt, sitemap.xml. Role aplikasi + RLS.

mod common;

use axum::http::Method;
use common::*;
use serde_json::{Value, json};
use sqlx::PgPool;
use umkm_api::{build_router, site_head::SiteHead};
use umkm_database::MIGRATOR;
use uuid::Uuid;

const SHELL: &str = "<!DOCTYPE html><html lang=\"id\"><head><meta charset=\"utf-8\"><title>UMKM OS</title><meta name=\"description\" content=\"shell\"/><script src=\"/_next/static/a.js\"></script></head><body><div id=\"root\"></div></body></html>";

fn studio_doc() -> Value {
    let raw = include_str!("../../../crates/domain/tests/fixtures/studio_browser_doc.json");
    let mut d: Value = serde_json::from_str(raw).unwrap();
    d["template"] = json!("modern_store");
    d["seo"] = json!({"title": "Kopi Senja </title><script>x</script>", "description": "Kopi & roti \"enak\""});
    d
}

async fn get(a: &axum::Router, uri: &str, host: &str) -> Res {
    send(a, Method::GET, uri, None, Some(host), None, vec![]).await
}

async fn publish_doc(a: &TestApp, t: Uuid, o: &Who, doc: Value) {
    let w = format!("/api/v1/tenants/{t}/website");
    let r = json(
        &a.router,
        Method::PUT,
        &w,
        Some(o),
        Some(json!({"template_key": "modern_store"})),
    )
    .await;
    assert!(r.status < 300, "{}", r.json());
    let d = json(
        &a.router,
        Method::GET,
        &format!("{w}/pages/home/draft"),
        Some(o),
        None,
    )
    .await;
    let rev = d.json()["data"]["revision"].as_i64().unwrap();
    let r = json(
        &a.router,
        Method::PUT,
        &format!("{w}/pages/home/draft"),
        Some(o),
        Some(json!({"content": doc, "base_revision": rev})),
    )
    .await;
    assert_eq!(r.status, 200, "{}", r.json());
    let r = json(
        &a.router,
        Method::POST,
        &format!("{w}/pages/home/publish"),
        Some(o),
        Some(json!({})),
    )
    .await;
    assert_eq!(r.status, 200, "{}", r.json());
    let r = json(
        &a.router,
        Method::PUT,
        &format!("/api/v1/tenants/{t}/business/profile"),
        Some(o),
        Some(json!({"name": "Kopi <Senja>", "description": "Kedai kopi", "whatsapp": "081234567890", "timezone": "Asia/Jakarta"})),
    )
    .await;
    assert!(r.status < 300, "{}", r.json());
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn document_injects_escaped_head_per_host_and_never_leaks(pool: PgPool) {
    let a = app(&pool).await;
    let router = build_router(
        a.state.clone().with_site(SiteHead {
            shell: Some(SHELL.into()),
            api_origin: "https://api.ryasai.my.id".into(),
            scheme: "https".into(),
        }),
        LIMITS,
    );
    let t = tenant(&pool, "kopi").await;
    let o = member(&pool, t, "Owner").await;
    let t2 = tenant(&pool, "lain").await;
    let o2 = member(&pool, t2, "Owner").await;
    publish_doc(&a, t, &o, studio_doc()).await;

    let r = get(
        &router,
        "/api/v1/public/document?path=/",
        "kopi.ryasai.my.id",
    )
    .await;
    assert_eq!(r.status, 200);
    let html = String::from_utf8(r.bytes.clone()).unwrap();
    assert_eq!(r.headers["content-type"], "text/html; charset=utf-8");
    assert_eq!(r.headers["cache-control"], "public, max-age=60");
    // Judul SEO dari versi terbit, ter-escape; judul shell diganti; skrip shell utuh.
    assert!(
        html.contains("<title>Kopi Senja &lt;/title&gt;&lt;script&gt;x&lt;/script&gt;</title>"),
        "{html}"
    );
    assert_eq!(html.matches("<title>").count(), 1);
    assert!(html.contains("content=\"Kopi &amp; roti &quot;enak&quot;\""));
    assert!(html.contains("<link rel=\"canonical\" href=\"https://kopi.ryasai.my.id/\">"));
    assert!(html.contains("og:site_name\" content=\"Kopi &lt;Senja&gt;\""));
    assert!(html.contains("application/ld+json") && html.contains("LocalBusiness"));
    assert!(html.contains("<script src=\"/_next/static/a.js\"></script>"));
    assert!(!html.contains("<script>x"));

    // Path berbahaya → kanonik "/".
    let r = get(
        &router,
        "/api/v1/public/document?path=/%22%3E%3Cscript%3E",
        "kopi.ryasai.my.id",
    )
    .await;
    assert!(
        String::from_utf8(r.bytes)
            .unwrap()
            .contains("href=\"https://kopi.ryasai.my.id/\">")
    );

    // Tenant lain tanpa versi terbit: tidak membocorkan meta toko pertama.
    let _ = o2;
    let r = get(
        &router,
        "/api/v1/public/document?path=/",
        "lain.ryasai.my.id",
    )
    .await;
    let other = String::from_utf8(r.bytes).unwrap();
    assert!(!other.contains("Kopi"), "{other}");

    // Host tak dikenal → 404 (Caddy jatuh balik ke shell statis).
    assert_eq!(
        get(
            &router,
            "/api/v1/public/document?path=/",
            "tidakada.ryasai.my.id"
        )
        .await
        .status,
        404
    );

    // Fitur nonaktif (tanpa shell) → 404.
    assert_eq!(
        get(
            &a.router,
            "/api/v1/public/document?path=/",
            "kopi.ryasai.my.id"
        )
        .await
        .status,
        404
    );

    // robots & sitemap per Host.
    let r = get(&router, "/api/v1/public/robots.txt", "kopi.ryasai.my.id").await;
    assert_eq!(r.status, 200);
    assert!(
        String::from_utf8(r.bytes)
            .unwrap()
            .contains("Sitemap: https://kopi.ryasai.my.id/sitemap.xml")
    );
    let r = get(&router, "/api/v1/public/sitemap.xml", "kopi.ryasai.my.id").await;
    assert_eq!(r.status, 200);
    let xml = String::from_utf8(r.bytes).unwrap();
    assert!(
        xml.contains("<loc>https://kopi.ryasai.my.id/</loc>"),
        "{xml}"
    );
}
