//! P7-2: penegakan batas paket di backend (PRD.md §4.15, §12; FR-SUB-001, AC-SUB-001; ADR-0016).
//! Batas dibaca dari `plans.limits_json` tiap permintaan: mengubah data mengubah penegakan tanpa deploy.

mod common;

use axum::http::{Method, header};
use common::*;
use serde_json::{Value, json};
use sqlx::PgPool;
use umkm_database::MIGRATOR;
use uuid::Uuid;

const HOST: &str = "toko-a.ryasai.my.id";

async fn set_limit(pool: &PgPool, plan: &str, key: &str, v: Value) {
    sqlx::query(
        "UPDATE plans SET limits_json = jsonb_set(limits_json, ARRAY[$2], $3) WHERE code = $1",
    )
    .bind(plan)
    .bind(key)
    .bind(v)
    .execute(pool)
    .await
    .unwrap();
}

async fn post(a: &TestApp, w: &Who, uri: &str, body: Value) -> Res {
    json(&a.router, Method::POST, uri, Some(w), Some(body)).await
}

fn limit_of(r: &Res) -> (u16, Value, Value) {
    let d = r.json()["error"]["details"].clone();
    (r.status, d["reason"].clone(), d["limit"].clone())
}

async fn product(a: &TestApp, w: &Who, t: Uuid, n: usize) -> Res {
    post(a, w, &format!("/api/v1/tenants/{t}/products"), json!({"name": format!("Produk {n}"), "variants": [{"sku": format!("P-{n}"), "price": "1000"}]})).await
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn product_member_storage_limits_follow_plan_data(pool: PgPool) {
    let a = app(&pool).await;
    let t = tenant(&pool, "toko-a").await; // uji coba Starter
    let o = member(&pool, t, "Owner").await;

    // Produk: batas Starter diubah ke 2 di data → produk ke-3 ditolak.
    set_limit(&pool, "starter", "max_products", json!(2)).await;
    for n in 0..2 {
        assert_eq!(product(&a, &o, t, n).await.status, 201);
    }
    let r = product(&a, &o, t, 2).await;
    assert_eq!(
        limit_of(&r),
        (403, json!("plan_limit"), json!("max_products"))
    );
    assert_eq!(r.json()["error"]["details"]["max"], 2);
    // AC-SUB-001: naikkan batas di data → langsung diterima (tanpa deploy).
    set_limit(&pool, "starter", "max_products", json!(3)).await;
    assert_eq!(product(&a, &o, t, 2).await.status, 201);
    assert_eq!(
        limit_of(&product(&a, &o, t, 3).await).1,
        json!("plan_limit")
    );
    // Produk archived tidak dihitung.
    sqlx::query(
        "UPDATE products SET status = 'archived' WHERE tenant_id = $1 AND name = 'Produk 0'",
    )
    .bind(t)
    .execute(&pool)
    .await
    .unwrap();
    assert_eq!(product(&a, &o, t, 3).await.status, 201);
    // Pindah ke Pro → batas Pro berlaku.
    pro(&pool, t).await;
    assert_eq!(product(&a, &o, t, 4).await.status, 201);

    // Anggota: Starter 2 (owner + 1 undangan). Undangan tertunda memesan kursi.
    let t2 = tenant(&pool, "toko-b").await;
    let o2 = member(&pool, t2, "Owner").await;
    let role: Uuid = sqlx::query_scalar(
        "SELECT id FROM roles WHERE tenant_id IS NULL AND name = 'Support Agent'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    let inv_url = format!("/api/v1/tenants/{t2}/invitations");
    let inv =
        |email: &'static str| post(&a, &o2, &inv_url, json!({"email": email, "role_id": role}));
    assert_eq!(inv("a@x.id").await.status, 201);
    assert_eq!(
        limit_of(&inv("b@x.id").await),
        (403, json!("plan_limit"), json!("max_members"))
    );
    set_limit(&pool, "starter", "max_members", Value::Null).await; // null = tak terbatas
    assert_eq!(inv("b@x.id").await.status, 201);

    // Penyimpanan: total byte file tenant.
    set_limit(&pool, "starter", "storage_bytes", json!(10)).await;
    let r = upload(&a.router, t2, &o2, png()).await;
    assert_eq!(
        limit_of(&r),
        (403, json!("plan_limit"), json!("storage_bytes"))
    );
    set_limit(&pool, "starter", "storage_bytes", json!(10_000_000)).await;
    assert_eq!(upload(&a.router, t2, &o2, png()).await.status, 201);
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn ai_quota_comes_from_plan(pool: PgPool) {
    let a = app(&pool).await;
    let t = tenant(&pool, "toko-a").await;
    let o = member(&pool, t, "Owner").await;
    let prov: Uuid = sqlx::query_scalar(
        "INSERT INTO ai_providers (tenant_id, name, source, provider_type, model, capabilities)
         VALUES ($1, 'Platform', 'platform', 'platform', 'platform-small', '{\"chat\":true}') RETURNING id",
    )
    .bind(t)
    .fetch_one(&pool)
    .await
    .unwrap();
    sqlx::query("INSERT INTO chatbot_configs (tenant_id, enabled, mode, primary_provider_id) VALUES ($1, true, 'auto', $2)")
        .bind(t)
        .bind(prov)
        .execute(&pool)
        .await
        .unwrap();
    let test_url = format!("/api/v1/tenants/{t}/ai/test");
    let ask = || post(&a, &o, &test_url, json!({"message": "halo"}));
    // Kuota yang DITAMPILKAN (GET usage) = kuota yang DITEGAKKAN (paket ∩ operator), bukan kuota operator saja.
    let usage_url = format!("/api/v1/tenants/{t}/ai/usage");
    let quota = || async {
        let r = send(
            &a.router,
            Method::GET,
            &usage_url,
            Some(&o),
            None,
            None,
            vec![],
        )
        .await;
        assert_eq!(r.status, 200);
        r.json()["data"]["platform_quota"].as_u64()
    };
    assert_eq!(
        quota().await,
        Some(0),
        "Starter: token platform tidak termasuk paket"
    );
    // Uji coba Starter: token AI platform 0 → quota_exceeded tanpa memanggil provider.
    let r = ask().await;
    assert_eq!(r.status, 200, "{}", String::from_utf8_lossy(&r.bytes));
    assert_eq!(r.json()["data"]["status"], "quota_exceeded");
    // Data paket diubah (Starter dapat 1000 token) → tidak lagi quota_exceeded (provider uji tak terjangkau → error).
    set_limit(&pool, "starter", "platform_ai_tokens_monthly", json!(1000)).await;
    assert_ne!(ask().await.json()["data"]["status"], "quota_exceeded");
    assert_eq!(quota().await, Some(1000));
    // Kunci paket rusak → fail-closed: kuota 0.
    sqlx::query(
        "UPDATE plans SET limits_json = limits_json - 'byok_allowed' WHERE code = 'starter'",
    )
    .execute(&pool)
    .await
    .unwrap();
    assert_eq!(ask().await.json()["data"]["status"], "quota_exceeded");
    assert_eq!(
        quota().await,
        Some(0),
        "paket rusak → tampil 0 juga (fail-closed)"
    );
}

async fn public(a: &TestApp, m: Method, path: &str, body: Option<Value>) -> Res {
    use http_body_util::BodyExt as _;
    use tower::ServiceExt as _;
    let res = a
        .router
        .clone()
        .oneshot(
            axum::http::Request::builder()
                .method(m)
                .uri(path)
                .header(header::HOST, HOST)
                .header(header::ORIGIN, format!("https://{HOST}"))
                .header(header::CONTENT_TYPE, "application/json")
                .body(axum::body::Body::from(
                    body.map(|v| v.to_string()).unwrap_or_default(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    let status = res.status().as_u16();
    let headers = res.headers().clone();
    let bytes = res.into_body().collect().await.unwrap().to_bytes().to_vec();
    Res {
        status,
        headers,
        bytes,
    }
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn expired_subscription_disables_public_store_and_chat_quota(pool: PgPool) {
    let a = app(&pool).await;
    let t = tenant(&pool, "toko-a").await;
    let o = member(&pool, t, "Owner").await;
    let prov: Uuid = sqlx::query_scalar(
        "INSERT INTO ai_providers (tenant_id, name, source, provider_type, model, capabilities)
         VALUES ($1, 'Platform', 'platform', 'platform', 'platform-small', '{\"chat\":true}') RETURNING id",
    )
    .bind(t)
    .fetch_one(&pool)
    .await
    .unwrap();
    sqlx::query("INSERT INTO chatbot_configs (tenant_id, enabled, mode, primary_provider_id) VALUES ($1, true, 'human_only', $2)")
        .bind(t)
        .bind(prov)
        .execute(&pool)
        .await
        .unwrap();
    let start = || {
        public(
            &a,
            Method::POST,
            "/api/v1/public/chat/conversations",
            Some(json!({"consent_version": umkm_domain::ai::consent::VERSION, "accept": true})),
        )
    };
    // Kuota percakapan bulanan (Starter → 1 lewat data).
    set_limit(&pool, "starter", "conversations_monthly", json!(1)).await;
    assert_eq!(start().await.status, 201);
    assert_eq!(
        limit_of(&start().await),
        (403, json!("plan_limit"), json!("conversations_monthly"))
    );
    set_limit(&pool, "starter", "conversations_monthly", json!(100)).await;
    assert_eq!(start().await.status, 201);
    assert_eq!(
        public(&a, Method::GET, "/api/v1/public/products", None)
            .await
            .status,
        200
    );

    // Uji coba habis → fitur publik toko nonaktif.
    sqlx::query(
        "UPDATE subscriptions SET trial_ends_at = now() - interval '1 second' WHERE tenant_id = $1",
    )
    .bind(t)
    .execute(&pool)
    .await
    .unwrap();
    for p in [
        "/api/v1/public/site",
        "/api/v1/public/products",
        "/api/v1/public/categories",
        "/api/v1/public/checkout-config",
    ] {
        let r = public(&a, Method::GET, p, None).await;
        assert_eq!(
            (r.status, r.json()["error"]["details"]["reason"].clone()),
            (403, json!("subscription_expired")),
            "{p}"
        );
    }
    let r = public(&a, Method::POST, "/api/v1/public/checkout", Some(json!({}))).await;
    assert_ne!(r.status, 201, "checkout tidak boleh berhasil");
    assert_eq!(
        public(&a, Method::GET, "/api/v1/public/chat/config", None)
            .await
            .json()["data"]["enabled"],
        false
    );
    assert_eq!(
        start().await.json()["error"]["details"]["reason"],
        "subscription_expired"
    );
    // Dashboard tetap bisa dibuka untuk membayar / mengelola.
    let s = json(
        &a.router,
        Method::GET,
        &format!("/api/v1/tenants/{t}/subscription"),
        Some(&o),
        None,
    )
    .await;
    assert_eq!(
        (s.status, s.json()["data"]["status"].clone()),
        (200, json!("expired"))
    );
    assert_eq!(s.json()["data"]["usage"]["conversations_month"], 2);
    assert_eq!(
        json(
            &a.router,
            Method::GET,
            &format!("/api/v1/tenants/{t}/products"),
            Some(&o),
            None
        )
        .await
        .status,
        200
    );
    // Aktif lagi (Pro) → toko kembali hidup.
    pro(&pool, t).await;
    assert_eq!(
        public(&a, Method::GET, "/api/v1/public/products", None)
            .await
            .status,
        200
    );
}
