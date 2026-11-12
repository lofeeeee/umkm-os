//! P4-4: notifikasi (AC-NTF-001..003): 9 event, tepat satu per penerima, idempoten saat worker
//! mengulang, isolasi tenant, API baca/tandai.

mod common;

use std::time::Duration;

use axum::http::{Method, header};
use common::*;
use serde_json::{Value, json};
use sqlx::PgPool;
use umkm_application::notify::OutboxEvent;
use umkm_database::MIGRATOR;
use umkm_domain::auth::AuthPolicy;
use umkm_infrastructure::{
    metrics::Metrics,
    notification_repo,
    outbox::{CapturingEmailSender, WorkerConfig, enqueue, process_once},
};
use uuid::Uuid;

const HOST: &str = "toko-a.ryasai.my.id";

fn cfg() -> WorkerConfig {
    WorkerConfig {
        worker_id: "w".into(),
        batch: 50,
        lease: Duration::from_secs(30),
        max_attempts: 3,
        base_backoff: Duration::from_secs(1),
        verify_url_base: "https://app.ryasai.my.id/verify-email".into(),
        policy: AuthPolicy::default(),
    }
}

async fn work(a: &TestApp, sender: &CapturingEmailSender) {
    let db = a.state.db.clone().unwrap();
    for _ in 0..5 {
        if process_once(&db, sender, &cfg(), &Metrics::new())
            .await
            .unwrap()
            .claimed
            == 0
        {
            break;
        }
    }
    let dead: i64 =
        sqlx::query_scalar("SELECT count(*) FROM notification_outbox WHERE status <> 'sent'")
            .fetch_one(&a.owner)
            .await
            .unwrap();
    assert_eq!(dead, 0, "semua event terproses");
}

async fn call(a: &TestApp, m: Method, url: String, who: &Who, body: Option<Value>) -> Res {
    json(&a.router, m, &url, Some(who), body).await
}

async fn count(a: &TestApp, t: Uuid, user: Uuid, kind: &str) -> i64 {
    sqlx::query_scalar("SELECT count(*) FROM notifications WHERE tenant_id = $1 AND user_id = $2 AND event_type = $3")
        .bind(t).bind(user).bind(kind).fetch_one(&a.owner).await.unwrap()
}

async fn checkout(a: &TestApp, vid: &str) -> String {
    use http_body_util::BodyExt as _;
    use tower::ServiceExt as _;
    let body = json!({"items": [{"variant_id": vid, "quantity": 1}], "fulfillment": "pickup", "name": "Sari", "email": "sari@x.id", "accept_privacy": true, "accept_terms": true});
    let req = axum::http::Request::builder()
        .method(Method::POST)
        .uri("/api/v1/public/checkout")
        .header(header::HOST, HOST)
        .header(header::CONTENT_TYPE, "application/json")
        .header(
            "idempotency-key",
            format!("key-{}", Uuid::new_v4().simple()),
        )
        .body(axum::body::Body::from(body.to_string()))
        .unwrap();
    let res = a.router.clone().oneshot(req).await.unwrap();
    let v: Value =
        serde_json::from_slice(&res.into_body().collect().await.unwrap().to_bytes()).unwrap();
    v["data"]["access_token"].as_str().unwrap().to_owned()
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn nine_events_one_notification_per_recipient(pool: PgPool) {
    // AC-NTF-002, AC-NTF-003
    let a = app(&pool).await;
    let t = tenant(&pool, "toko-a").await;
    let o = member(&pool, t, "Owner").await;
    let mgr = member(&pool, t, "Manager").await;
    let agent = member(&pool, t, "Support Agent").await;
    let viewer = member(&pool, t, "Viewer").await;
    let base = format!("/api/v1/tenants/{t}");
    sqlx::query("UPDATE business_profiles SET pickup_available = true WHERE tenant_id = $1")
        .bind(t)
        .execute(&a.owner)
        .await
        .unwrap();
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
        Some(json!({"kind": "stock_in", "quantity": 6, "reason": "awal"})),
    )
    .await;
    call(
        &a,
        Method::PUT,
        format!("{base}/inventory/{vid}/settings"),
        &o,
        Some(json!({"min_stock": 5})),
    )
    .await;
    let q = upload(&a.router, t, &o, png()).await.json()["data"]["id"]
        .as_str()
        .unwrap()
        .to_owned();
    call(&a, Method::PUT, format!("{base}/commerce/settings"), &o, Some(json!({"qris": format!("asset:{q}"), "qris_receiver_name": "T", "checkout_enabled": true, "order_approval_required": true}))).await;

    // 1 order baru, 2 order disetujui (+ email customer), 7 stok menipis (reservasi 6→5 → tidak, konfirmasi → ya)
    checkout(&a, &vid).await;
    checkout(&a, &vid).await;
    let oid: String = sqlx::query_scalar("SELECT id::text FROM orders ORDER BY created_at LIMIT 1")
        .fetch_one(&a.owner)
        .await
        .unwrap();
    call(
        &a,
        Method::POST,
        format!("{base}/orders/{oid}/payment/confirm"),
        &mgr,
        Some(json!({"amount_received": "15000"})),
    )
    .await;
    call(
        &a,
        Method::POST,
        format!("{base}/orders/{oid}/approve"),
        &o,
        Some(json!({})),
    )
    .await;
    // 3 ticket baru, 4 ditugaskan, 5 balasan pelanggan (lewat repo/endpoint)
    let tk = call(
        &a,
        Method::POST,
        format!("{base}/tickets"),
        &o,
        Some(json!({"subject": "Tanya stok"})),
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
        Some(json!({"assignee_user_id": agent.user})),
    )
    .await;
    // 6 permintaan approval
    call(
        &a,
        Method::PUT,
        format!("{base}/approval-policies/stock_adjustment"),
        &o,
        Some(json!({"enabled": true, "steps": [{}]})),
    )
    .await;
    call(
        &a,
        Method::POST,
        format!("{base}/inventory/{vid}/movements"),
        &mgr,
        Some(json!({"kind": "stock_in", "quantity": 1, "reason": "x"})),
    )
    .await;
    // 8 AI handover, 9 status langganan (pemicu di P6/P7; aturan penerima diuji di sini)
    let db = a.state.db.clone().unwrap();
    for (kind, key) in [
        (notification_repo::AI_HANDOVER, "h1"),
        (notification_repo::SUBSCRIPTION_STATUS, "s1"),
    ] {
        enqueue(
            &db,
            &OutboxEvent {
                tenant_id: Some(t),
                event_type: kind,
                payload: json!({"status": "aktif"}),
                idempotency_key: key.into(),
            },
        )
        .await
        .unwrap();
    }
    let sender = CapturingEmailSender::default();
    work(&a, &sender).await;

    let (ow, mg, ag, vw) = (o.user, mgr.user, agent.user, viewer.user);
    for (kind, user, n) in [
        ("order.created", ow, 2),
        ("order.created", mg, 2),
        ("order.created", ag, 0),
        ("order.created", vw, 0),
        ("order.status_changed", ow, 1),
        ("order.status_changed", mg, 1),
        ("order.status_changed", vw, 0),
        ("payment.claimed", ow, 0),
        ("ticket.created", ow, 1),
        ("ticket.created", mg, 1),
        ("ticket.created", ag, 0),
        ("ticket.assigned", ag, 1),
        ("ticket.assigned", ow, 0),
        ("approval.requested", ow, 1),
        ("approval.requested", mg, 0),
        ("inventory.low_stock", ow, 1),
        ("inventory.low_stock", mg, 1),
        ("inventory.low_stock", vw, 0),
        ("ai.handover", ag, 1),
        ("ai.handover", vw, 0),
        ("subscription.status", ow, 1),
        ("subscription.status", mg, 0),
    ] {
        assert_eq!(count(&a, t, user, kind).await, n, "{kind} untuk {user}");
    }
    // email customer: dibayar + disetujui, masing-masing sekali
    let mails: Vec<String> = sender
        .sent
        .lock()
        .unwrap()
        .iter()
        .filter(|m| m.template == "order_status")
        .map(|m| m.subject.clone())
        .collect();
    assert_eq!(mails.len(), 2, "{mails:?}");
    assert!(
        mails.iter().any(|s| s.ends_with("disetujui"))
            && mails.iter().any(|s| s.ends_with("dibayar"))
    );

    // AC-NTF-003: worker crash setelah kirim sebelum ack → pemrosesan ulang tidak menggandakan
    let before: i64 = sqlx::query_scalar("SELECT count(*) FROM notifications")
        .fetch_one(&a.owner)
        .await
        .unwrap();
    sqlx::query("UPDATE notification_outbox SET status = 'pending', processed_at = NULL, available_at = now()").execute(&a.owner).await.unwrap();
    let again = CapturingEmailSender::default();
    work(&a, &again).await;
    let after: i64 = sqlx::query_scalar("SELECT count(*) FROM notifications")
        .fetch_one(&a.owner)
        .await
        .unwrap();
    assert_eq!(before, after);
    assert_eq!(
        again
            .sent
            .lock()
            .unwrap()
            .iter()
            .filter(|m| m.template == "order_status")
            .count(),
        0,
        "email yang sudah terkirim tidak dikirim ulang"
    );
    // niat kirim tercatat tapi belum terkirim (crash sebelum kirim) → dikirim tepat sekali saat diulang
    sqlx::query("UPDATE notification_deliveries SET status = 'pending', sent_at = NULL")
        .execute(&a.owner)
        .await
        .unwrap();
    sqlx::query("UPDATE notification_outbox SET status = 'pending', processed_at = NULL, available_at = now() WHERE event_type = 'order.status_changed'").execute(&a.owner).await.unwrap();
    let retry = CapturingEmailSender::default();
    work(&a, &retry).await;
    assert_eq!(retry.sent.lock().unwrap().len(), 2);
    let sent: i64 =
        sqlx::query_scalar("SELECT count(*) FROM notification_deliveries WHERE status = 'sent'")
            .fetch_one(&a.owner)
            .await
            .unwrap();
    assert_eq!(sent, 2);
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn notifications_are_tenant_and_user_scoped(pool: PgPool) {
    // AC-NTF-001
    let a = app(&pool).await;
    let (ta, tb) = (tenant(&pool, "toko-a").await, tenant(&pool, "toko-b").await);
    let oa = member(&pool, ta, "Owner").await;
    let ob = member(&pool, tb, "Owner").await;
    // pengguna yang menjadi anggota KEDUA tenant
    let both = member(&pool, ta, "Manager").await;
    sqlx::query("INSERT INTO tenant_memberships (tenant_id, user_id, role_id) SELECT $1, $2, id FROM roles WHERE tenant_id IS NULL AND name = 'Manager'")
        .bind(tb).bind(both.user).execute(&a.owner).await.unwrap();
    call(
        &a,
        Method::POST,
        format!("/api/v1/tenants/{ta}/tickets"),
        &oa,
        Some(json!({"subject": "Rahasia toko A"})),
    )
    .await;
    work(&a, &CapturingEmailSender::default()).await;
    assert_eq!(count(&a, ta, both.user, "ticket.created").await, 1);
    assert_eq!(count(&a, tb, both.user, "ticket.created").await, 0);
    assert_eq!(count(&a, tb, ob.user, "ticket.created").await, 0);
    let rb = call(
        &a,
        Method::GET,
        format!("/api/v1/tenants/{tb}/notifications"),
        &both,
        None,
    )
    .await;
    assert_eq!(
        rb.json()["data"]["items"].as_array().unwrap().len(),
        0,
        "notifikasi toko A tidak muncul di toko B"
    );
    let ra = call(
        &a,
        Method::GET,
        format!("/api/v1/tenants/{ta}/notifications"),
        &both,
        None,
    )
    .await;
    assert_eq!(
        (
            ra.json()["data"]["unread"].clone(),
            ra.json()["data"]["items"][0]["title"].clone()
        ),
        (json!(1), json!("Ticket baru"))
    );
    let nid = ra.json()["data"]["items"][0]["id"]
        .as_str()
        .unwrap()
        .to_owned();
    // pengguna lain tidak bisa menandai notifikasi orang lain
    assert_eq!(
        call(
            &a,
            Method::POST,
            format!("/api/v1/tenants/{ta}/notifications/{nid}/read"),
            &oa,
            None
        )
        .await
        .status,
        404
    );
    assert_eq!(
        call(
            &a,
            Method::POST,
            format!("/api/v1/tenants/{tb}/notifications/{nid}/read"),
            &both,
            None
        )
        .await
        .status,
        404
    );
    assert_eq!(
        call(
            &a,
            Method::POST,
            format!("/api/v1/tenants/{ta}/notifications/{nid}/read"),
            &both,
            None
        )
        .await
        .status,
        204
    );
    assert_eq!(
        call(
            &a,
            Method::GET,
            format!("/api/v1/tenants/{ta}/notifications?unread=true"),
            &both,
            None
        )
        .await
        .json()["data"]["unread"],
        0
    );
    // event dengan tenant_id A tetapi rujukan milik B → tidak ada notifikasi (RLS)
    let tb_ticket = call(
        &a,
        Method::POST,
        format!("/api/v1/tenants/{tb}/tickets"),
        &ob,
        Some(json!({"subject": "B"})),
    )
    .await
    .json()["data"]["id"]
        .as_str()
        .unwrap()
        .to_owned();
    work(&a, &CapturingEmailSender::default()).await;
    let db = a.state.db.clone().unwrap();
    enqueue(
        &db,
        &OutboxEvent {
            tenant_id: Some(ta),
            event_type: "ticket.created",
            payload: json!({"ticket_id": tb_ticket}),
            idempotency_key: "cross".into(),
        },
    )
    .await
    .unwrap();
    let before: i64 = sqlx::query_scalar("SELECT count(*) FROM notifications WHERE tenant_id = $1")
        .bind(ta)
        .fetch_one(&a.owner)
        .await
        .unwrap();
    work(&a, &CapturingEmailSender::default()).await;
    let after: i64 = sqlx::query_scalar("SELECT count(*) FROM notifications WHERE tenant_id = $1")
        .bind(ta)
        .fetch_one(&a.owner)
        .await
        .unwrap();
    assert_eq!(before, after);
}
