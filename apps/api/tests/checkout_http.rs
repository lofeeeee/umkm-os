//! P3-5: checkout transaction boundary (AC-ORD-001/002/003/006/009/010, AC-INV-005 via HTTP,
//! AC-CRM-003), pengaturan toko + expiry order. Role aplikasi + RLS.

mod common;

use axum::http::{Method, header};
use common::*;
use serde_json::{Value, json};
use sqlx::PgPool;
use umkm_database::MIGRATOR;
use umkm_infrastructure::{commerce_repo, inventory_repo};
use umkm_shared::ids::TenantId;
use uuid::Uuid;

const HOST: &str = "toko-a.ryasai.my.id";

struct Shop {
    t: Uuid,
    owner: Who,
    product: String,
    variant: String,
}

async fn req(
    a: &TestApp,
    m: Method,
    host: &str,
    path: &str,
    headers: &[(&str, &str)],
    body: Option<Value>,
) -> Res {
    use http_body_util::BodyExt as _;
    use tower::ServiceExt as _;
    let mut b = axum::http::Request::builder()
        .method(m)
        .uri(path)
        .header(header::HOST, host)
        .header(header::CONTENT_TYPE, "application/json");
    for (k, v) in headers {
        b = b.header(*k, *v);
    }
    let res = a
        .router
        .clone()
        .oneshot(
            b.body(axum::body::Body::from(
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

async fn staff(a: &TestApp, m: Method, url: String, who: &Who, body: Option<Value>) -> Res {
    json(&a.router, m, &url, Some(who), body).await
}

/// Toko siap jual: pickup+delivery, produk aktif dengan stok, QRIS diatur.
async fn shop(a: &TestApp, pool: &PgPool, slug: &str, stock: i64) -> Shop {
    let t = tenant(pool, slug).await;
    let o = member(pool, t, "Owner").await;
    sqlx::query("UPDATE business_profiles SET pickup_available = true, delivery_available = true WHERE tenant_id = $1")
        .bind(t)
        .execute(&a.owner)
        .await
        .unwrap();
    let p = staff(a, Method::POST, format!("/api/v1/tenants/{t}/products"), &o, Some(json!({
        "name": "Kopi Susu", "variants": [{"sku": "KS-1", "price": "18000", "promo_price": "15000"}]
    }))).await.json()["data"].clone();
    let pid = p["id"].as_str().unwrap().to_owned();
    let vid = p["variants"][0]["id"].as_str().unwrap().to_owned();
    staff(
        a,
        Method::POST,
        format!("/api/v1/tenants/{t}/products/{pid}/status"),
        &o,
        Some(json!({"status": "active"})),
    )
    .await;
    if stock > 0 {
        staff(
            a,
            Method::POST,
            format!("/api/v1/tenants/{t}/inventory/{vid}/movements"),
            &o,
            Some(json!({"kind": "stock_in", "quantity": stock, "reason": "awal"})),
        )
        .await;
    }
    let qris = upload(&a.router, t, &o, png()).await.json()["data"]["id"]
        .as_str()
        .unwrap()
        .to_owned();
    let r = staff(a, Method::PUT, format!("/api/v1/tenants/{t}/commerce/settings"), &o, Some(json!({
        "qris": format!("asset:{qris}"), "qris_receiver_name": "Toko A", "checkout_enabled": true,
        "delivery_fee": "10000"
    }))).await;
    assert_eq!(r.status, 200, "{}", r.json());
    Shop {
        t,
        owner: o,
        product: pid,
        variant: vid,
    }
}

fn cart(variant: &str, qty: i64) -> Value {
    json!({
        "items": [{"variant_id": variant, "quantity": qty}],
        "fulfillment": "pickup", "name": "Sari", "email": "sari@x.id",
        "accept_privacy": true, "accept_terms": true
    })
}

async fn checkout(a: &TestApp, host: &str, key: &str, body: Value) -> Res {
    req(
        a,
        Method::POST,
        host,
        "/api/v1/public/checkout",
        &[("idempotency-key", key)],
        Some(body),
    )
    .await
}

async fn level(a: &TestApp, t: Uuid, v: &str) -> (i64, i64) {
    let l = inventory_repo::level(
        &a.state.db.clone().unwrap(),
        TenantId::from_uuid(t),
        Uuid::parse_str(v).unwrap(),
    )
    .await
    .unwrap()
    .unwrap();
    (l.on_hand, l.reserved)
}

fn codes(r: &Res) -> Vec<String> {
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
async fn checkout_requires_configured_shop(pool: PgPool) {
    let a = app(&pool).await;
    let t = tenant(&pool, "toko-a").await;
    let o = member(&pool, t, "Owner").await;
    let r = checkout(&a, HOST, "key-000001", cart(&Uuid::new_v4().to_string(), 1)).await;
    assert_eq!(
        r.json()["error"]["details"]["reason"],
        "payment_not_configured"
    );
    // checkout tidak bisa diaktifkan tanpa QRIS; QRIS toko lain ditolak; Manager tidak boleh
    let url = format!("/api/v1/tenants/{t}/commerce/settings");
    let r = staff(
        &a,
        Method::PUT,
        url.clone(),
        &o,
        Some(json!({"checkout_enabled": true})),
    )
    .await;
    assert_eq!(codes(&r), vec!["qris:required_for_checkout".to_owned()]);
    let tb = tenant(&pool, "toko-b").await;
    let ob = member(&pool, tb, "Owner").await;
    let other = upload(&a.router, tb, &ob, png()).await.json()["data"]["id"]
        .as_str()
        .unwrap()
        .to_owned();
    let r = staff(
        &a,
        Method::PUT,
        url.clone(),
        &o,
        Some(json!({
            "qris": format!("asset:{other}"), "qris_receiver_name": "X", "checkout_enabled": true
        })),
    )
    .await;
    assert_eq!(codes(&r), vec!["qris:invalid_asset_ref".to_owned()]);
    let mgr = member(&pool, t, "Manager").await;
    assert_eq!(
        staff(&a, Method::PUT, url, &mgr, Some(json!({})))
            .await
            .status,
        403
    );
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn guest_checkout_prices_on_server_and_reserves_stock(pool: PgPool) {
    // AC-ORD-002, AC-ORD-010
    let a = app(&pool).await;
    let s = shop(&a, &pool, "toko-a", 5).await;
    let mut body = cart(&s.variant, 2);
    body["items"][0]["price"] = json!("1");
    body["total"] = json!("1");
    body["subtotal"] = json!("1");
    let r = checkout(&a, HOST, "key-000001", body).await;
    assert_eq!(r.status, 201, "{}", r.json());
    assert_eq!(r.headers["cache-control"], "private, no-store");
    let d = r.json()["data"].clone();
    assert_eq!(
        d["order"]["total"], "30000.00",
        "harga promo server × 2, harga klien diabaikan"
    );
    assert_eq!(d["order"]["delivery_fee"], "0.00", "pickup tanpa ongkir");
    assert_eq!(d["order"]["status"], "AWAITING_PAYMENT");
    assert_eq!(d["order"]["payment"]["status"], "pending");
    assert_eq!(d["order"]["payment"]["amount"], "30000.00");
    assert_eq!(d["order"]["payment"]["receiver_name"], "Toko A");
    assert!(
        d["order"]["payment"]["qris_url"]
            .as_str()
            .unwrap()
            .starts_with("/files/")
    );
    assert_eq!(
        level(&a, s.t, &s.variant).await,
        (5, 2),
        "stok direservasi, belum dipotong"
    );

    let (pv, tv, cust): (String, String, Uuid) = sqlx::query_as(
        "SELECT privacy_version, terms_version, customer_id FROM orders WHERE code = $1",
    )
    .bind(d["order"]["code"].as_str().unwrap())
    .fetch_one(&a.owner)
    .await
    .unwrap();
    assert_eq!(
        (pv.as_str(), tv.as_str()),
        ("v0", "v0"),
        "versi D-12 tersimpan"
    );
    let n: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM notification_outbox WHERE event_type = 'order.created'",
    )
    .fetch_one(&a.owner)
    .await
    .unwrap();
    assert_eq!(n, 1);

    // delivery: ongkir toko diterapkan, alamat wajib
    let mut del = cart(&s.variant, 1);
    del["fulfillment"] = json!("delivery");
    let r = checkout(&a, HOST, "key-000002", del.clone()).await;
    assert_eq!(codes(&r), vec!["delivery_address:required".to_owned()]);
    del["delivery_address"] = json!("Jl. Mawar 1");
    let r = checkout(&a, HOST, "key-000003", del).await;
    assert_eq!(r.json()["data"]["order"]["total"], "25000.00");

    // persetujuan privacy/terms wajib
    let mut b = cart(&s.variant, 1);
    b["accept_terms"] = json!(false);
    let r = checkout(&a, HOST, "key-000004", b).await;
    assert_eq!(codes(&r), vec!["accept_terms:required".to_owned()]);

    // AC-CRM-003: staf melihat order pada detail customer
    let r = staff(
        &a,
        Method::GET,
        format!("/api/v1/tenants/{}/customers/{cust}", s.t),
        &s.owner,
        None,
    )
    .await;
    assert_eq!(r.json()["data"]["orders"].as_array().unwrap().len(), 1);
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn invalid_carts_are_rejected(pool: PgPool) {
    // AC-ORD-001
    let a = app(&pool).await;
    let s = shop(&a, &pool, "toko-a", 5).await;
    let b = shop(&a, &pool, "toko-b", 5).await;
    let r = checkout(&a, HOST, "key-000001", cart(&s.variant, 0)).await;
    assert_eq!(
        codes(&r),
        vec!["items[0].quantity:invalid_quantity".to_owned()]
    );
    let r = checkout(&a, HOST, "key-000002", cart(&b.variant, 1)).await;
    assert_eq!(
        codes(&r),
        vec!["items[0].variant_id:product_unavailable".to_owned()],
        "varian toko lain"
    );
    staff(
        &a,
        Method::POST,
        format!("/api/v1/tenants/{}/products/{}/status", s.t, s.product),
        &s.owner,
        Some(json!({"status": "archived"})),
    )
    .await;
    let r = checkout(&a, HOST, "key-000003", cart(&s.variant, 1)).await;
    assert_eq!(
        codes(&r),
        vec!["items[0].variant_id:product_unavailable".to_owned()],
        "produk arsip"
    );
    let r = checkout(&a, HOST, "bad key", cart(&s.variant, 1)).await;
    assert_eq!(codes(&r), vec!["Idempotency-Key:required".to_owned()]);
    let r = req(
        &a,
        Method::POST,
        HOST,
        "/api/v1/public/checkout",
        &[
            ("idempotency-key", "key-000005"),
            ("origin", "https://toko-b.ryasai.my.id"),
        ],
        Some(cart(&s.variant, 1)),
    )
    .await;
    assert_eq!(r.status, 403, "Origin toko lain");
    assert_eq!(
        level(&a, s.t, &s.variant).await,
        (5, 0),
        "tidak ada reservasi tersisa"
    );
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn idempotency_key_creates_exactly_one_order(pool: PgPool) {
    // AC-ORD-006
    let a = app(&pool).await;
    let s = shop(&a, &pool, "toko-a", 50).await;
    let first = checkout(&a, HOST, "key-same-01", cart(&s.variant, 1)).await;
    let again = checkout(&a, HOST, "key-same-01", cart(&s.variant, 1)).await;
    assert_eq!((first.status, again.status), (201, 200));
    assert_eq!(again.json()["data"]["replay"], true);
    assert_eq!(
        first.json()["data"]["order"]["code"],
        again.json()["data"]["order"]["code"]
    );
    assert_eq!(
        first.json()["data"]["access_token"],
        again.json()["data"]["access_token"]
    );
    let r = checkout(&a, HOST, "key-same-01", cart(&s.variant, 2)).await;
    assert_eq!(
        r.json()["error"]["details"]["reason"],
        "idempotency_key_reused"
    );

    // paralel: 10 request kunci sama → 1 order, semua respons menunjuk order yang sama
    let rs = futures_util::future::join_all(
        (0..10).map(|_| checkout(&a, HOST, "key-par-0001", cart(&s.variant, 1))),
    )
    .await;
    let ok: Vec<_> = rs
        .iter()
        .filter(|r| matches!(r.status, 200 | 201))
        .collect();
    assert_eq!(
        ok.len(),
        10,
        "{:?}",
        rs.iter().map(|r| r.status).collect::<Vec<_>>()
    );
    assert_eq!(rs.iter().filter(|r| r.status == 201).count(), 1);
    let codes: std::collections::BTreeSet<_> = ok
        .iter()
        .map(|r| r.json()["data"]["order"]["code"].clone().to_string())
        .collect();
    assert_eq!(codes.len(), 1);
    let n: i64 =
        sqlx::query_scalar("SELECT count(*) FROM orders WHERE idempotency_key = 'key-par-0001'")
            .fetch_one(&a.owner)
            .await
            .unwrap();
    assert_eq!(n, 1);
    assert_eq!(
        level(&a, s.t, &s.variant).await,
        (50, 2),
        "reservasi tidak ganda"
    );
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn parallel_checkouts_do_not_oversell(pool: PgPool) {
    // AC-INV-005 lewat HTTP checkout
    let a = app(&pool).await;
    let s = shop(&a, &pool, "toko-a", 1).await;
    let rs = futures_util::future::join_all((0..10).map(|i| {
        checkout(
            &a,
            HOST,
            Box::leak(format!("key-race-{i:04}").into_boxed_str()),
            cart(&s.variant, 1),
        )
    }))
    .await;
    assert_eq!(rs.iter().filter(|r| r.status == 201).count(), 1);
    assert!(
        rs.iter()
            .filter(|r| r.status != 201)
            .all(|r| r.status == 409
                && r.json()["error"]["details"]["reason"] == "insufficient_stock")
    );
    assert_eq!(level(&a, s.t, &s.variant).await, (1, 1));
    let n: i64 = sqlx::query_scalar("SELECT count(*) FROM orders")
        .fetch_one(&a.owner)
        .await
        .unwrap();
    assert_eq!(n, 1, "checkout gagal tidak meninggalkan order/customer");
    let c: i64 = sqlx::query_scalar("SELECT count(*) FROM customers")
        .fetch_one(&a.owner)
        .await
        .unwrap();
    assert_eq!(c, 1);
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn order_snapshot_is_immutable(pool: PgPool) {
    // AC-ORD-003
    let a = app(&pool).await;
    let s = shop(&a, &pool, "toko-a", 5).await;
    let r = checkout(&a, HOST, "key-000001", cart(&s.variant, 1)).await;
    let token = r.json()["data"]["access_token"]
        .as_str()
        .unwrap()
        .to_owned();
    let mut body = json!({"name": "Kopi Mahal", "variants": [{"sku": "KS-1", "price": "99000"}]});
    body["variants"][0]["id"] = json!(s.variant);
    let r = staff(
        &a,
        Method::PUT,
        format!("/api/v1/tenants/{}/products/{}", s.t, s.product),
        &s.owner,
        Some(body),
    )
    .await;
    assert_eq!(r.status, 200, "{}", r.json());
    let o = req(
        &a,
        Method::GET,
        HOST,
        "/api/v1/public/orders/current",
        &[("x-order-token", &token)],
        None,
    )
    .await;
    let it = &o.json()["data"]["items"][0];
    assert_eq!(
        (it["product_name"].as_str(), it["unit_price"].as_str()),
        (Some("Kopi Susu"), Some("15000.00"))
    );
    assert_eq!(o.json()["data"]["total"], "15000.00");
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn order_access_is_token_bound(pool: PgPool) {
    // AC-ORD-009
    let a = app(&pool).await;
    let s = shop(&a, &pool, "toko-a", 5).await;
    shop(&a, &pool, "toko-b", 5).await;
    let r1 = checkout(&a, HOST, "key-000001", cart(&s.variant, 1)).await;
    let r2 = checkout(&a, HOST, "key-000002", cart(&s.variant, 1)).await;
    let (t1, t2) = (
        r1.json()["data"]["access_token"]
            .as_str()
            .unwrap()
            .to_owned(),
        r2.json()["data"]["access_token"]
            .as_str()
            .unwrap()
            .to_owned(),
    );
    let get = |host: &'static str, tok: String| {
        let a = &a;
        async move {
            req(
                a,
                Method::GET,
                host,
                "/api/v1/public/orders/current",
                &[("x-order-token", &tok)],
                None,
            )
            .await
        }
    };
    assert_eq!(
        get(HOST, t1.clone()).await.json()["data"]["code"],
        r1.json()["data"]["order"]["code"]
    );
    assert_eq!(
        get(HOST, t2).await.json()["data"]["code"],
        r2.json()["data"]["order"]["code"]
    );
    assert_eq!(get(HOST, format!("{t1}x")).await.status, 404);
    assert_eq!(
        get("toko-b.ryasai.my.id", t1).await.status,
        404,
        "token toko A di host toko B"
    );
    assert_eq!(
        req(
            &a,
            Method::GET,
            HOST,
            "/api/v1/public/orders/current",
            &[],
            None
        )
        .await
        .status,
        404
    );
    // respons publik tidak memuat data internal
    let raw = String::from_utf8(
        get(
            HOST,
            r1.json()["data"]["access_token"]
                .as_str()
                .unwrap()
                .to_owned(),
        )
        .await
        .bytes,
    )
    .unwrap();
    assert!(
        !raw.contains("customer_id") && !raw.contains("events") && !raw.contains("verifier_note"),
        "{raw}"
    );
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn unpaid_orders_expire_and_release_stock(pool: PgPool) {
    let a = app(&pool).await;
    let s = shop(&a, &pool, "toko-a", 3).await;
    let r = checkout(&a, HOST, "key-000001", cart(&s.variant, 2)).await;
    let code = r.json()["data"]["order"]["code"]
        .as_str()
        .unwrap()
        .to_owned();
    let db = a.state.db.clone().unwrap();
    assert_eq!(
        commerce_repo::expire_orders(&db, 100).await.unwrap(),
        0,
        "belum lewat batas"
    );
    // reservasi order tidak dilepas job reservasi umum walau sudah lewat waktunya
    sqlx::query("UPDATE inventory_reservations SET expires_at = now() - interval '1 minute'")
        .execute(&a.owner)
        .await
        .unwrap();
    assert_eq!(
        inventory_repo::expire_reservations(&db, 100).await.unwrap(),
        0
    );
    sqlx::query("UPDATE orders SET expires_at = now() - interval '1 minute' WHERE code = $1")
        .bind(&code)
        .execute(&a.owner)
        .await
        .unwrap();
    assert_eq!(commerce_repo::expire_orders(&db, 100).await.unwrap(), 1);
    assert_eq!(commerce_repo::expire_orders(&db, 100).await.unwrap(), 0);
    assert_eq!(level(&a, s.t, &s.variant).await, (3, 0));
    let r = staff(
        &a,
        Method::GET,
        format!("/api/v1/tenants/{}/orders?status=EXPIRED", s.t),
        &s.owner,
        None,
    )
    .await;
    assert_eq!(r.json()["data"]["total"], 1);
    let id = r.json()["data"]["items"][0]["id"]
        .as_str()
        .unwrap()
        .to_owned();
    let d = staff(
        &a,
        Method::GET,
        format!("/api/v1/tenants/{}/orders/{id}", s.t),
        &s.owner,
        None,
    )
    .await
    .json()["data"]
        .clone();
    assert_eq!(d["payment"]["status"], "expired");
    let evs: Vec<_> = d["events"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| e["to_status"].as_str().unwrap().to_owned())
        .collect();
    assert_eq!(evs, ["AWAITING_PAYMENT", "EXPIRED"]);
    assert_eq!(d["events"][1]["actor_type"], "system");
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn staff_order_access_is_scoped(pool: PgPool) {
    let a = app(&pool).await;
    let s = shop(&a, &pool, "toko-a", 5).await;
    let b = shop(&a, &pool, "toko-b", 5).await;
    checkout(&a, HOST, "key-000001", cart(&s.variant, 1)).await;
    let catalog = member(&pool, s.t, "Catalog Manager").await;
    let viewer = member(&pool, s.t, "Viewer").await;
    let list = staff(
        &a,
        Method::GET,
        format!("/api/v1/tenants/{}/orders", s.t),
        &viewer,
        None,
    )
    .await;
    assert_eq!(list.json()["data"]["total"], 1);
    let id = list.json()["data"]["items"][0]["id"]
        .as_str()
        .unwrap()
        .to_owned();
    assert_eq!(
        staff(
            &a,
            Method::GET,
            format!("/api/v1/tenants/{}/orders", s.t),
            &catalog,
            None
        )
        .await
        .status,
        403
    );
    assert_eq!(
        staff(
            &a,
            Method::GET,
            format!("/api/v1/tenants/{}/orders/{id}", b.t),
            &b.owner,
            None
        )
        .await
        .status,
        404
    );
    assert_eq!(
        staff(
            &a,
            Method::GET,
            format!("/api/v1/tenants/{}/orders", b.t),
            &b.owner,
            None
        )
        .await
        .json()["data"]["total"],
        0
    );
}
