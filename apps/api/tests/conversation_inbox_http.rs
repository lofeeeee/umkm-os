//! P6-5: admin inbox (antrean, hitungan, assign), notifikasi AI handover lewat outbox, ticket dari
//! percakapan (FR-TKT-006, AC-TKT-006). Role aplikasi + RLS.

mod common;

use std::time::Duration;

use axum::http::Method;
use common::*;
use serde_json::{Value, json};
use sqlx::PgPool;
use umkm_database::MIGRATOR;
use umkm_domain::{auth::AuthPolicy, conversation::ConversationStatus};
use umkm_infrastructure::{
    conversation_repo::{self as repo, Actor, StartInput},
    metrics::Metrics,
    outbox::{CapturingEmailSender, WorkerConfig, process_once},
};
use umkm_shared::ids::TenantId;
use uuid::Uuid;

async fn start(pool: &PgPool, t: Uuid, initial: ConversationStatus) -> Uuid {
    repo::start(
        &app_pool(pool).await,
        TenantId::from_uuid(t),
        &StartInput {
            consent_version: umkm_domain::ai::consent::VERSION,
            customer_id: None,
            initial,
        },
    )
    .await
    .unwrap()
    .conversation_id
}

async fn to(pool: &PgPool, t: Uuid, c: Uuid, s: ConversationStatus, who: Actor) {
    repo::transition(&app_pool(pool).await, TenantId::from_uuid(t), c, s, who)
        .await
        .unwrap();
}

fn base(t: Uuid) -> String {
    format!("/api/v1/tenants/{t}/conversations")
}

async fn get(a: &TestApp, w: &Who, uri: &str) -> Value {
    let r = json(&a.router, Method::GET, uri, Some(w), None).await;
    assert_eq!(
        r.status,
        200,
        "{uri}: {}",
        String::from_utf8_lossy(&r.bytes)
    );
    r.json()["data"].clone()
}

async fn post(a: &TestApp, w: &Who, uri: &str, body: Value) -> Res {
    json(&a.router, Method::POST, uri, Some(w), Some(body)).await
}

fn ids(v: &Value) -> Vec<String> {
    v["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|x| x["id"].as_str().unwrap().to_owned())
        .collect()
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn inbox_queue_counts_and_assignment(pool: PgPool) {
    let a = app(&pool).await;
    let t = tenant(&pool, "toko-a").await;
    let tb = tenant(&pool, "toko-b").await;
    let lead = member(&pool, t, "Manager").await;
    let agent = member(&pool, t, "Support Agent").await;
    let catalog = member(&pool, t, "Catalog Manager").await;
    let viewer = member(&pool, t, "Viewer").await;
    let outsider = member(&pool, tb, "Support Agent").await;

    let ai = start(&pool, t, ConversationStatus::Ai).await;
    let w1 = start(&pool, t, ConversationStatus::Ai).await;
    to(
        &pool,
        t,
        w1,
        ConversationStatus::WaitingHuman,
        Actor::Customer,
    )
    .await;
    let w2 = start(&pool, t, ConversationStatus::WaitingHuman).await;
    // w1 menunggu paling lama.
    sqlx::query(
        "UPDATE conversations SET last_message_at = now() - interval '10 minutes' WHERE id = $1",
    )
    .bind(w1)
    .execute(&pool)
    .await
    .unwrap();
    start(&pool, tb, ConversationStatus::WaitingHuman).await; // toko lain tidak terhitung

    let all = get(&a, &lead, &base(t)).await;
    assert_eq!(
        all["counts"],
        json!({"ai": 1, "waiting_human": 2, "human": 0, "closed": 0, "mine": 0})
    );
    // Antrean menunggu staf FIFO.
    let q = get(&a, &lead, &format!("{}?status=WAITING_HUMAN", base(t))).await;
    assert_eq!(ids(&q), [w1.to_string(), w2.to_string()]);

    // Assign: hanya ke anggota aktif yang dapat membalas; Viewer tidak boleh menugaskan.
    let u = format!("{}/{w1}/assign", base(t));
    assert_eq!(
        post(&a, &viewer, &u, json!({"user_id": agent.user}))
            .await
            .status,
        403
    );
    for bad in [catalog.user, outsider.user, Uuid::new_v4()] {
        let r = post(&a, &lead, &u, json!({"user_id": bad})).await;
        assert_eq!(
            (
                r.status,
                r.json()["error"]["details"]["errors"][0]["path"].clone()
            ),
            (422, json!("user_id"))
        );
    }
    let r = post(&a, &lead, &u, json!({"user_id": agent.user})).await;
    assert_eq!(
        (r.status, r.json()["data"]["status"].clone()),
        (200, json!("HUMAN"))
    );
    let mine = get(&a, &agent, &format!("{}?mine=true", base(t))).await;
    assert_eq!(ids(&mine), [w1.to_string()]);
    assert_eq!(mine["counts"]["mine"], 1);
    assert_eq!(
        get(&a, &lead, &format!("{}?mine=true", base(t))).await["total"],
        0
    );
    // Penangan baru bisa membalas; pemberi tugas kini "staf lain".
    assert_eq!(
        post(
            &a,
            &agent,
            &format!("{}/{w1}/messages", base(t)),
            json!({"body": "Halo kak"})
        )
        .await
        .status,
        200
    );
    let r = post(
        &a,
        &lead,
        &format!("{}/{w1}/messages", base(t)),
        json!({"body": "x"}),
    )
    .await;
    assert_eq!(r.json()["error"]["details"]["reason"], "assigned_to_other");
    // Re-assign HUMAN → anggota lain diperbolehkan untuk pemegang takeover.
    assert_eq!(
        post(&a, &lead, &u, json!({"user_id": lead.user}))
            .await
            .status,
        200
    );
    let d = get(&a, &lead, &format!("{}/{w1}", base(t))).await;
    assert_eq!(d["assignee_user_id"], json!(lead.user));
    let assigned: Vec<&Value> = d["history"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|e| e["kind"] == "assigned")
        .collect();
    assert_eq!(assigned.len(), 2);
    // Ditutup → tidak bisa ditugaskan; toko lain → 404.
    post(&a, &lead, &format!("{}/{w1}/close", base(t)), json!({})).await;
    assert_eq!(
        post(&a, &lead, &u, json!({"user_id": agent.user}))
            .await
            .status,
        409
    );
    assert_eq!(
        post(
            &a,
            &outsider,
            &format!("{}/{w2}/assign", base(tb)),
            json!({"user_id": outsider.user})
        )
        .await
        .status,
        404
    );
    let _ = ai;
}

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

#[sqlx::test(migrator = "MIGRATOR")]
async fn handover_notifies_takeover_staff_once(pool: PgPool) {
    let a = app(&pool).await;
    let t = tenant(&pool, "toko-a").await;
    let tb = tenant(&pool, "toko-b").await;
    let agent = member(&pool, t, "Support Agent").await;
    let viewer = member(&pool, t, "Viewer").await;
    let other = member(&pool, tb, "Support Agent").await;

    let c = start(&pool, t, ConversationStatus::Ai).await;
    to(&pool, t, c, ConversationStatus::WaitingHuman, Actor::Ai).await;
    start(&pool, t, ConversationStatus::WaitingHuman).await; // Human Only: dari awal menunggu
    to(
        &pool,
        t,
        c,
        ConversationStatus::Human,
        Actor::Staff(agent.user),
    )
    .await; // bukan handover
    let n: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM notification_outbox WHERE event_type = 'ai.handover'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(
        n, 2,
        "satu event per handover, ditulis bersama perubahan status"
    );

    let sender = CapturingEmailSender::default();
    let db = a.state.db.clone().unwrap();
    for _ in 0..3 {
        process_once(&db, &sender, &cfg(), &Metrics::new())
            .await
            .unwrap();
    }
    let items = |v: Value| v["items"].as_array().cloned().unwrap_or_default();
    let mine = items(get(&a, &agent, &format!("/api/v1/tenants/{t}/notifications")).await);
    let handovers: Vec<&Value> = mine
        .iter()
        .filter(|x| x["title"] == "Percakapan perlu diambil alih")
        .collect();
    assert_eq!(handovers.len(), 2);
    assert!(
        handovers[0]["link"]
            .as_str()
            .unwrap()
            .starts_with("/conversations")
    );
    // Tanpa conversations.takeover (Viewer) dan tenant lain: tidak menerima.
    assert!(
        items(get(&a, &viewer, &format!("/api/v1/tenants/{t}/notifications")).await)
            .iter()
            .all(|x| x["title"] != "Percakapan perlu diambil alih")
    );
    assert!(
        items(get(&a, &other, &format!("/api/v1/tenants/{tb}/notifications")).await).is_empty()
    );
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn tickets_from_conversation_are_independent(pool: PgPool) {
    // FR-TKT-006 / AC-TKT-006
    let a = app(&pool).await;
    let t = tenant(&pool, "toko-a").await;
    let agent = member(&pool, t, "Support Agent").await;
    let viewer = member(&pool, t, "Viewer").await;
    let customer: Uuid = sqlx::query_scalar(
        "INSERT INTO customers (tenant_id, name, email) VALUES ($1, 'Sari', 's@x.id') RETURNING id",
    )
    .bind(t)
    .fetch_one(&pool)
    .await
    .unwrap();
    let c = repo::start(
        &app_pool(&pool).await,
        TenantId::from_uuid(t),
        &StartInput {
            consent_version: umkm_domain::ai::consent::VERSION,
            customer_id: Some(customer),
            initial: ConversationStatus::Ai,
        },
    )
    .await
    .unwrap()
    .conversation_id;
    let u = format!("{}/{c}/tickets", base(t));
    assert_eq!(
        post(&a, &viewer, &u, json!({"subject": "x"})).await.status,
        403,
        "Viewer tanpa tickets.reply"
    );
    assert_eq!(
        post(&a, &agent, &u, json!({"subject": "  "})).await.status,
        422
    );
    assert_eq!(
        post(
            &a,
            &agent,
            &u,
            json!({"subject": "x", "priority": "darurat"})
        )
        .await
        .status,
        422
    );
    let r1 = post(
        &a,
        &agent,
        &u,
        json!({"subject": "Paket rusak", "priority": "high", "description": "Foto menyusul"}),
    )
    .await;
    assert_eq!(r1.status, 201, "{}", String::from_utf8_lossy(&r1.bytes));
    let r2 = post(&a, &agent, &u, json!({"subject": "Minta faktur"})).await;
    assert_eq!(r2.status, 201);
    let t1 = r1.json()["data"]["id"].as_str().unwrap().to_owned();

    // Ticket bersumber chat, pelanggan & percakapan tertaut; satu percakapan → 2 ticket.
    let tk = get(&a, &agent, &format!("/api/v1/tenants/{t}/tickets/{t1}")).await;
    assert_eq!(
        (tk["source"].clone(), tk["priority"].clone()),
        (json!("chat"), json!("high"))
    );
    assert_eq!(tk["conversation_id"], json!(c));
    assert_eq!(
        (tk["status"].clone(), tk["assignee_user_id"].clone()),
        (json!("ASSIGNED"), json!(agent.user))
    );
    let d = get(&a, &agent, &format!("{}/{c}", base(t))).await;
    assert_eq!(d["tickets"].as_array().unwrap().len(), 2);
    assert_eq!(d["tickets"][0]["subject"], "Paket rusak");
    let n: i64 = sqlx::query_scalar("SELECT count(*) FROM tickets WHERE customer_id = $1")
        .bind(customer)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(n, 2);

    // AC-TKT-006: ticket ditutup → percakapan tetap terbuka; percakapan ditutup → ticket tetap.
    for s in ["RESOLVED", "CLOSED"] {
        let r = json(
            &a.router,
            Method::POST,
            &format!("/api/v1/tenants/{t}/tickets/{t1}/status"),
            Some(&agent),
            Some(json!({"status": s})),
        )
        .await;
        assert!(
            r.status == 200,
            "{s}: {}",
            String::from_utf8_lossy(&r.bytes)
        );
    }
    assert_eq!(
        get(&a, &agent, &format!("{}/{c}", base(t))).await["status"],
        "AI"
    );
    post(&a, &agent, &format!("{}/{c}/close", base(t)), json!({})).await;
    let t2 = r2.json()["data"]["id"].as_str().unwrap().to_owned();
    assert_eq!(
        get(&a, &agent, &format!("/api/v1/tenants/{t}/tickets/{t2}")).await["status"],
        "ASSIGNED"
    );
    // Percakapan toko lain → 404.
    let tb = tenant(&pool, "toko-b").await;
    let ob = member(&pool, tb, "Owner").await;
    assert_eq!(
        post(
            &a,
            &ob,
            &format!("{}/{c}/tickets", base(tb)),
            json!({"subject": "x"})
        )
        .await
        .status,
        404
    );
}
