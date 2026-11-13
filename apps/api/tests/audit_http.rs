//! P4-5: audit log minimal (PRD.md §6.9): setiap kategori yang modulnya sudah ada menghasilkan
//! event audit lewat alur nyata; append-only; tanpa rahasia; hanya `audit.read`; isolasi tenant.

mod common;

use axum::http::{Method, header};
use common::*;
use serde_json::{Value, json};
use sqlx::PgPool;
use umkm_database::MIGRATOR;
use umkm_domain::audit::PRD_CATEGORIES;
use uuid::Uuid;

const APP: &str = "https://app.ryasai.my.id";
const HOST: &str = "toko-audit.ryasai.my.id";

async fn raw(
    a: &TestApp,
    m: Method,
    uri: &str,
    headers: &[(&str, &str)],
    body: Option<Value>,
) -> Res {
    use http_body_util::BodyExt as _;
    use tower::ServiceExt as _;
    let mut b = axum::http::Request::builder()
        .method(m)
        .uri(uri)
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

async fn call(a: &TestApp, m: Method, url: String, who: &Who, body: Option<Value>) -> Res {
    let r = json(&a.router, m, &url, Some(who), body).await;
    assert!(r.status < 300, "{url}: {} {}", r.status, r.json());
    r
}

async fn checkout(a: &TestApp, vid: &str) -> (Uuid, String) {
    let body = json!({"items": [{"variant_id": vid, "quantity": 1}], "fulfillment": "pickup", "name": "Sari", "email": "sari@x.id", "accept_privacy": true, "accept_terms": true});
    let key = format!("key-{}", Uuid::new_v4().simple());
    let r = raw(
        a,
        Method::POST,
        "/api/v1/public/checkout",
        &[("host", HOST), ("idempotency-key", &key)],
        Some(body),
    )
    .await;
    assert_eq!(r.status, 201, "{}", r.json());
    let code = r.json()["data"]["order"]["code"]
        .as_str()
        .unwrap()
        .to_owned();
    let id: Uuid = sqlx::query_scalar("SELECT id FROM orders WHERE code = $1")
        .bind(&code)
        .fetch_one(&a.owner)
        .await
        .unwrap();
    (
        id,
        r.json()["data"]["access_token"]
            .as_str()
            .unwrap()
            .to_owned(),
    )
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn every_available_prd_category_is_audited(pool: PgPool) {
    let a = app(&pool).await;
    // --- 1 security: registrasi, login gagal, login, logout
    let r = raw(&a, Method::POST, "/api/v1/auth/register", &[("origin", APP)],
        Some(json!({"email": "owner@x.id", "password": "rahasia-owner-1", "business_name": "Toko Audit", "slug": "toko-audit", "accept_terms": true, "accept_privacy": true}))).await;
    assert_eq!(r.status, 201, "{}", r.json());
    let t: Uuid = r.json()["data"]["tenant"]["id"]
        .as_str()
        .unwrap()
        .parse()
        .unwrap();
    pro(&a.owner, t).await; // kategori AI + anggota tambahan (ADR-0016)
    assert_eq!(
        raw(
            &a,
            Method::POST,
            "/api/v1/auth/login",
            &[("origin", APP)],
            Some(json!({"email": "owner@x.id", "password": "salah-sekali-1"}))
        )
        .await
        .status,
        401
    );
    let r = raw(
        &a,
        Method::POST,
        "/api/v1/auth/login",
        &[("origin", APP)],
        Some(json!({"email": "owner@x.id", "password": "rahasia-owner-1"})),
    )
    .await;
    let cookie = r.headers[header::SET_COOKIE]
        .to_str()
        .unwrap()
        .split(';')
        .next()
        .unwrap()
        .to_owned();
    let o = Who {
        user: r.json()["data"]["user_id"]
            .as_str()
            .unwrap()
            .parse()
            .unwrap(),
        cookie,
        csrf: r.json()["data"]["csrf_token"].as_str().unwrap().to_owned(),
    };
    // sesi kedua untuk logout (sesi utama tetap dipakai)
    let r2 = raw(
        &a,
        Method::POST,
        "/api/v1/auth/login",
        &[("origin", APP)],
        Some(json!({"email": "owner@x.id", "password": "rahasia-owner-1"})),
    )
    .await;
    let c2 = r2.headers[header::SET_COOKIE]
        .to_str()
        .unwrap()
        .split(';')
        .next()
        .unwrap()
        .to_owned();
    let csrf2 = r2.json()["data"]["csrf_token"].as_str().unwrap().to_owned();
    assert_eq!(
        raw(
            &a,
            Method::POST,
            "/api/v1/auth/logout",
            &[("origin", APP), ("cookie", &c2), ("x-csrf-token", &csrf2)],
            None
        )
        .await
        .status,
        200
    );
    let base = format!("/api/v1/tenants/{t}");
    let mgr = member(&pool, t, "Manager").await;

    // --- 2 tim
    let viewer_role = call(&a, Method::GET, format!("{base}/roles"), &o, None)
        .await
        .json()["data"]
        .as_array()
        .unwrap()
        .iter()
        .find(|x| x["name"] == "Viewer")
        .unwrap()["id"]
        .as_str()
        .unwrap()
        .to_owned();
    call(
        &a,
        Method::POST,
        format!("{base}/invitations"),
        &o,
        Some(json!({"email": "baru@x.id", "role_id": viewer_role})),
    )
    .await;
    call(
        &a,
        Method::POST,
        format!("{base}/roles"),
        &o,
        Some(json!({"name": "Kasir", "permissions": ["orders.read"]})),
    )
    .await;
    let v = member(&pool, t, "Viewer").await;
    let vm = call(&a, Method::GET, format!("{base}/members"), &o, None)
        .await
        .json()["data"]
        .as_array()
        .unwrap()
        .iter()
        .find(|m| m["user_id"] == v.user.to_string())
        .unwrap()["membership_id"]
        .as_str()
        .unwrap()
        .to_owned();
    call(
        &a,
        Method::PATCH,
        format!("{base}/members/{vm}"),
        &o,
        Some(json!({"status": "suspended"})),
    )
    .await;

    // --- 3 bisnis / website / pengaturan
    let mut profile = call(
        &a,
        Method::GET,
        format!("{base}/business/profile"),
        &o,
        None,
    )
    .await
    .json()["data"]["profile"]
        .clone();
    profile["pickup_available"] = json!(true);
    call(
        &a,
        Method::PUT,
        format!("{base}/business/profile"),
        &o,
        Some(profile),
    )
    .await;
    call(
        &a,
        Method::PUT,
        format!("{base}/website"),
        &o,
        Some(json!({"template_key": "modern_store"})),
    )
    .await;
    call(
        &a,
        Method::POST,
        format!("{base}/website/pages/home/publish"),
        &o,
        Some(json!({})),
    )
    .await;
    let q = upload(&a.router, t, &o, png()).await.json()["data"]["id"]
        .as_str()
        .unwrap()
        .to_owned();
    call(&a, Method::PUT, format!("{base}/commerce/settings"), &o, Some(json!({"qris": format!("asset:{q}"), "qris_receiver_name": "T", "checkout_enabled": true, "order_approval_required": true}))).await;

    // --- 4 produk & stok
    let p = call(
        &a,
        Method::POST,
        format!("{base}/products"),
        &o,
        Some(json!({"name": "Kopi", "variants": [{"sku": "K-1", "price": "15000"}]})),
    )
    .await
    .json()["data"]
        .clone();
    let (pid, vid) = (
        p["id"].as_str().unwrap().to_owned(),
        p["variants"][0]["id"].as_str().unwrap().to_owned(),
    );
    call(
        &a,
        Method::PUT,
        format!("{base}/products/{pid}"),
        &o,
        Some(
            json!({"name": "Kopi Susu", "variants": [{"id": vid, "sku": "K-1", "price": "15000"}]}),
        ),
    )
    .await;
    call(
        &a,
        Method::POST,
        format!("{base}/products/{pid}/status"),
        &o,
        Some(json!({"status": "active"})),
    )
    .await;
    call(
        &a,
        Method::POST,
        format!("{base}/inventory/{vid}/movements"),
        &o,
        Some(json!({"kind": "stock_in", "quantity": 20, "reason": "awal"})),
    )
    .await;

    // --- 5/6 pesanan & pembayaran
    let (o1, tok1) = checkout(&a, &vid).await;
    raw(
        &a,
        Method::POST,
        "/api/v1/public/orders/current/payment-claim",
        &[("host", HOST), ("x-order-token", &tok1)],
        Some(json!({})),
    )
    .await;
    call(
        &a,
        Method::POST,
        format!("{base}/orders/{o1}/payment/confirm"),
        &mgr,
        Some(json!({"amount_received": "15000"})),
    )
    .await;
    call(
        &a,
        Method::POST,
        format!("{base}/orders/{o1}/approve"),
        &o,
        Some(json!({})),
    )
    .await;
    call(
        &a,
        Method::POST,
        format!("{base}/orders/{o1}/refunds"),
        &o,
        Some(json!({"amount": "15000", "reason": "Salah kirim", "restock": true})),
    )
    .await;
    let (o2, _) = checkout(&a, &vid).await;
    call(
        &a,
        Method::POST,
        format!("{base}/orders/{o2}/payment/confirm"),
        &mgr,
        Some(json!({"amount_received": "15000"})),
    )
    .await;
    call(
        &a,
        Method::POST,
        format!("{base}/orders/{o2}/reject"),
        &o,
        Some(json!({"reason": "Habis"})),
    )
    .await;
    let (o3, _) = checkout(&a, &vid).await;
    call(
        &a,
        Method::POST,
        format!("{base}/orders/{o3}/payment/reject"),
        &o,
        Some(json!({"reason": "Dana tidak masuk"})),
    )
    .await;
    let (o4, _) = checkout(&a, &vid).await;
    call(
        &a,
        Method::POST,
        format!("{base}/orders/{o4}/status"),
        &o,
        Some(json!({"status": "CANCELLED"})),
    )
    .await;

    // --- 7 ticket
    let tk = call(
        &a,
        Method::POST,
        format!("{base}/tickets"),
        &o,
        Some(json!({"subject": "Komplain"})),
    )
    .await
    .json()["data"]["id"]
        .as_str()
        .unwrap()
        .to_owned();
    call(
        &a,
        Method::POST,
        format!("{base}/tickets/{tk}/assign"),
        &o,
        Some(json!({"assignee_user_id": mgr.user})),
    )
    .await;
    call(
        &a,
        Method::POST,
        format!("{base}/tickets/{tk}/status"),
        &o,
        Some(json!({"status": "RESOLVED"})),
    )
    .await;
    call(
        &a,
        Method::POST,
        format!("{base}/tickets/{tk}/status"),
        &o,
        Some(json!({"status": "CLOSED"})),
    )
    .await;

    // --- 9 approval
    call(
        &a,
        Method::PUT,
        format!("{base}/approval-policies/stock_adjustment"),
        &o,
        Some(json!({"enabled": true, "steps": [{}]})),
    )
    .await;
    let ar = call(
        &a,
        Method::POST,
        format!("{base}/inventory/{vid}/movements"),
        &mgr,
        Some(json!({"kind": "stock_in", "quantity": 1, "reason": "x"})),
    )
    .await;
    let aid = ar.json()["data"]["approval_request_id"]
        .as_str()
        .unwrap()
        .to_owned();
    call(
        &a,
        Method::POST,
        format!("{base}/approvals/{aid}/approve"),
        &o,
        Some(json!({"step": 0})),
    )
    .await;
    let ar = call(
        &a,
        Method::POST,
        format!("{base}/inventory/{vid}/movements"),
        &mgr,
        Some(json!({"kind": "stock_in", "quantity": 2, "reason": "y"})),
    )
    .await;
    let aid = ar.json()["data"]["approval_request_id"]
        .as_str()
        .unwrap()
        .to_owned();
    call(
        &a,
        Method::POST,
        format!("{base}/approvals/{aid}/reject"),
        &o,
        Some(json!({"step": 0, "reason": "Tidak perlu"})),
    )
    .await;

    // AI provider/config/key (P5)
    let p = call(
        &a,
        Method::POST,
        format!("{base}/ai/providers"),
        &o,
        Some(json!({"name": "BYOK", "source": "byok", "provider_type": "openai",
            "base_url": "https://api.openai.com/v1", "model": "gpt-4o-mini", "api_key": "sk-audit-SECRET-123456"})),
    )
    .await;
    let pid = p.json()["data"]["id"].as_str().unwrap().to_owned();
    call(
        &a,
        Method::PATCH,
        format!("{base}/ai/providers/{pid}"),
        &o,
        Some(json!({"api_key": "sk-audit-ROTATED-654321"})),
    )
    .await;
    call(
        &a,
        Method::PUT,
        format!("{base}/ai/config"),
        &o,
        Some(json!({"enabled": false, "mode": "hybrid", "primary_provider_id": pid, "fallback_provider_id": null,
            "temperature": 0.3, "max_output_tokens": 512, "timeout_ms": 20000, "system_prompt": "", "welcome_message": "",
            "fallback_message": "Kami teruskan.", "allowed_topics": [], "restricted_topics": [], "handover_policy": {},
            "operating_schedule": {}, "monthly_token_budget": null})),
    )
    .await;

    // --- 10 platform admin: tagihan langganan → bukti dilihat & disetujui super admin (sesi MFA)
    let inv = call(
        &a,
        Method::POST,
        format!("{base}/subscription/invoices"),
        &o,
        Some(json!({"plan_code": "pro"})),
    )
    .await
    .json()["data"]["id"]
        .as_str()
        .unwrap()
        .to_owned();
    let r = send(
        &a.router,
        Method::POST,
        &format!("{base}/subscription/invoices/{inv}/proof"),
        Some(&o),
        None,
        Some("application/octet-stream"),
        png(),
    )
    .await;
    assert_eq!(r.status, 200, "{}", String::from_utf8_lossy(&r.bytes));
    let admin = platform_admin(&a.owner, true).await;
    assert_eq!(
        send(
            &a.router,
            Method::GET,
            &format!("/api/v1/platform/invoices/{inv}/proof"),
            Some(&admin),
            None,
            None,
            vec![]
        )
        .await
        .status,
        200
    );
    assert_eq!(
        call(
            &a,
            Method::POST,
            format!("/api/v1/platform/invoices/{inv}/approve"),
            &admin,
            Some(json!({}))
        )
        .await
        .status,
        200
    );

    // --- cakupan
    let actions: Vec<String> = sqlx::query_scalar("SELECT DISTINCT action FROM audit_logs WHERE tenant_id = $1 OR tenant_id IS NULL OR actor_user_id = $2")
        .bind(t).bind(o.user).fetch_all(&a.owner).await.unwrap();
    let mut covered = 0;
    for (category, required, later) in PRD_CATEGORIES {
        if let Some(phase) = later
            && required.is_empty()
        {
            println!("kategori '{category}' menunggu {phase}");
            continue;
        }
        for act in required {
            assert!(
                actions.iter().any(|x| x == act),
                "kategori '{category}': aksi {act} tidak diaudit. Ada: {actions:?}"
            );
        }
        covered += 1;
    }
    assert_eq!(covered, 10, "10 dari 10 kategori PRD §6.9 aktif");
    assert!(actions.len() >= 25, "{} jenis event", actions.len());

    // API: hanya audit.read; filter awalan; tenant lain tidak terlihat
    let r = call(
        &a,
        Method::GET,
        format!("{base}/audit-logs?action=order.&limit=100"),
        &o,
        None,
    )
    .await;
    let got: Vec<String> = r.json()["data"]["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|x| x["action"].as_str().unwrap().to_owned())
        .collect();
    assert!(
        !got.is_empty() && got.iter().all(|x| x.starts_with("order.")),
        "{got:?}"
    );
    assert_eq!(
        json(
            &a.router,
            Method::GET,
            &format!("{base}/audit-logs"),
            Some(&mgr),
            None
        )
        .await
        .status,
        403,
        "Manager tanpa audit.read"
    );
    let r = call(
        &a,
        Method::GET,
        format!("{base}/audit-logs?resource_type=order&resource_id={o1}"),
        &o,
        None,
    )
    .await;
    assert!(
        r.json()["data"]["total"].as_i64().unwrap() >= 4,
        "riwayat satu pesanan: dibuat, klaim, konfirmasi, disetujui, refund"
    );
    assert_eq!(
        json(
            &a.router,
            Method::GET,
            &format!("{base}/audit-logs?from=bukan-tanggal"),
            Some(&o),
            None
        )
        .await
        .status,
        422
    );
    let tb = tenant(&pool, "toko-b").await;
    let ob = member(&pool, tb, "Owner").await;
    let rb = call(
        &a,
        Method::GET,
        format!("/api/v1/tenants/{tb}/audit-logs"),
        &ob,
        None,
    )
    .await;
    assert_eq!(rb.json()["data"]["total"], 0);

    // tanpa rahasia: password, token, isi email lengkap pembeli tidak pernah ada di summary
    let all: Vec<Value> = sqlx::query_scalar("SELECT summary FROM audit_logs")
        .fetch_all(&a.owner)
        .await
        .unwrap();
    let dump = serde_json::to_string(&all).unwrap();
    for secret in [
        "rahasia-owner-1",
        "salah-sekali-1",
        &tok1,
        "sari@x.id",
        "baru@x.id",
        &o.csrf,
        "sk-audit-SECRET-123456",
        "sk-audit-ROTATED-654321",
    ] {
        assert!(!dump.contains(secret), "rahasia bocor ke audit: {secret}");
    }
    // append-only bagi role aplikasi
    let app_db = app_pool(&pool).await;
    for sql in [
        "UPDATE audit_logs SET action = 'x.y'",
        "DELETE FROM audit_logs",
    ] {
        let mut tx =
            umkm_database::begin_tenant_tx(&app_db, umkm_shared::ids::TenantId::from_uuid(t))
                .await
                .unwrap();
        assert!(
            sqlx::query(sqlx::AssertSqlSafe(sql))
                .execute(&mut *tx)
                .await
                .is_err(),
            "{sql}"
        );
    }
    let logout: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM audit_logs WHERE action = 'auth.logout' AND actor_user_id = $1",
    )
    .bind(o.user)
    .fetch_one(&a.owner)
    .await
    .unwrap();
    assert_eq!(logout, 1);
}
