//! P4-6: realtime TenantGuard penuh lintas instance (PRD.md §7.5, §6.3). Dua "instance" API
//! (hub terpisah) disambung lewat Redis pub/sub; worker mempublikasikan notifikasi.

mod common;

use std::{sync::Arc, time::Duration};

use axum::{
    Router,
    body::Body,
    http::{Method, Request, header},
};
use common::*;
use http_body_util::BodyExt as _;
use serde_json::json;
use sqlx::PgPool;
use tower::ServiceExt as _;
use umkm_api::{AppState, build_router, realtime::spawn_bridge};
use umkm_database::MIGRATOR;
use umkm_domain::auth::AuthPolicy;
use umkm_infrastructure::{
    metrics::Metrics,
    outbox::{CapturingEmailSender, WorkerConfig, process_once_with},
    realtime::{RtPublisher, channel},
    redis::{KeyNamespace, RedisClient},
};
use umkm_shared::config::SecretString;
use uuid::Uuid;

struct Cluster {
    a: TestApp,
    i1: Router,
    i2: Router,
    publisher: RtPublisher,
    client: RedisClient,
    ns: KeyNamespace,
}

fn redis() -> RedisClient {
    RedisClient::open(&SecretString::new(
        std::env::var("REDIS_URL").expect("REDIS_URL"),
    ))
    .unwrap()
}

/// Dua instance API dengan hub masing-masing, keduanya mendengar kanal Redis yang sama.
async fn cluster(pool: &PgPool) -> Cluster {
    let a = app(pool).await;
    let ns = KeyNamespace::new(&format!("umkmrt{}", Uuid::new_v4().simple())).unwrap();
    let client = redis();
    let publisher = RtPublisher::new(&client, &ns).unwrap();
    let mk = || {
        let st: AppState = AppState {
            realtime: Default::default(),
            ..a.state.clone()
        }
        .with_rt_publisher(publisher.clone());
        spawn_bridge(st.realtime.clone(), Arc::new(redis()), ns.clone());
        build_router(st, LIMITS)
    };
    let (i1, i2) = (mk(), mk());
    tokio::time::sleep(Duration::from_millis(300)).await; // langganan siap
    Cluster {
        a,
        i1,
        i2,
        publisher,
        client,
        ns,
    }
}

async fn subscribe(r: &Router, who: &Who, t: Uuid) -> (u16, Body) {
    let res = r
        .clone()
        .oneshot(
            Request::get(format!("/api/v1/tenants/{t}/events"))
                .header(header::COOKIE, &who.cookie)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    (res.status().as_u16(), res.into_body())
}

/// Frame data berikutnya (lewati keep-alive). `None` = tidak ada dalam batas waktu.
async fn next_event(body: &mut Body, wait: Duration) -> Option<String> {
    let deadline = tokio::time::Instant::now() + wait;
    loop {
        let left = deadline.checked_duration_since(tokio::time::Instant::now())?;
        match tokio::time::timeout(left, body.frame()).await {
            Err(_) | Ok(None) => return None,
            Ok(Some(f)) => {
                let s = f
                    .ok()?
                    .into_data()
                    .ok()
                    .map(|d| String::from_utf8_lossy(&d).into_owned())
                    .unwrap_or_default();
                if s.contains("event:") {
                    return Some(s);
                }
            }
        }
    }
}

async fn patch_name(r: &Router, t: Uuid, who: &Who, name: &str) -> u16 {
    let res = r
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::PATCH)
                .uri(format!("/api/v1/tenants/{t}"))
                .header(header::COOKIE, &who.cookie)
                .header("x-csrf-token", &who.csrf)
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(json!({ "name": name }).to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    res.status().as_u16()
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn events_fan_out_across_instances_only_within_tenant(pool: PgPool) {
    let c = cluster(&pool).await;
    let (ta, tb) = (tenant(&pool, "toko-a").await, tenant(&pool, "toko-b").await);
    let (oa, ob) = (
        member(&pool, ta, "Owner").await,
        member(&pool, tb, "Owner").await,
    );
    let (s, mut on_i2) = subscribe(&c.i2, &oa, ta).await;
    assert_eq!(s, 200);
    let (_, mut b_on_i2) = subscribe(&c.i2, &ob, tb).await;
    let (_, mut a_on_i1) = subscribe(&c.i1, &oa, ta).await;
    // perubahan di instance 1 → diterima pelanggan di instance 2 (dan 1), tidak di tenant B
    assert_eq!(patch_name(&c.i1, ta, &oa, "Toko A Baru").await, 200);
    let got = next_event(&mut on_i2, Duration::from_secs(5))
        .await
        .expect("instance 2 menerima");
    assert!(
        got.contains("event: tenant.updated") && got.contains("Toko A Baru"),
        "{got}"
    );
    assert!(
        next_event(&mut a_on_i1, Duration::from_secs(5))
            .await
            .is_some(),
        "instance asal juga menerima (sekali, lewat bridge)"
    );
    assert!(
        next_event(&mut a_on_i1, Duration::from_millis(500))
            .await
            .is_none(),
        "tidak ganda di instance asal"
    );
    assert!(
        next_event(&mut b_on_i2, Duration::from_millis(800))
            .await
            .is_none(),
        "tenant B tidak menerima"
    );
    // B tidak bisa berlangganan stream A di instance mana pun
    assert_eq!(subscribe(&c.i1, &ob, ta).await.0, 404);
    let _ = &c.a;
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn worker_notifications_reach_only_recipient_and_permitted_members(pool: PgPool) {
    let c = cluster(&pool).await;
    let t = tenant(&pool, "toko-a").await;
    let o = member(&pool, t, "Owner").await;
    let agent = member(&pool, t, "Support Agent").await;
    let agent2 = member(&pool, t, "Support Agent").await;
    let cat = member(&pool, t, "Catalog Manager").await;
    let (_, mut s_agent) = subscribe(&c.i1, &agent, t).await;
    let (_, mut s_agent2) = subscribe(&c.i2, &agent2, t).await;
    let (_, mut s_cat) = subscribe(&c.i2, &cat, t).await;
    let id = json(
        &c.a.router,
        Method::POST,
        &format!("/api/v1/tenants/{t}/tickets"),
        Some(&o),
        Some(json!({"subject": "Tolong", "assignee_user_id": agent.user})),
    )
    .await
    .json()["data"]["id"]
        .as_str()
        .unwrap()
        .to_owned();
    let cfg = WorkerConfig {
        worker_id: "w".into(),
        batch: 50,
        lease: Duration::from_secs(30),
        max_attempts: 3,
        base_backoff: Duration::from_secs(1),
        verify_url_base: "https://app.ryasai.my.id/verify-email".into(),
        policy: AuthPolicy::default(),
    };
    process_once_with(
        &c.a.state.db.clone().unwrap(),
        &CapturingEmailSender::default(),
        &cfg,
        &Metrics::new(),
        Some(&c.publisher),
    )
    .await
    .unwrap();

    // assignee: notifikasi pribadi "ditugaskan"
    let mut seen = String::new();
    while let Some(f) = next_event(&mut s_agent, Duration::from_secs(3)).await {
        seen.push_str(&f);
    }
    assert!(
        seen.contains("event: notification.created")
            && seen.contains("Ticket ditugaskan kepada Anda"),
        "{seen}"
    );
    // agen lain: siaran ticket.* (punya tickets.read) tetapi TIDAK notifikasi pribadi agen pertama
    let mut seen2 = String::new();
    while let Some(f) = next_event(&mut s_agent2, Duration::from_secs(2)).await {
        seen2.push_str(&f);
    }
    assert!(seen2.contains("event: ticket.assigned"), "{seen2}");
    assert!(
        !seen2.contains("Ticket ditugaskan kepada Anda"),
        "notifikasi pribadi bocor: {seen2}"
    );
    // Catalog Manager (tanpa tickets.read): tidak menerima apa pun tentang ticket
    assert!(
        next_event(&mut s_cat, Duration::from_millis(800))
            .await
            .is_none()
    );
    let _ = id;
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn malformed_bus_messages_are_dropped_and_permission_rechecked(pool: PgPool) {
    let c = cluster(&pool).await;
    let t = tenant(&pool, "toko-a").await;
    let o = member(&pool, t, "Owner").await;
    let m = member(&pool, t, "Manager").await;
    let (_, mut s) = subscribe(&c.i1, &m, t).await;
    let mut conn = c.client.connection_manager().await.unwrap();
    let ch = channel(&c.ns);
    for junk in [
        "bukan json".to_owned(),
        json!({"tenant_id": t, "kind": "Bad Kind", "data": {}, "user_id": null, "permission": null}).to_string(),
        json!({"tenant_id": t, "kind": "x.y", "data": {}, "user_id": null, "permission": "root.all"}).to_string(),
        json!({"tenant_id": t, "kind": "x.y", "data": {}, "extra": 1, "user_id": null, "permission": null}).to_string(),
    ] {
        let _: i64 = redis::cmd("PUBLISH").arg(&ch).arg(junk).query_async(&mut conn).await.unwrap();
    }
    assert!(
        next_event(&mut s, Duration::from_millis(800))
            .await
            .is_none(),
        "pesan rusak dibuang"
    );
    // event ber-permission orders.read: Manager menerima; setelah diturunkan ke Catalog Manager tidak lagi
    let ev = |n: i64| {
        json!({"tenant_id": t, "kind": "order.created", "data": {"n": n}, "user_id": null, "permission": "orders.read"}).to_string()
    };
    let _: i64 = redis::cmd("PUBLISH")
        .arg(&ch)
        .arg(ev(1))
        .query_async(&mut conn)
        .await
        .unwrap();
    assert!(
        next_event(&mut s, Duration::from_secs(5))
            .await
            .unwrap()
            .contains("\"n\":1")
    );
    let cat_role: Uuid = sqlx::query_scalar(
        "SELECT id FROM roles WHERE tenant_id IS NULL AND name = 'Catalog Manager'",
    )
    .fetch_one(&c.a.owner)
    .await
    .unwrap();
    sqlx::query("UPDATE tenant_memberships SET role_id = $1 WHERE user_id = $2")
        .bind(cat_role)
        .bind(m.user)
        .execute(&c.a.owner)
        .await
        .unwrap();
    let _: i64 = redis::cmd("PUBLISH")
        .arg(&ch)
        .arg(ev(2))
        .query_async(&mut conn)
        .await
        .unwrap();
    assert!(
        next_event(&mut s, Duration::from_millis(1500))
            .await
            .is_none(),
        "hak dicabut → tidak diteruskan"
    );
    // event tenant lain dengan tenant_id A palsu tidak mungkin dibuat klien: hanya server yang PUBLISH
    let _ = o;
}
