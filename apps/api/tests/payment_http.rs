//! P3-7 / P3-8: pembayaran QRIS statis + verifikasi manual (AC-ORD-007), refund (AC-ORD-008),
//! approval order dengan separation-of-duties (AC-ORD-005), transisi manual (AC-ORD-004).

mod common;

use axum::http::{Method, header};
use common::*;
use serde_json::{Value, json};
use sqlx::PgPool;
use umkm_database::MIGRATOR;
use umkm_infrastructure::inventory_repo;
use umkm_shared::ids::TenantId;
use uuid::Uuid;

const HOST: &str = "toko-a.ryasai.my.id";

struct Shop {
    t: Uuid,
    owner: Who,
    variant: String,
}

async fn req(
    a: &TestApp,
    m: Method,
    host: &str,
    path: &str,
    headers: &[(&str, &str)],
    body: Vec<u8>,
    ct: &str,
) -> Res {
    use http_body_util::BodyExt as _;
    use tower::ServiceExt as _;
    let mut b = axum::http::Request::builder()
        .method(m)
        .uri(path)
        .header(header::HOST, host)
        .header(header::CONTENT_TYPE, ct);
    for (k, v) in headers {
        b = b.header(*k, *v);
    }
    let res = a
        .router
        .clone()
        .oneshot(b.body(axum::body::Body::from(body)).unwrap())
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

async fn shop(a: &TestApp, pool: &PgPool, slug: &str, settings: Value) -> Shop {
    let t = tenant(pool, slug).await;
    let o = member(pool, t, "Owner").await;
    sqlx::query("UPDATE business_profiles SET pickup_available = true, delivery_available = true WHERE tenant_id = $1")
        .bind(t)
        .execute(&a.owner)
        .await
        .unwrap();
    let p = staff(
        a,
        Method::POST,
        format!("/api/v1/tenants/{t}/products"),
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
    staff(
        a,
        Method::POST,
        format!("/api/v1/tenants/{t}/products/{pid}/status"),
        &o,
        Some(json!({"status": "active"})),
    )
    .await;
    staff(
        a,
        Method::POST,
        format!("/api/v1/tenants/{t}/inventory/{vid}/movements"),
        &o,
        Some(json!({"kind": "stock_in", "quantity": 10, "reason": "awal"})),
    )
    .await;
    let qris = upload(&a.router, t, &o, png()).await.json()["data"]["id"]
        .as_str()
        .unwrap()
        .to_owned();
    let mut body = json!({"qris": format!("asset:{qris}"), "qris_receiver_name": "Toko", "checkout_enabled": true});
    for (k, v) in settings.as_object().unwrap() {
        body[k] = v.clone();
    }
    let r = staff(
        a,
        Method::PUT,
        format!("/api/v1/tenants/{t}/commerce/settings"),
        &o,
        Some(body),
    )
    .await;
    assert_eq!(r.status, 200, "{}", r.json());
    Shop {
        t,
        owner: o,
        variant: vid,
    }
}

/// Checkout 2 item (Rp 30.000). Kembalikan (order_id, token).
async fn order(a: &TestApp, s: &Shop, key: &str) -> (String, String) {
    let body = json!({
        "items": [{"variant_id": s.variant, "quantity": 2}], "fulfillment": "pickup",
        "name": "Sari", "email": "sari@x.id", "accept_privacy": true, "accept_terms": true
    });
    let r = req(
        a,
        Method::POST,
        HOST,
        "/api/v1/public/checkout",
        &[("idempotency-key", key)],
        body.to_string().into_bytes(),
        "application/json",
    )
    .await;
    assert_eq!(r.status, 201, "{}", r.json());
    let token = r.json()["data"]["access_token"]
        .as_str()
        .unwrap()
        .to_owned();
    let code = r.json()["data"]["order"]["code"]
        .as_str()
        .unwrap()
        .to_owned();
    let id: Uuid = sqlx::query_scalar("SELECT id FROM orders WHERE code = $1")
        .bind(code)
        .fetch_one(&a.owner)
        .await
        .unwrap();
    (id.to_string(), token)
}

fn url(s: &Shop, id: &str, tail: &str) -> String {
    format!("/api/v1/tenants/{}/orders/{id}{tail}", s.t)
}

async fn confirm(a: &TestApp, s: &Shop, who: &Who, id: &str, amount: &str) -> Res {
    staff(
        a,
        Method::POST,
        url(s, id, "/payment/confirm"),
        who,
        Some(json!({"amount_received": amount})),
    )
    .await
}

async fn level(a: &TestApp, s: &Shop) -> (i64, i64) {
    let l = inventory_repo::level(
        &a.state.db.clone().unwrap(),
        TenantId::from_uuid(s.t),
        Uuid::parse_str(&s.variant).unwrap(),
    )
    .await
    .unwrap()
    .unwrap();
    (l.on_hand, l.reserved)
}

async fn audits(a: &TestApp, s: &Shop, action: &str) -> Vec<Value> {
    sqlx::query_scalar("SELECT summary FROM audit_logs WHERE tenant_id = $1 AND action = $2 ORDER BY created_at, id")
        .bind(s.t)
        .bind(action)
        .fetch_all(&a.owner)
        .await
        .unwrap()
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn buyer_claim_and_private_proof(pool: PgPool) {
    let a = app(&pool).await;
    let s = shop(&a, &pool, "toko-a", json!({})).await;
    let (id, token) = order(&a, &s, "key-000001").await;
    let (_, other) = order(&a, &s, "key-000002").await;
    let tok = [("x-order-token", token.as_str())];
    let r = req(
        &a,
        Method::POST,
        HOST,
        "/api/v1/public/orders/current/payment-claim",
        &tok,
        json!({"payer_reference": "a.n. Sari"})
            .to_string()
            .into_bytes(),
        "application/json",
    )
    .await;
    assert_eq!(r.status, 200, "{}", r.json());
    assert_eq!(r.json()["data"]["payment"]["status"], "claimed");
    assert_eq!(
        r.json()["data"]["status"],
        "AWAITING_PAYMENT",
        "klaim bukan pembayaran"
    );
    let extended: bool = sqlx::query_scalar(
        "SELECT expires_at > now() + interval '47 hours' FROM orders WHERE id = $1::uuid",
    )
    .bind(&id)
    .fetch_one(&a.owner)
    .await
    .unwrap();
    assert!(
        extended,
        "klaim memperpanjang batas agar staf sempat verifikasi"
    );

    // bukti: gambar diperiksa isi; bukan gambar ditolak
    let proof = |tokv: &str, body: Vec<u8>| {
        let a = &a;
        let t = tokv.to_owned();
        async move {
            req(
                a,
                Method::POST,
                HOST,
                "/api/v1/public/orders/current/payment-proof",
                &[("x-order-token", &t)],
                body,
                "image/png",
            )
            .await
        }
    };
    assert_eq!(
        proof(&token, b"<svg onload=alert(1)>".to_vec())
            .await
            .status,
        422
    );
    assert_eq!(proof(&token, png()).await.status, 204);
    assert_eq!(
        proof(&token, jpeg()).await.status,
        204,
        "mengganti bukti lama"
    );
    let n: i64 = sqlx::query_scalar("SELECT count(*) FROM payment_proofs")
        .fetch_one(&a.owner)
        .await
        .unwrap();
    assert_eq!(n, 1);
    assert_eq!(proof("salah", png()).await.status, 404);

    // staf: bukti tersedia lewat endpoint privat; tidak masuk pustaka media / aset publik
    let d = staff(&a, Method::GET, url(&s, &id, ""), &s.owner, None)
        .await
        .json()["data"]
        .clone();
    assert_eq!(d["payment"]["has_proof"], true);
    assert_eq!(d["payment"]["payer_reference"], "a.n. Sari");
    let r = staff(
        &a,
        Method::GET,
        url(&s, &id, "/payment/proof"),
        &s.owner,
        None,
    )
    .await;
    assert_eq!(r.status, 200);
    assert_eq!(r.headers["content-type"], "image/jpeg");
    assert_eq!(r.headers["x-content-type-options"], "nosniff");
    let media = staff(
        &a,
        Method::GET,
        format!("/api/v1/tenants/{}/uploads", s.t),
        &s.owner,
        None,
    )
    .await;
    assert_eq!(
        media.json()["data"].as_array().unwrap().len(),
        1,
        "hanya QRIS, bukan bukti pembeli"
    );
    let catalog = member(&pool, s.t, "Catalog Manager").await;
    assert_eq!(
        staff(
            &a,
            Method::GET,
            url(&s, &id, "/payment/proof"),
            &catalog,
            None
        )
        .await
        .status,
        403
    );

    // setelah dikonfirmasi pembeli tidak bisa klaim/unggah lagi
    assert_eq!(confirm(&a, &s, &s.owner, &id, "30000").await.status, 200);
    let r = req(
        &a,
        Method::POST,
        HOST,
        "/api/v1/public/orders/current/payment-claim",
        &tok,
        b"{}".to_vec(),
        "application/json",
    )
    .await;
    assert_eq!(r.status, 409);
    assert_eq!(proof(&token, png()).await.status, 409);
    let _ = other;
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn confirmation_is_processed_once(pool: PgPool) {
    // AC-ORD-007
    let a = app(&pool).await;
    let s = shop(&a, &pool, "toko-a", json!({})).await;
    let (id, token) = order(&a, &s, "key-000001").await;
    let support = member(&pool, s.t, "Support Agent").await;
    let viewer = member(&pool, s.t, "Viewer").await;
    let mgr = member(&pool, s.t, "Manager").await;
    assert_eq!(confirm(&a, &s, &support, &id, "30000").await.status, 403);
    assert_eq!(confirm(&a, &s, &viewer, &id, "30000").await.status, 403);
    let r = confirm(&a, &s, &mgr, &id, "25000").await;
    assert_eq!(
        r.json()["error"]["details"]["errors"][0]["code"],
        "required_for_amount_mismatch"
    );

    let rs =
        futures_util::future::join_all((0..6).map(|_| confirm(&a, &s, &mgr, &id, "30000"))).await;
    assert_eq!(
        rs.iter().filter(|r| r.status == 200).count(),
        1,
        "{:?}",
        rs.iter().map(|r| r.status).collect::<Vec<_>>()
    );
    assert_eq!(rs.iter().filter(|r| r.status == 409).count(), 5);
    assert_eq!(audits(&a, &s, "payment.confirmed").await.len(), 1);
    assert_eq!(
        level(&a, &s).await,
        (8, 0),
        "stok dipotong sekali (reservasi → penjualan)"
    );
    let sales: i64 =
        sqlx::query_scalar("SELECT count(*) FROM inventory_movements WHERE kind = 'sale'")
            .fetch_one(&a.owner)
            .await
            .unwrap();
    assert_eq!(sales, 1);
    let d = staff(&a, Method::GET, url(&s, &id, ""), &s.owner, None)
        .await
        .json()["data"]
        .clone();
    assert_eq!(
        d["status"], "PROCESSING",
        "tanpa approval: PAID → PROCESSING"
    );
    assert_eq!(d["payment"]["status"], "confirmed");
    assert_eq!(d["payment"]["decided_by"], mgr.user.to_string());
    let evs: Vec<_> = d["events"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| e["to_status"].as_str().unwrap().to_owned())
        .collect();
    assert_eq!(evs, ["AWAITING_PAYMENT", "PAID", "PROCESSING"]);
    let buyer = req(
        &a,
        Method::GET,
        HOST,
        "/api/v1/public/orders/current",
        &[("x-order-token", &token)],
        vec![],
        "application/json",
    )
    .await;
    assert_eq!(buyer.json()["data"]["status"], "PROCESSING");
    assert_eq!(
        buyer.json()["data"]["payment"]["qris_url"],
        Value::Null,
        "QRIS hanya saat menunggu bayar"
    );
    let notif: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM notification_outbox WHERE event_type = 'order.status_changed'",
    )
    .fetch_one(&a.owner)
    .await
    .unwrap();
    assert_eq!(notif, 2);

    // tenant lain tidak dapat mengonfirmasi
    let b = shop(&a, &pool, "toko-b", json!({})).await;
    let (id2, _) = order(&a, &s, "key-000002").await;
    assert_eq!(confirm(&a, &b, &b.owner, &id2, "30000").await.status, 404);
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn approval_enforces_separation_of_duties(pool: PgPool) {
    // AC-ORD-005
    let a = app(&pool).await;
    let s = shop(
        &a,
        &pool,
        "toko-a",
        json!({"order_approval_required": true}),
    )
    .await;
    let mgr = member(&pool, s.t, "Manager").await;
    let (id, _) = order(&a, &s, "key-000001").await;
    assert_eq!(
        confirm(&a, &s, &mgr, &id, "30000").await.json()["data"]["status"],
        "APPROVAL_PENDING"
    );
    let r = staff(
        &a,
        Method::POST,
        url(&s, &id, "/approve"),
        &mgr,
        Some(json!({})),
    )
    .await;
    assert_eq!(
        (r.status, r.json()["error"]["details"]["reason"].clone()),
        (403, json!("separation_of_duties"))
    );
    let r = staff(
        &a,
        Method::POST,
        url(&s, &id, "/approve"),
        &s.owner,
        Some(json!({"reason": "Stok aman"})),
    )
    .await;
    assert_eq!(r.json()["data"]["status"], "APPROVED");
    assert_eq!(
        staff(
            &a,
            Method::POST,
            url(&s, &id, "/approve"),
            &s.owner,
            Some(json!({}))
        )
        .await
        .status,
        409
    );
    // pemrosesan manual; pickup tidak bisa SHIPPED
    let st = |to: &'static str| {
        let (a, s, id) = (&a, &s, id.clone());
        async move {
            staff(
                a,
                Method::POST,
                url(s, &id, "/status"),
                &s.owner,
                Some(json!({"status": to})),
            )
            .await
        }
    };
    assert_eq!(st("PROCESSING").await.status, 200);
    assert_eq!(st("SHIPPED").await.status, 409);
    assert_eq!(st("READY_FOR_PICKUP").await.status, 200);
    assert_eq!(st("COMPLETED").await.json()["data"]["status"], "COMPLETED");
    assert_eq!(
        st("PAID").await.status,
        422,
        "status hanya lewat aliran khusus"
    );
    let a1 = audits(&a, &s, "order.approved").await;
    assert_eq!(a1.len(), 1);

    // tolak: alasan wajib; stok kembali; lanjut refund penuh → REFUNDED
    let (id2, _) = order(&a, &s, "key-000002").await;
    confirm(&a, &s, &mgr, &id2, "30000").await;
    let before = level(&a, &s).await;
    assert_eq!(
        staff(
            &a,
            Method::POST,
            url(&s, &id2, "/reject"),
            &s.owner,
            Some(json!({}))
        )
        .await
        .status,
        422
    );
    let viewer = member(&pool, s.t, "Viewer").await;
    assert_eq!(
        staff(
            &a,
            Method::POST,
            url(&s, &id2, "/reject"),
            &viewer,
            Some(json!({"reason": "x"}))
        )
        .await
        .status,
        403
    );
    let r = staff(
        &a,
        Method::POST,
        url(&s, &id2, "/reject"),
        &s.owner,
        Some(json!({"reason": "Bahan habis"})),
    )
    .await;
    assert_eq!(r.json()["data"]["status"], "REJECTED");
    assert_eq!(level(&a, &s).await.0, before.0 + 2, "stok dikembalikan");
    let rej = audits(&a, &s, "order.rejected").await;
    assert_eq!(rej[0]["reason"], "Bahan habis");
    let r = staff(
        &a,
        Method::POST,
        url(&s, &id2, "/refunds"),
        &s.owner,
        Some(json!({"amount": "30000", "reason": "Order ditolak", "restock": true})),
    )
    .await;
    assert_eq!(r.json()["data"]["status"], "REFUNDED");
    assert_eq!(
        level(&a, &s).await.0,
        before.0 + 2,
        "tidak restock dua kali"
    );

    // SoD dimatikan: verifikator boleh menyetujui
    staff(&a, Method::PUT, format!("/api/v1/tenants/{}/commerce/settings", s.t), &s.owner, Some(json!({
        "qris": staff(&a, Method::GET, format!("/api/v1/tenants/{}/commerce/settings", s.t), &s.owner, None).await.json()["data"]["qris"],
        "qris_receiver_name": "Toko", "checkout_enabled": true, "order_approval_required": true, "separation_of_duties": false
    }))).await;
    let (id3, _) = order(&a, &s, "key-000003").await;
    confirm(&a, &s, &mgr, &id3, "30000").await;
    assert_eq!(
        staff(
            &a,
            Method::POST,
            url(&s, &id3, "/approve"),
            &mgr,
            Some(json!({}))
        )
        .await
        .status,
        200
    );
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn refunds_are_bounded_and_audited(pool: PgPool) {
    // AC-ORD-008
    let a = app(&pool).await;
    let s = shop(&a, &pool, "toko-a", json!({})).await;
    let mgr = member(&pool, s.t, "Manager").await;
    let (id, _) = order(&a, &s, "key-000001").await;
    let refund = |who: &'static str, amount: &str, restock: bool| {
        let (a, s, id) = (&a, &s, id.clone());
        let w = if who == "owner" { &s.owner } else { &mgr };
        let body = json!({"amount": amount, "reason": "Kopi tumpah", "restock": restock});
        async move { staff(a, Method::POST, url(s, &id, "/refunds"), w, Some(body)).await }
    };
    assert_eq!(
        refund("owner", "1000", false).await.status,
        409,
        "belum dibayar"
    );
    confirm(&a, &s, &mgr, &id, "30000").await;
    assert_eq!(
        refund("mgr", "1000", false).await.status,
        403,
        "Manager tanpa payments.refund"
    );
    let r = refund("owner", "10000", false).await;
    assert_eq!(
        (
            r.json()["data"]["status"].clone(),
            r.json()["data"]["payment"]["refunded"].clone()
        ),
        (json!("PROCESSING"), json!("10000.00"))
    );
    let r = refund("owner", "20000.01", false).await;
    assert_eq!(
        r.json()["error"]["details"]["errors"][0]["code"],
        "exceeds_refundable"
    );
    let r = refund("owner", "20000", true).await;
    assert_eq!(r.json()["data"]["status"], "REFUNDED");
    assert_eq!(level(&a, &s).await, (10, 0), "restock mengembalikan 2 item");
    let au = audits(&a, &s, "payment.refunded").await;
    assert_eq!(au.len(), 2);
    assert_eq!(
        (
            au[0]["amount"].clone(),
            au[0]["reason"].clone(),
            au[1]["full"].clone()
        ),
        (json!("10000.00"), json!("Kopi tumpah"), json!(true))
    );
    let actor: Uuid = sqlx::query_scalar(
        "SELECT actor_user_id FROM audit_logs WHERE action = 'payment.refunded' LIMIT 1",
    )
    .fetch_one(&a.owner)
    .await
    .unwrap();
    assert_eq!(actor, s.owner.user);
    assert_eq!(
        refund("owner", "1", false).await.status,
        409,
        "REFUNDED terminal"
    );
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn rejected_payment_and_cancellation_release_stock(pool: PgPool) {
    let a = app(&pool).await;
    let s = shop(&a, &pool, "toko-a", json!({})).await;
    let (id, token) = order(&a, &s, "key-000001").await;
    assert_eq!(level(&a, &s).await, (10, 2));
    let r = staff(
        &a,
        Method::POST,
        url(&s, &id, "/payment/reject"),
        &s.owner,
        Some(json!({})),
    )
    .await;
    assert_eq!(r.status, 422, "alasan wajib");
    let r = staff(
        &a,
        Method::POST,
        url(&s, &id, "/payment/reject"),
        &s.owner,
        Some(json!({"reason": "Dana tidak masuk"})),
    )
    .await;
    assert_eq!(r.json()["data"]["status"], "CANCELLED");
    assert_eq!(level(&a, &s).await, (10, 0));
    let buyer = req(
        &a,
        Method::GET,
        HOST,
        "/api/v1/public/orders/current",
        &[("x-order-token", &token)],
        vec![],
        "application/json",
    )
    .await;
    assert_eq!(
        buyer.json()["data"]["payment"]["rejection_reason"],
        "Dana tidak masuk"
    );
    assert_eq!(confirm(&a, &s, &s.owner, &id, "30000").await.status, 409);

    let (id2, _) = order(&a, &s, "key-000002").await;
    let viewer = member(&pool, s.t, "Viewer").await;
    let body = json!({"status": "CANCELLED", "reason": "Pembeli batal"});
    assert_eq!(
        staff(
            &a,
            Method::POST,
            url(&s, &id2, "/status"),
            &viewer,
            Some(body.clone())
        )
        .await
        .status,
        403
    );
    assert_eq!(
        staff(
            &a,
            Method::POST,
            url(&s, &id2, "/status"),
            &s.owner,
            Some(body)
        )
        .await
        .json()["data"]["status"],
        "CANCELLED"
    );
    assert_eq!(level(&a, &s).await, (10, 0));
    let (id3, _) = order(&a, &s, "key-000003").await;
    confirm(&a, &s, &s.owner, &id3, "30000").await;
    let r = staff(
        &a,
        Method::POST,
        url(&s, &id3, "/status"),
        &s.owner,
        Some(json!({"status": "CANCELLED"})),
    )
    .await;
    assert_eq!(
        r.status, 409,
        "order dibayar keluar lewat refund, bukan batal"
    );
}
