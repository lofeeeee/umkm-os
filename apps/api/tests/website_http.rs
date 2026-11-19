//! P2-2/P2-3/P2-4/P2-7 (API): template, draft, preview, publish, rollback, situs publik.
//! Lima skenario Website `PRD.md §13.3` + isolasi tenant. Role aplikasi + RLS.

mod common;

use axum::http::Method;
use common::*;
use serde_json::{Value, json};
use sqlx::PgPool;
use umkm_database::MIGRATOR;
use uuid::Uuid;

fn w(t: Uuid) -> String {
    format!("/api/v1/tenants/{t}/website")
}

async fn select(a: &TestApp, t: Uuid, who: &Who, key: &str) -> Res {
    json(
        &a.router,
        Method::PUT,
        &w(t),
        Some(who),
        Some(json!({"template_key": key})),
    )
    .await
}

async fn draft(a: &TestApp, t: Uuid, who: &Who) -> (Value, i64) {
    let r = json(
        &a.router,
        Method::GET,
        &format!("{}/pages/home/draft", w(t)),
        Some(who),
        None,
    )
    .await;
    assert_eq!(r.status, 200, "{}", r.json());
    let d = r.json()["data"].clone();
    (d["content"].clone(), d["revision"].as_i64().unwrap())
}

async fn save(a: &TestApp, t: Uuid, who: &Who, content: Value, rev: i64) -> Res {
    json(
        &a.router,
        Method::PUT,
        &format!("{}/pages/home/draft", w(t)),
        Some(who),
        Some(json!({"content": content, "base_revision": rev})),
    )
    .await
}

async fn publish(a: &TestApp, t: Uuid, who: &Who) -> Res {
    json(
        &a.router,
        Method::POST,
        &format!("{}/pages/home/publish", w(t)),
        Some(who),
        Some(json!({})),
    )
    .await
}

async fn public(a: &TestApp, host: &str) -> Res {
    send(
        &a.router,
        Method::GET,
        "/api/v1/public/site",
        None,
        Some(host),
        None,
        vec![],
    )
    .await
}

fn hero_title(content: &Value) -> String {
    content["sections"]
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["type"] == "hero")
        .unwrap()["data"]["title"]
        .as_str()
        .unwrap()
        .to_owned()
}

fn set_hero_title(mut c: Value, title: &str) -> Value {
    for s in c["sections"].as_array_mut().unwrap() {
        if s["type"] == "hero" {
            s["data"]["title"] = json!(title);
        }
    }
    c
}

fn paths(r: &Res) -> Vec<String> {
    r.json()["error"]["details"]["errors"]
        .as_array()
        .map(|a| {
            a.iter()
                .map(|e| {
                    format!(
                        "{}:{}",
                        e["path"].as_str().unwrap(),
                        e["code"].as_str().unwrap()
                    )
                })
                .collect()
        })
        .unwrap_or_default()
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn templates_are_listed_and_previewed_without_side_effects(pool: PgPool) {
    // AC-WEB-001, AC-WEB-002
    let a = app(&pool).await;
    let t = tenant(&pool, "toko-a").await;
    let viewer = member(&pool, t, "Viewer").await;
    let r = json(
        &a.router,
        Method::GET,
        &format!("{}/templates", w(t)),
        Some(&viewer),
        None,
    )
    .await;
    assert_eq!(r.status, 200);
    let keys: Vec<_> = r.json()["data"]
        .as_array()
        .unwrap()
        .iter()
        .map(|x| x["key"].as_str().unwrap().to_owned())
        .collect();
    assert_eq!(keys, vec!["modern_store", "warung_hangat"]);
    let r = json(
        &a.router,
        Method::GET,
        &format!("{}/templates/warung_hangat/preview", w(t)),
        Some(&viewer),
        None,
    )
    .await;
    assert_eq!(r.status, 200);
    assert_eq!(r.json()["data"]["content"]["template"], "warung_hangat");
    let r = json(
        &a.router,
        Method::GET,
        &format!("{}/templates/tidak_ada/preview", w(t)),
        Some(&viewer),
        None,
    )
    .await;
    assert_eq!(r.status, 404);
    let n: i64 = sqlx::query_scalar("SELECT count(*) FROM tenant_websites")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(n, 0, "preview tidak membuat website");
    assert_eq!(
        json(&a.router, Method::GET, &w(t), Some(&viewer), None)
            .await
            .status,
        404
    );
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn select_template_creates_home_draft_and_rbac_applies(pool: PgPool) {
    // PRD §13.3 "Template dapat dipilih"
    let a = app(&pool).await;
    let t = tenant(&pool, "toko-a").await;
    let owner = member(&pool, t, "Owner").await;
    let viewer = member(&pool, t, "Viewer").await;
    assert_eq!(select(&a, t, &viewer, "modern_store").await.status, 403);
    let r = select(&a, t, &owner, "nope").await;
    assert_eq!(paths(&r), vec!["template_key:unknown_template"]);
    let r = select(&a, t, &owner, "modern_store").await;
    assert_eq!(r.status, 200, "{}", r.json());
    assert_eq!(r.json()["data"]["template_key"], "modern_store");
    assert_eq!(r.json()["data"]["pages"][0]["slug"], "home");
    assert_eq!(
        r.json()["data"]["pages"][0]["published_version"],
        Value::Null
    );
    let (c, _) = draft(&a, t, &owner).await;
    assert_eq!(c["template"], "modern_store");
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn draft_edits_are_validated_and_invalid_saves_change_nothing(pool: PgPool) {
    // AC-WEB-004, AC-WEB-005, AC-WEB-007, AC-WEB-009, AC-WEB-010
    let a = app(&pool).await;
    let t = tenant(&pool, "toko-a").await;
    let o = member(&pool, t, "Owner").await;
    select(&a, t, &o, "modern_store").await;
    let (c, rev) = draft(&a, t, &o).await;

    // reorder: balik urutan section
    let mut reordered = c.clone();
    reordered["sections"].as_array_mut().unwrap().reverse();
    let r = save(&a, t, &o, reordered.clone(), rev).await;
    assert_eq!(r.status, 200, "{}", r.json());
    let rev2 = r.json()["data"]["revision"].as_i64().unwrap();
    assert_eq!(rev2, rev + 1);
    let (c2, _) = draft(&a, t, &o).await;
    assert_eq!(c2["sections"][0]["type"], "footer", "urutan tersimpan");

    // invalid: field HTML, URL javascript, template lain, theme salah
    let mut bad = c2.clone();
    bad["sections"][0]["data"]["html"] = json!("<script>");
    bad["theme"]["primary"] = json!("red");
    let r = save(&a, t, &o, bad, rev2).await;
    assert_eq!(r.status, 422);
    let got = paths(&r);
    assert!(
        got.contains(&"sections[0].data.html:unknown_field".to_owned()),
        "{got:?}"
    );
    assert!(
        got.contains(&"theme.primary:invalid_color".to_owned()),
        "{got:?}"
    );
    let mut other = c2.clone();
    other["template"] = json!("warung_hangat");
    assert_eq!(
        paths(&save(&a, t, &o, other, rev2).await),
        vec!["template:template_mismatch"]
    );
    let (c3, rev3) = draft(&a, t, &o).await;
    assert_eq!((c3, rev3), (c2.clone(), rev2), "tidak ada yang tersimpan");

    // stale revision -> 409
    let r = save(&a, t, &o, c2.clone(), rev).await;
    assert_eq!(r.status, 409);
    assert_eq!(r.json()["error"]["details"]["current_revision"], rev2);
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn assets_must_belong_to_the_tenant(pool: PgPool) {
    // AC-WEB-009
    let a = app(&pool).await;
    let ta = tenant(&pool, "toko-a").await;
    let tb = tenant(&pool, "toko-b").await;
    let oa = member(&pool, ta, "Owner").await;
    let ob = member(&pool, tb, "Owner").await;
    select(&a, ta, &oa, "modern_store").await;
    let mine = upload(&a.router, ta, &oa, png()).await.json()["data"]["ref"]
        .as_str()
        .unwrap()
        .to_owned();
    let theirs = upload(&a.router, tb, &ob, png()).await.json()["data"]["ref"]
        .as_str()
        .unwrap()
        .to_owned();
    let (c, rev) = draft(&a, ta, &oa).await;
    let with_image = |img: &str| {
        let mut x = c.clone();
        for s in x["sections"].as_array_mut().unwrap() {
            if s["type"] == "hero" {
                s["data"]["image"] = json!(img);
            }
        }
        x
    };
    let r = save(&a, ta, &oa, with_image(&theirs), rev).await;
    assert_eq!(paths(&r), vec![format!("{theirs}:asset_not_found")]);
    let r = save(
        &a,
        ta,
        &oa,
        with_image(&format!("asset:{}", Uuid::new_v4())),
        rev,
    )
    .await;
    assert_eq!(r.status, 422);
    let r = save(&a, ta, &oa, with_image(&mine), rev).await;
    assert_eq!(r.status, 200);
    // Draft memberi signed URL untuk preview
    let d = json(
        &a.router,
        Method::GET,
        &format!("{}/pages/home/draft", w(ta)),
        Some(&oa),
        None,
    )
    .await;
    assert!(
        d.json()["data"]["assets"][&mine]
            .as_str()
            .unwrap()
            .starts_with("/files/")
    );
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn preview_publish_rollback_and_public_site(pool: PgPool) {
    // AC-WEB-008, 011, 012, 014, 015 — 5 skenario Website PRD §13.3
    let a = app(&pool).await;
    let t = tenant(&pool, "toko-a").await;
    let o = member(&pool, t, "Owner").await;
    let host = "toko-a.ryasai.my.id";
    select(&a, t, &o, "modern_store").await;
    assert_eq!(public(&a, host).await.status, 404, "belum terbit -> 404");

    // v1
    let (c, rev) = draft(&a, t, &o).await;
    let v1 = set_hero_title(c, "Versi Satu");
    let rev = save(&a, t, &o, v1, rev).await.json()["data"]["revision"]
        .as_i64()
        .unwrap();
    let r = publish(&a, t, &o).await;
    assert_eq!(r.status, 200, "{}", r.json());
    assert_eq!(r.json()["data"]["version"], 1);
    let p = public(&a, host).await;
    assert_eq!(p.status, 200, "{}", p.json());
    assert_eq!(
        hero_title(&p.json()["data"]["page"]["content"]),
        "Versi Satu"
    );
    assert_eq!(p.headers["cache-control"], "public, max-age=60");
    // AC-WEB-008: section enabled=false (promo-1 di template) tidak dikirim publik
    let types: Vec<_> = p.json()["data"]["page"]["content"]["sections"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| s["id"].as_str().unwrap().to_owned())
        .collect();
    assert!(!types.contains(&"promo-1".to_owned()), "{types:?}");
    assert!(p.json()["data"]["business"]["profile"]["name"].is_string());
    assert!(p.json()["data"]["status"]["status"].is_string());

    // AC-WEB-011: edit draft -> preview menampilkan draft, publik tetap v1
    let (c, _) = draft(&a, t, &o).await;
    let v2 = set_hero_title(c, "Versi Dua <script>alert(1)</script>");
    save(&a, t, &o, v2, rev).await;
    let (d, _) = draft(&a, t, &o).await;
    assert_eq!(hero_title(&d), "Versi Dua <script>alert(1)</script>");
    assert_eq!(
        hero_title(&public(&a, host).await.json()["data"]["page"]["content"]),
        "Versi Satu"
    );

    // AC-WEB-012: Manager tidak boleh publish; Owner -> v2; audit
    let m = member(&pool, t, "Manager").await;
    assert_eq!(publish(&a, t, &m).await.status, 403);
    assert_eq!(publish(&a, t, &o).await.json()["data"]["version"], 2);
    assert_eq!(
        hero_title(&public(&a, host).await.json()["data"]["page"]["content"]),
        "Versi Dua <script>alert(1)</script>"
    );
    let n: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM audit_logs WHERE tenant_id = $1 AND action = 'website.publish'",
    )
    .bind(t)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(n, 2);

    // AC-WEB-014: rollback ke v1
    let r = json(
        &a.router,
        Method::POST,
        &format!("{}/pages/home/rollback", w(t)),
        Some(&o),
        Some(json!({"version": 1})),
    )
    .await;
    assert_eq!(r.status, 200, "{}", r.json());
    assert_eq!(
        hero_title(&public(&a, host).await.json()["data"]["page"]["content"]),
        "Versi Satu"
    );
    let vs = json(
        &a.router,
        Method::GET,
        &format!("{}/pages/home/versions", w(t)),
        Some(&o),
        None,
    )
    .await;
    let vs = vs.json()["data"].clone();
    assert_eq!(vs.as_array().unwrap().len(), 2, "versi 2 tetap ada");
    assert_eq!(vs[1]["version"], 1);
    assert_eq!(vs[1]["is_current"], true);
    let r = json(
        &a.router,
        Method::POST,
        &format!("{}/pages/home/rollback", w(t)),
        Some(&o),
        Some(json!({"version": 9})),
    )
    .await;
    assert_eq!(r.status, 404);
    assert_eq!(
        json(
            &a.router,
            Method::POST,
            &format!("{}/pages/home/rollback", w(t)),
            Some(&m),
            Some(json!({"version": 2}))
        )
        .await
        .status,
        403
    );
    let n: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM audit_logs WHERE tenant_id = $1 AND action = 'website.rollback'",
    )
    .bind(t)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(n, 1);
    // Versi lama dapat dipreview
    let r = json(
        &a.router,
        Method::GET,
        &format!("{}/pages/home/versions/2", w(t)),
        Some(&o),
        None,
    )
    .await;
    assert_eq!(
        hero_title(&r.json()["data"]["content"]),
        "Versi Dua <script>alert(1)</script>"
    );
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn publish_with_stale_revision_is_rejected(pool: PgPool) {
    let a = app(&pool).await;
    let t = tenant(&pool, "toko-a").await;
    let o = member(&pool, t, "Owner").await;
    select(&a, t, &o, "modern_store").await;
    let (_, rev) = draft(&a, t, &o).await;
    let r = json(
        &a.router,
        Method::POST,
        &format!("{}/pages/home/publish", w(t)),
        Some(&o),
        Some(json!({"revision": rev - 1})),
    )
    .await;
    assert_eq!(r.status, 409);
    let r = json(
        &a.router,
        Method::POST,
        &format!("{}/pages/home/publish", w(t)),
        Some(&o),
        Some(json!({"revision": rev})),
    )
    .await;
    assert_eq!(r.status, 200);
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn invalid_draft_already_in_db_cannot_be_published(pool: PgPool) {
    // Draft yang ditulis di luar API (mis. data lama) tetap divalidasi saat publish.
    let a = app(&pool).await;
    let t = tenant(&pool, "toko-a").await;
    let o = member(&pool, t, "Owner").await;
    select(&a, t, &o, "modern_store").await;
    sqlx::query("UPDATE website_pages SET draft_content = '{\"schema_version\":1,\"template\":\"modern_store\",\"theme\":{\"primary\":\"#000000\",\"font\":\"inter\"},\"sections\":[{\"id\":\"x\",\"type\":\"raw_html\",\"enabled\":true,\"data\":{}}]}' WHERE tenant_id = $1")
        .bind(t).execute(&pool).await.unwrap();
    let r = publish(&a, t, &o).await;
    assert_eq!(paths(&r), vec!["sections[0].type:unknown_section_type"]);
    let n: i64 = sqlx::query_scalar("SELECT count(*) FROM website_page_versions")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(n, 0);
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn public_site_resolves_only_active_hosts_and_never_leaks_other_tenants(pool: PgPool) {
    // AC-WEB-015, AC-WEB-016 + isolasi
    let a = app(&pool).await;
    let ta = tenant(&pool, "toko-a").await;
    let tb = tenant(&pool, "toko-b").await;
    let oa = member(&pool, ta, "Owner").await;
    let ob = member(&pool, tb, "Owner").await;
    for (t, o, title) in [(ta, &oa, "Milik A"), (tb, &ob, "Milik B")] {
        select(&a, t, o, "modern_store").await;
        let (c, rev) = draft(&a, t, o).await;
        save(&a, t, o, set_hero_title(c, title), rev).await;
        assert_eq!(publish(&a, t, o).await.status, 200);
    }
    assert_eq!(
        hero_title(&public(&a, "toko-a.ryasai.my.id").await.json()["data"]["page"]["content"]),
        "Milik A"
    );
    assert_eq!(
        hero_title(&public(&a, "toko-b.ryasai.my.id").await.json()["data"]["page"]["content"]),
        "Milik B"
    );
    assert_eq!(public(&a, "tidak-ada.ryasai.my.id").await.status, 404);
    assert_eq!(public(&a, "evil.example").await.status, 404);
    sqlx::query("UPDATE tenant_domains SET status = 'suspended' WHERE tenant_id = $1")
        .bind(tb)
        .execute(&pool)
        .await
        .unwrap();
    assert_eq!(
        public(&a, "toko-b.ryasai.my.id").await.status,
        404,
        "domain suspended (TEN-009)"
    );
    // Slug halaman tidak valid (path traversal) -> 404
    let r = send(
        &a.router,
        Method::GET,
        "/api/v1/public/site?page=../x",
        None,
        Some("toko-a.ryasai.my.id"),
        None,
        vec![],
    )
    .await;
    assert_eq!(r.status, 404);
    // Tenant B tidak dapat membaca/merollback/publish halaman A lewat path A
    for (m, p, b) in [
        (Method::GET, "pages/home/draft", None),
        (Method::GET, "pages/home/versions", None),
        (Method::POST, "pages/home/publish", Some(json!({}))),
        (
            Method::POST,
            "pages/home/rollback",
            Some(json!({"version": 1})),
        ),
    ] {
        assert_eq!(
            json(&a.router, m, &format!("{}/{p}", w(ta)), Some(&ob), b)
                .await
                .status,
            404,
            "{p}"
        );
    }
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn public_assets_are_signed_only_for_own_objects(pool: PgPool) {
    let a = app(&pool).await;
    let t = tenant(&pool, "toko-a").await;
    let o = member(&pool, t, "Owner").await;
    select(&a, t, &o, "modern_store").await;
    let img = upload(&a.router, t, &o, png()).await.json()["data"]["ref"]
        .as_str()
        .unwrap()
        .to_owned();
    let (mut c, rev) = draft(&a, t, &o).await;
    for s in c["sections"].as_array_mut().unwrap() {
        if s["type"] == "hero" {
            s["data"]["image"] = json!(img);
        }
    }
    assert_eq!(save(&a, t, &o, c, rev).await.status, 200);
    publish(&a, t, &o).await;
    let p = public(&a, "toko-a.ryasai.my.id").await;
    let url = p.json()["data"]["assets"][&img]
        .as_str()
        .unwrap()
        .to_owned();
    let f = send(&a.router, Method::GET, &url, None, None, None, vec![]).await;
    assert_eq!(f.status, 200);
    assert_eq!(f.bytes, png());
}

async fn create_page_req(a: &TestApp, t: Uuid, who: &Who, slug: &str, title: &str) -> Res {
    json(
        &a.router,
        Method::POST,
        &format!("{}/pages", w(t)),
        Some(who),
        Some(json!({"slug": slug, "title": title})),
    )
    .await
}

async fn delete_page_req(a: &TestApp, t: Uuid, who: &Who, slug: &str) -> Res {
    json(
        &a.router,
        Method::DELETE,
        &format!("{}/pages/{slug}", w(t)),
        Some(who),
        None,
    )
    .await
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn create_page_starter_vs_pro(pool: PgPool) {
    let a = app(&pool).await;
    let t = tenant(&pool, "toko-a").await;
    let owner = member(&pool, t, "Owner").await;
    let viewer = member(&pool, t, "Viewer").await;
    select(&a, t, &owner, "modern_store").await;

    // RBAC: viewer tidak punya website.edit -> 403
    let r = create_page_req(&a, t, &viewer, "about", "Tentang Kami").await;
    assert_eq!(r.status, 403);

    // Starter: "about" dan "contact" diizinkan (201)
    let r = create_page_req(&a, t, &owner, "about", "Tentang Kami").await;
    assert_eq!(r.status, 201, "{}", r.json());
    assert_eq!(r.json()["data"]["slug"], "about");
    assert_eq!(r.json()["data"]["title"], "Tentang Kami");
    assert_eq!(r.json()["data"]["draft_revision"], 1);
    assert!(r.json()["data"]["published_version"].is_null());

    let r = create_page_req(&a, t, &owner, "contact", "Kontak Kami").await;
    assert_eq!(r.status, 201, "{}", r.json());
    assert_eq!(r.json()["data"]["slug"], "contact");

    // Starter: nama/slug bebas diizinkan; yang dibatasi hanya jumlah (Beranda + 4 = 5 halaman).
    let r = create_page_req(&a, t, &owner, "promo", "Promo Spesial").await;
    assert_eq!(r.status, 201, "{}", r.json());
    assert_eq!(r.json()["data"]["title"], "Promo Spesial");
    let r = create_page_req(&a, t, &owner, "galeri", "Galeri").await;
    assert_eq!(r.status, 201, "{}", r.json());
    let r = create_page_req(&a, t, &owner, "lebih", "Satu lagi").await;
    assert_eq!(r.status, 403, "{}", r.json());
    assert_eq!(
        r.json()["error"]["message"],
        "Batas jumlah halaman paket ini tercapai"
    );
    assert_eq!(r.json()["error"]["details"]["max_pages"], 5);

    // Format slug tidak valid -> 404 (slug_ok)
    let r = create_page_req(&a, t, &owner, "-invalid-", "Invalid").await;
    assert_eq!(r.status, 404);

    // Pro: batas naik
    pro(&pool, t).await;
    let r = create_page_req(&a, t, &owner, "lebih", "Satu lagi").await;
    assert_eq!(r.status, 201, "{}", r.json());

    // Slug duplikat -> 409 Conflict
    let r = create_page_req(&a, t, &owner, "about", "Tentang Kami Lagi").await;
    assert_eq!(r.status, 409, "{}", r.json());
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn delete_page_rules(pool: PgPool) {
    let a = app(&pool).await;
    let t = tenant(&pool, "toko-a").await;
    let owner = member(&pool, t, "Owner").await;
    let viewer = member(&pool, t, "Viewer").await;
    select(&a, t, &owner, "modern_store").await;

    // Buat halaman "about"
    let r = create_page_req(&a, t, &owner, "about", "Tentang Kami").await;
    assert_eq!(r.status, 201);

    // RBAC: viewer tidak punya website.edit -> 403
    let r = delete_page_req(&a, t, &viewer, "about").await;
    assert_eq!(r.status, 403);

    // Tolak hapus "home" -> 400 Bad Request
    let r = delete_page_req(&a, t, &owner, "home").await;
    assert_eq!(r.status, 400, "{}", r.json());

    // Hapus "about" yang sah -> 200 OK
    let r = delete_page_req(&a, t, &owner, "about").await;
    assert_eq!(r.status, 200, "{}", r.json());

    // Pastikan "about" sudah terhapus (draft 404)
    let r = json(
        &a.router,
        Method::GET,
        &format!("{}/pages/about/draft", w(t)),
        Some(&owner),
        None,
    )
    .await;
    assert_eq!(r.status, 404);

    // Hapus halaman yang tidak ada -> 404 Not Found
    let r = delete_page_req(&a, t, &owner, "nonexistent").await;
    assert_eq!(r.status, 404);
}

async fn order_req(a: &TestApp, t: Uuid, who: &Who, slugs: Value) -> Res {
    json(
        &a.router,
        Method::PUT,
        &format!("{}/page-order", w(t)),
        Some(who),
        Some(json!({ "slugs": slugs })),
    )
    .await
}

async fn menu(a: &TestApp, t: Uuid, who: &Who) -> Vec<String> {
    let r = json(&a.router, Method::GET, &w(t), Some(who), None).await;
    r.json()["data"]["pages"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| p["slug"].as_str().unwrap().to_owned())
        .collect()
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn page_order_drives_the_menu_and_home_stays_first(pool: PgPool) {
    let a = app(&pool).await;
    let t = tenant(&pool, "toko-a").await;
    let owner = member(&pool, t, "Owner").await;
    let viewer = member(&pool, t, "Viewer").await;
    select(&a, t, &owner, "modern_store").await;
    for (s, n) in [
        ("about", "Tentang"),
        ("contact", "Kontak"),
        ("promo", "Promo"),
    ] {
        assert_eq!(create_page_req(&a, t, &owner, s, n).await.status, 201);
    }
    // Bawaan: beranda lalu abjad.
    assert_eq!(
        menu(&a, t, &owner).await,
        ["home", "about", "contact", "promo"]
    );
    // RBAC & validasi.
    assert_eq!(
        order_req(&a, t, &viewer, json!(["promo"])).await.status,
        403
    );
    assert_eq!(
        order_req(&a, t, &owner, json!(["promo", "promo"]))
            .await
            .status,
        422
    );
    assert_eq!(
        order_req(&a, t, &owner, json!(["Bad Slug"])).await.status,
        422
    );
    // Urutan pemilik dipakai; "home" di tengah daftar tetap pertama; slug tak dikenal diabaikan.
    let r = order_req(
        &a,
        t,
        &owner,
        json!(["promo", "home", "contact", "ghost", "about"]),
    )
    .await;
    assert_eq!(r.status, 200, "{}", r.json());
    assert_eq!(
        menu(&a, t, &owner).await,
        ["home", "promo", "contact", "about"]
    );
    // Halaman yang tidak disebut kembali ke belakang (urutan bawaan).
    assert_eq!(
        order_req(&a, t, &owner, json!(["contact"])).await.status,
        200
    );
    assert_eq!(
        menu(&a, t, &owner).await,
        ["home", "contact", "about", "promo"]
    );
}

async fn page_req(
    a: &TestApp,
    t: Uuid,
    who: &Who,
    m: Method,
    slug: &str,
    tail: &str,
    body: Option<Value>,
) -> Res {
    json(
        &a.router,
        m,
        &format!("{}/pages/{slug}{tail}", w(t)),
        Some(who),
        body,
    )
    .await
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn system_pages_are_studio_pages_with_one_locked_core_block(pool: PgPool) {
    let a = app(&pool).await;
    let t = tenant(&pool, "toko-a").await;
    let owner = member(&pool, t, "Owner").await;
    select(&a, t, &owner, "modern_store").await;

    // Rute toko lain tidak bisa dipakai halaman biasa.
    let r = create_page_req(&a, t, &owner, "produk", "Produk").await;
    assert_eq!(r.status, 422, "{}", r.json());

    // Starter penuh (Beranda + 4) — halaman sistem tetap bisa dibuat (tidak dihitung).
    for s in ["a1", "a2", "a3", "a4"] {
        assert_eq!(create_page_req(&a, t, &owner, s, s).await.status, 201);
    }
    let r = create_page_req(&a, t, &owner, "keranjang", "Keranjang").await;
    assert_eq!(r.status, 201, "{}", r.json());
    let r = page_req(&a, t, &owner, Method::GET, "keranjang", "/draft", None).await;
    let (mut doc, rev) = (
        r.json()["data"]["content"].clone(),
        r.json()["data"]["revision"].as_i64().unwrap(),
    );
    assert_eq!(doc["blocks"][0]["kind"], "shop_cart");

    // Blok bebas boleh ditambah; blok inti tidak boleh hilang, ganda, atau salah jenis.
    let text = json!({"id": "t1", "kind": "text", "x": 0, "y": 30, "w": 12, "h": 4, "style": "plain", "data": {"body": "Gratis ongkir!"}});
    doc["blocks"].as_array_mut().unwrap().push(text.clone());
    let mut no_core = doc.clone();
    no_core["blocks"] = json!([text]);
    let r = page_req(
        &a,
        t,
        &owner,
        Method::PUT,
        "keranjang",
        "/draft",
        Some(json!({"content": no_core, "base_revision": rev})),
    )
    .await;
    assert_eq!(r.status, 422, "{}", r.json());
    assert!(r.json().to_string().contains("core_block_required"));
    let mut wrong = doc.clone();
    wrong["blocks"][0]["kind"] = json!("shop_account");
    let r = page_req(
        &a,
        t,
        &owner,
        Method::PUT,
        "keranjang",
        "/draft",
        Some(json!({"content": wrong, "base_revision": rev})),
    )
    .await;
    assert_eq!(r.status, 422, "{}", r.json());
    let r = page_req(
        &a,
        t,
        &owner,
        Method::PUT,
        "keranjang",
        "/draft",
        Some(json!({"content": doc, "base_revision": rev})),
    )
    .await;
    assert_eq!(r.status, 200, "{}", r.json());

    // Blok inti tidak boleh dipasang di halaman biasa.
    let (mut home, hrev) = draft(&a, t, &owner).await;
    if home["schema_version"] == 2 {
        home["blocks"].as_array_mut().unwrap().push(json!({"id": "x9", "kind": "shop_cart", "x": 0, "y": 300, "w": 12, "h": 4, "style": "plain", "data": {}}));
        let r = save(&a, t, &owner, home, hrev).await;
        assert_eq!(r.status, 422, "{}", r.json());
    }

    // Terbit: tersedia untuk rute /keranjang tetapi tidak masuk menu navigasi.
    publish(&a, t, &owner).await;
    let r = page_req(
        &a,
        t,
        &owner,
        Method::POST,
        "keranjang",
        "/publish",
        Some(json!({})),
    )
    .await;
    assert_eq!(r.status, 200, "{}", r.json());
    let r = send(
        &a.router,
        Method::GET,
        "/api/v1/public/site?page=keranjang",
        None,
        Some("toko-a.ryasai.my.id"),
        None,
        vec![],
    )
    .await;
    assert_eq!(r.status, 200, "{}", r.json());
    assert_eq!(
        r.json()["data"]["page"]["content"]["blocks"][1]["data"]["body"],
        "Gratis ongkir!"
    );
    let nav: Vec<String> = r.json()["data"]["pages"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| p["slug"].as_str().unwrap().to_owned())
        .collect();
    assert!(!nav.contains(&"keranjang".to_owned()), "{nav:?}");

    // Tidak bisa dihapus.
    let r = delete_page_req(&a, t, &owner, "keranjang").await;
    assert_eq!(r.status, 400, "{}", r.json());
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn site_theme_is_drafted_then_published_over_every_page(pool: PgPool) {
    let a = app(&pool).await;
    let t = tenant(&pool, "toko-a").await;
    let owner = member(&pool, t, "Owner").await;
    let viewer = member(&pool, t, "Viewer").await;
    select(&a, t, &owner, "modern_store").await;
    let (_, rev) = draft(&a, t, &owner).await;
    let v2 = json!({"schema_version": 2, "template": "modern_store",
        "theme": {"primary": "#2563EB", "font": "inter"},
        "blocks": [{"id": "teks-1", "kind": "text", "x": 0, "y": 0, "w": 12, "h": 4, "style": "plain", "data": {"body": "Halo"}}]});
    assert_eq!(save(&a, t, &owner, v2, rev).await.status, 200);

    let theme = json!({"primary": "#0F766E", "font": "fraunces", "heading_font": "playfair",
        "heading_weight": 700, "accent": "#F59E0B", "radius": "round", "motion": "subtle"});
    let path = format!("{}/theme", w(t));
    assert_eq!(
        json(
            &a.router,
            Method::PUT,
            &path,
            Some(&viewer),
            Some(json!({"theme": theme}))
        )
        .await
        .status,
        403
    );
    let bad = json(
        &a.router,
        Method::PUT,
        &path,
        Some(&owner),
        Some(json!({"theme": {"primary": "biru", "font": "inter"}})),
    )
    .await;
    assert_eq!(bad.status, 422, "{}", bad.json());
    assert_eq!(paths(&bad), vec!["theme.primary:invalid_color"]);
    let ok = json(
        &a.router,
        Method::PUT,
        &path,
        Some(&owner),
        Some(json!({"theme": theme})),
    )
    .await;
    assert_eq!(ok.status, 200, "{}", ok.json());

    // Draf: terlihat di Studio, belum di situs publik.
    let s = json(&a.router, Method::GET, &w(t), Some(&owner), None).await;
    assert_eq!(s.json()["data"]["theme"]["accent"], "#F59E0B");
    assert_eq!(publish(&a, t, &owner).await.status, 200);
    let p = public(&a, "toko-a.ryasai.my.id").await;
    let th = &p.json()["data"]["page"]["content"]["theme"];
    assert_eq!(th["primary"], "#0F766E");
    assert_eq!(th["heading_font"], "playfair");
    assert_eq!(th["radius"], "round");
}
