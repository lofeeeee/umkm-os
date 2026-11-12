//! P2-6 HTTP: upload + signed URL (AC-FILE-001..006), role aplikasi + RLS.

mod common;

use axum::http::Method;
use common::*;
use sqlx::PgPool;
use umkm_database::MIGRATOR;
use umkm_shared::ids::TenantId;
use uuid::Uuid;

fn object_id(r: &Res) -> Uuid {
    r.json()["data"]["id"].as_str().unwrap().parse().unwrap()
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn owner_uploads_and_fetches_via_signed_url(pool: PgPool) {
    // AC-FILE-001, AC-FILE-003
    let a = app(&pool).await;
    let t = tenant(&pool, "toko-a").await;
    let owner = member(&pool, t, "Owner").await;
    let r = upload(&a.router, t, &owner, png()).await;
    assert_eq!(r.status, 201, "{}", r.json());
    let d = r.json()["data"].clone();
    let id = object_id(&r);
    assert_eq!(
        d["content_type"], "image/png",
        "dari isi, bukan header klien"
    );
    assert_eq!(d["ref"], format!("asset:{id}"));
    let url = d["url"].as_str().unwrap();
    assert!(url.starts_with(&format!("/files/{id}?t={t}&exp=")), "{url}");
    // AC-FILE-003: tidak ada nama file/path internal di respons.
    let raw = String::from_utf8(r.bytes.clone()).unwrap();
    assert!(
        !raw.contains("stored_objects") && !raw.contains(".png"),
        "{raw}"
    );

    let f = send(&a.router, Method::GET, url, None, None, None, vec![]).await;
    assert_eq!(f.status, 200);
    assert_eq!(f.bytes, png());
    assert_eq!(f.headers["content-type"], "image/png");
    assert_eq!(f.headers["x-content-type-options"], "nosniff");
    assert_eq!(
        f.headers["content-security-policy"],
        "default-src 'none'; sandbox"
    );
    assert!(
        f.headers["cache-control"]
            .to_str()
            .unwrap()
            .starts_with("private, max-age=")
    );

    // Audit tercatat (tenant + objek)
    let n: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM audit_logs WHERE tenant_id = $1 AND action = 'storage.object.uploaded' AND resource_id = $2",
    )
    .bind(t)
    .bind(id.to_string())
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(n, 1);
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn malicious_and_oversized_uploads_are_rejected_and_not_stored(pool: PgPool) {
    // AC-FILE-002
    let a = app(&pool).await;
    let t = tenant(&pool, "toko-a").await;
    let owner = member(&pool, t, "Owner").await;
    let mut polyglot = png();
    polyglot.extend(b"<script>alert(1)</script>");
    let mut too_big = jpeg();
    too_big.splice(2..2, std::iter::repeat_n(0u8, MAX_OBJECT));
    let cases: Vec<(&str, Vec<u8>, u16, &str)> = vec![
        (
            "svg",
            b"<svg xmlns='http://www.w3.org/2000/svg' onload='alert(1)'/>".to_vec(),
            422,
            "unsupported_type",
        ),
        (
            "html-as-png",
            b"<!doctype html><script>alert(1)</script>".to_vec(),
            422,
            "unsupported_type",
        ),
        ("polyglot", polyglot, 422, "malformed_file"),
        ("empty", vec![], 422, "empty_file"),
        ("too-big", too_big, 413, "file_too_large"),
    ];
    for (name, bytes, status, reason) in cases {
        let r = upload(&a.router, t, &owner, bytes).await;
        assert_eq!(r.status, status, "{name}: {}", r.json());
        if status == 422 {
            assert_eq!(r.json()["error"]["details"]["reason"], reason, "{name}");
        }
    }
    let n: i64 = sqlx::query_scalar("SELECT count(*) FROM stored_objects")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(n, 0);
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn quota_is_enforced(pool: PgPool) {
    let a = app(&pool).await;
    let t = tenant(&pool, "toko-a").await;
    let owner = member(&pool, t, "Owner").await;
    for _ in 0..MAX_OBJECTS {
        assert_eq!(upload(&a.router, t, &owner, png()).await.status, 201);
    }
    let r = upload(&a.router, t, &owner, png()).await;
    assert_eq!(r.status, 422);
    assert_eq!(r.json()["error"]["details"]["reason"], "quota_exceeded");
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn parallel_uploads_cannot_exceed_quota(pool: PgPool) {
    let a = app(&pool).await;
    let t = tenant(&pool, "toko-a").await;
    let owner = member(&pool, t, "Owner").await;
    for _ in 0..MAX_OBJECTS - 1 {
        assert_eq!(upload(&a.router, t, &owner, png()).await.status, 201);
    }
    let mut tasks = vec![];
    for _ in 0..(MAX_OBJECTS * 3) {
        let (r, o) = (
            a.router.clone(),
            Who {
                user: owner.user,
                cookie: owner.cookie.clone(),
                csrf: owner.csrf.clone(),
            },
        );
        tasks.push(tokio::spawn(async move {
            upload(&r, t, &o, png()).await.status
        }));
    }
    let mut created = 0;
    for task in tasks {
        if task.await.unwrap() == 201 {
            created += 1;
        }
    }
    assert_eq!(created, 1);
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn permissions_follow_rbac(pool: PgPool) {
    let a = app(&pool).await;
    let t = tenant(&pool, "toko-a").await;
    let viewer = member(&pool, t, "Viewer").await;
    let catalog = member(&pool, t, "Catalog Manager").await;
    assert_eq!(upload(&a.router, t, &viewer, png()).await.status, 403);
    assert_eq!(upload(&a.router, t, &catalog, png()).await.status, 403);
    // Viewer punya website.read -> boleh melihat daftar
    let r = json(
        &a.router,
        Method::GET,
        &format!("/api/v1/tenants/{t}/uploads"),
        Some(&viewer),
        None,
    )
    .await;
    assert_eq!(r.status, 200);
    // Tanpa sesi
    let r = send(
        &a.router,
        Method::POST,
        &format!("/api/v1/tenants/{t}/uploads"),
        None,
        None,
        None,
        png(),
    )
    .await;
    assert_eq!(r.status, 401);
}

fn tamper(url: &str, key: &str, value: &str) -> String {
    let (path, q) = url.split_once('?').unwrap();
    let q: Vec<String> = q
        .split('&')
        .map(|kv| {
            let (k, _) = kv.split_once('=').unwrap();
            if k == key {
                format!("{k}={value}")
            } else {
                kv.to_owned()
            }
        })
        .collect();
    format!("{path}?{}", q.join("&"))
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn signed_urls_cannot_cross_tenants_or_be_forged(pool: PgPool) {
    // AC-FILE-004, AC-FILE-005
    let a = app(&pool).await;
    let ta = tenant(&pool, "toko-a").await;
    let tb = tenant(&pool, "toko-b").await;
    let oa = member(&pool, ta, "Owner").await;
    let ob = member(&pool, tb, "Owner").await;
    let ra = upload(&a.router, ta, &oa, png()).await;
    let rb = upload(&a.router, tb, &ob, jpeg()).await;
    let (url_a, id_b) = (
        ra.json()["data"]["url"].as_str().unwrap().to_owned(),
        object_id(&rb),
    );
    let id_a = object_id(&ra);
    let get = |u: String| {
        let r = a.router.clone();
        async move {
            send(&r, Method::GET, &u, None, None, None, vec![])
                .await
                .status
        }
    };
    assert_eq!(get(url_a.clone()).await, 200);
    // Ganti objek di path ke milik B dengan signature A
    assert_eq!(
        get(url_a.replace(&id_a.to_string(), &id_b.to_string())).await,
        404
    );
    // Ganti tenant ke B
    assert_eq!(get(tamper(&url_a, "t", &tb.to_string())).await, 404);
    // Signature valid milik tenant A untuk objek B (penanda sah, tetapi objek bukan milik A) -> RLS
    let forged = a.signer.sign(TenantId::from_uuid(ta), id_b, common_now());
    assert_eq!(get(forged).await, 404);
    // Signature/expiry diubah, kedaluwarsa, sig tak valid base64
    assert_eq!(get(tamper(&url_a, "sig", "AAAA")).await, 404);
    assert_eq!(get(tamper(&url_a, "sig", "!!!")).await, 404);
    let exp: u64 = url_a
        .split("exp=")
        .nth(1)
        .unwrap()
        .split('&')
        .next()
        .unwrap()
        .parse()
        .unwrap();
    assert_eq!(
        get(tamper(&url_a, "exp", &(exp + 60).to_string())).await,
        404
    );
    let old = a.signer.sign(TenantId::from_uuid(ta), id_a, 1_000_000_000);
    assert_eq!(get(old).await, 404, "kedaluwarsa");
    // Tanpa query
    assert_eq!(get(format!("/files/{id_a}")).await, 404);
    assert_eq!(get("/files/not-a-uuid?t=x&exp=y&sig=z".into()).await, 404);
}

fn common_now() -> u64 {
    umkm_api::upload::now_unix()
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn tenant_b_cannot_list_or_delete_tenant_a_objects(pool: PgPool) {
    // AC-FILE-006 lewat HTTP
    let a = app(&pool).await;
    let ta = tenant(&pool, "toko-a").await;
    let tb = tenant(&pool, "toko-b").await;
    let oa = member(&pool, ta, "Owner").await;
    let ob = member(&pool, tb, "Owner").await;
    let id_a = object_id(&upload(&a.router, ta, &oa, png()).await);
    let list_b = json(
        &a.router,
        Method::GET,
        &format!("/api/v1/tenants/{tb}/uploads"),
        Some(&ob),
        None,
    )
    .await;
    assert_eq!(list_b.json()["data"].as_array().unwrap().len(), 0);
    // B memakai tenant_id A di path -> bukan anggota -> 404
    let r = json(
        &a.router,
        Method::DELETE,
        &format!("/api/v1/tenants/{ta}/uploads/{id_a}"),
        Some(&ob),
        None,
    )
    .await;
    assert_eq!(r.status, 404);
    // B memakai path miliknya dengan objek A -> 404, objek tetap ada
    let r = json(
        &a.router,
        Method::DELETE,
        &format!("/api/v1/tenants/{tb}/uploads/{id_a}"),
        Some(&ob),
        None,
    )
    .await;
    assert_eq!(r.status, 404);
    let n: i64 = sqlx::query_scalar("SELECT count(*) FROM stored_objects WHERE id = $1")
        .bind(id_a)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(n, 1);
    // Pemilik boleh menghapus
    let r = json(
        &a.router,
        Method::DELETE,
        &format!("/api/v1/tenants/{ta}/uploads/{id_a}"),
        Some(&oa),
        None,
    )
    .await;
    assert_eq!(r.status, 204);
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn object_referenced_by_a_page_cannot_be_deleted(pool: PgPool) {
    let a = app(&pool).await;
    let t = tenant(&pool, "toko-a").await;
    let o = member(&pool, t, "Owner").await;
    let id = object_id(&upload(&a.router, t, &o, png()).await);
    let tpl: Uuid =
        sqlx::query_scalar("SELECT id FROM website_templates WHERE key = 'modern_store'")
            .fetch_one(&pool)
            .await
            .unwrap();
    let w: Uuid = sqlx::query_scalar(
        "INSERT INTO tenant_websites (tenant_id, template_id) VALUES ($1, $2) RETURNING id",
    )
    .bind(t)
    .bind(tpl)
    .fetch_one(&pool)
    .await
    .unwrap();
    sqlx::query("INSERT INTO website_pages (tenant_id, website_id, slug, title, draft_content) VALUES ($1, $2, 'home', 'Beranda', $3)")
        .bind(t)
        .bind(w)
        .bind(sqlx::types::Json(serde_json::json!({"img": format!("asset:{id}")})))
        .execute(&pool)
        .await
        .unwrap();
    let r = json(
        &a.router,
        Method::DELETE,
        &format!("/api/v1/tenants/{t}/uploads/{id}"),
        Some(&o),
        None,
    )
    .await;
    assert_eq!(r.status, 409, "{}", r.json());
}

/// Kuota di bawah konkurensi pada port storage langsung (tanpa jeda HTTP/sesi yang
/// secara kebetulan menyerialkan request). Tanpa lock per tenant, beberapa transaksi
/// melihat count yang sama lalu sama-sama INSERT.
#[sqlx::test(migrator = "MIGRATOR")]
async fn concurrent_puts_on_the_port_respect_quota(pool: PgPool) {
    use umkm_infrastructure::storage::{ObjectStorage, PgObjectStorage, StorageLimits};
    let t = tenant(&pool, "toko-a").await;
    let o = member(&pool, t, "Owner").await;
    let store = PgObjectStorage::new(
        app_pool(&pool).await,
        StorageLimits {
            max_object_bytes: MAX_OBJECT,
            max_objects_per_tenant: MAX_OBJECTS,
        },
    );
    let actor = umkm_domain::rbac::Actor::new(o.user, TenantId::from_uuid(t), "Owner".into(), []);
    let img = png();
    // Sisakan satu slot; pool 5 koneksi -> tanpa lock hingga 5 transaksi melihat count = MAX-1.
    for _ in 0..MAX_OBJECTS - 1 {
        store.put(&actor, &img, (None, None)).await.unwrap();
    }
    let results = futures_util::future::join_all(
        (0..MAX_OBJECTS * 4).map(|_| store.put(&actor, &img, (None, None))),
    )
    .await;
    let ok = results.iter().filter(|r| r.is_ok()).count() as i64;
    assert_eq!(ok, 1, "{results:?}");
    let n: i64 = sqlx::query_scalar("SELECT count(*) FROM stored_objects WHERE tenant_id = $1")
        .bind(t)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(n, MAX_OBJECTS);
}
