//! P6-3: Conversation Viewer + masking (FR-CNV-002/003, AC-CNV-002/003). Role aplikasi + RLS.

mod common;

use axum::http::Method;
use common::*;
use serde_json::{Value, json};
use sqlx::PgPool;
use umkm_database::MIGRATOR;
use umkm_domain::conversation::{ConversationStatus, SenderType};
use umkm_infrastructure::conversation_repo::{self as repo, Actor, NewMessage, StartInput};
use umkm_shared::ids::TenantId;
use uuid::Uuid;

const CUSTOMER_TEXT: &str =
    "Halo, email saya budi.s@mail.co.id, WA 0812 3456 7890. Pesanan ORD-1234567?";

struct Fixture {
    t: Uuid,
    conv: Uuid,
    owner: Who,
    agent: Who,
    viewer: Who,
    catalog: Who,
    other: Who,
}

async fn fixture(pool: &PgPool) -> Fixture {
    let t = tenant(pool, "toko-a").await;
    let tb = tenant(pool, "toko-b").await;
    let owner = member(pool, t, "Owner").await;
    let agent = member(pool, t, "Support Agent").await;
    let viewer = member(pool, t, "Viewer").await;
    let catalog = member(pool, t, "Catalog Manager").await;
    let other = member(pool, tb, "Owner").await;
    let tid = TenantId::from_uuid(t);
    let app = app_pool(pool).await;

    let customer: Uuid = sqlx::query_scalar(
        "INSERT INTO customers (tenant_id, name, email, phone) VALUES ($1, 'Budi Santoso', 'budi@toko.id', '+6281234567890') RETURNING id",
    )
    .bind(t)
    .fetch_one(pool)
    .await
    .unwrap();
    let s = repo::start(
        &app,
        tid,
        &StartInput {
            consent_version: umkm_domain::ai::consent::VERSION,
            customer_id: Some(customer),
            initial: ConversationStatus::Ai,
        },
    )
    .await
    .unwrap();
    let conv = s.conversation_id;
    repo::append(
        &app,
        tid,
        conv,
        &NewMessage {
            sender: SenderType::Customer,
            session_id: Some(s.session_id),
            sender_user_id: None,
            body: CUSTOMER_TEXT,
            ai_run_id: None,
        },
    )
    .await
    .unwrap();
    // Run AI (provider platform, token, latensi) + tool call.
    let prov: Uuid = sqlx::query_scalar(
        "INSERT INTO ai_providers (tenant_id, name, source, provider_type, model, capabilities)
         VALUES ($1, 'AI Platform', 'platform', 'platform', 'm1', '{\"chat\":true}') RETURNING id",
    )
    .bind(t)
    .fetch_one(pool)
    .await
    .unwrap();
    let run: Uuid = sqlx::query_scalar(
        "INSERT INTO ai_runs (tenant_id, conversation_id, provider_id, source, model, status, decision,
             prompt_tokens, completion_tokens, latency_ms)
         VALUES ($1, $2, $3, 'platform', 'm1', 'ok', 'send_auto', 321, 45, 870) RETURNING id",
    )
    .bind(t)
    .bind(conv)
    .bind(prov)
    .fetch_one(pool)
    .await
    .unwrap();
    sqlx::query("INSERT INTO ai_tool_calls (tenant_id, ai_run_id, tool_name, result_status, args_summary) VALUES ($1, $2, 'get_order_status', 'ok', '{\"order_number\":\"ORD-1234567\"}')")
        .bind(t)
        .bind(run)
        .execute(pool)
        .await
        .unwrap();
    repo::append(
        &app,
        tid,
        conv,
        &NewMessage {
            sender: SenderType::Ai,
            session_id: None,
            sender_user_id: None,
            body: "Pesanan ORD-1234567 sedang dikemas.",
            ai_run_id: Some(run),
        },
    )
    .await
    .unwrap();
    // Run tanpa pesan (handover) + takeover staf + balasan staf.
    sqlx::query("INSERT INTO ai_runs (tenant_id, conversation_id, source, model, status, decision, error_code) VALUES ($1, $2, 'platform', 'm1', 'timeout', 'handover', 'timeout')")
        .bind(t)
        .bind(conv)
        .execute(pool)
        .await
        .unwrap();
    repo::transition(&app, tid, conv, ConversationStatus::WaitingHuman, Actor::Ai)
        .await
        .unwrap();
    repo::transition(
        &app,
        tid,
        conv,
        ConversationStatus::Human,
        Actor::Staff(agent.user),
    )
    .await
    .unwrap();
    repo::append(
        &app,
        tid,
        conv,
        &NewMessage {
            sender: SenderType::Admin,
            session_id: None,
            sender_user_id: Some(agent.user),
            body: "Saya bantu cek, kak.",
            ai_run_id: None,
        },
    )
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO approval_requests (tenant_id, entity_type, entity_id, action, summary, steps, requested_by, expires_at)
         VALUES ($1, 'ai_response', $2, 'ai_response.send', 'Balasan untuk 0812 3456 7890', '[{\"permission\":\"conversations.reply\"}]', $3, now() + interval '1 day')",
    )
    .bind(t)
    .bind(conv)
    .bind(owner.user)
    .execute(pool)
    .await
    .unwrap();
    Fixture {
        t,
        conv,
        owner,
        agent,
        viewer,
        catalog,
        other,
    }
}

async fn get(a: &TestApp, who: &Who, uri: &str) -> Res {
    json(&a.router, Method::GET, uri, Some(who), None).await
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn viewer_shows_every_field_for_support_agent(pool: PgPool) {
    // AC-CNV-002
    let a = app(&pool).await;
    let f = fixture(&pool).await;
    let r = get(
        &a,
        &f.agent,
        &format!("/api/v1/tenants/{}/conversations/{}", f.t, f.conv),
    )
    .await;
    assert_eq!(r.status, 200, "{}", String::from_utf8_lossy(&r.bytes));
    let d = &r.json()["data"];
    assert_eq!(d["masked"], false);
    assert_eq!(
        (d["status"].clone(), d["channel"].clone()),
        (json!("HUMAN"), json!("web"))
    );
    assert_eq!(d["consent_version"], umkm_domain::ai::consent::VERSION);
    // Customer context
    assert_eq!(d["customer"]["name"], "Budi Santoso");
    assert_eq!(d["customer"]["email"], "budi@toko.id");
    assert_eq!(d["customer"]["phone"], "+6281234567890");
    // Timeline 4 sender, isi utuh
    let tl = d["timeline"].as_array().unwrap();
    assert_eq!(
        tl.iter()
            .map(|m| m["sender"].as_str().unwrap())
            .collect::<Vec<_>>(),
        ["CUSTOMER", "AI", "ADMIN"]
    );
    assert_eq!(tl[0]["body"], CUSTOMER_TEXT);
    // Provider/model, token, latensi, tool summary
    let ai = &tl[1]["ai"];
    assert_eq!(
        (
            ai["provider_name"].clone(),
            ai["model"].clone(),
            ai["source"].clone()
        ),
        (json!("AI Platform"), json!("m1"), json!("platform"))
    );
    assert_eq!(
        (
            ai["prompt_tokens"].clone(),
            ai["completion_tokens"].clone(),
            ai["latency_ms"].clone()
        ),
        (json!(321), json!(45), json!(870))
    );
    assert_eq!(ai["tool_calls"][0]["name"], "get_order_status");
    assert_eq!(
        ai["tool_calls"][0]["args_summary"]["order_number"],
        "ORD-1234567"
    );
    assert_eq!(d["other_ai_runs"][0]["decision"], "handover");
    assert_eq!(d["other_ai_runs"][0]["error_code"], "timeout");
    // Assignment & handover history
    assert_eq!(d["assignee_user_id"], json!(f.agent.user));
    assert!(d["assignee_email"].as_str().unwrap().ends_with("@x.id"));
    assert_eq!(tl[2]["sender_user_id"], json!(f.agent.user));
    let kinds: Vec<&str> = d["history"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| e["kind"].as_str().unwrap())
        .collect();
    assert_eq!(kinds, ["created", "handover_requested", "taken_over"]);
    assert_eq!(d["history"][2]["actor_user_id"], json!(f.agent.user));
    // Approval state
    assert_eq!(d["approvals"][0]["status"], "pending");
    assert_eq!(d["approvals"][0]["summary"], "Balasan untuk 0812 3456 7890");

    // Owner juga melihat penuh; daftar memuat ringkasan.
    let l = get(
        &a,
        &f.owner,
        &format!("/api/v1/tenants/{}/conversations", f.t),
    )
    .await;
    assert_eq!(l.json()["data"]["masked"], false);
    assert_eq!(l.json()["data"]["total"], 1);
    let it = &l.json()["data"]["items"][0];
    assert_eq!(
        (it["message_count"].clone(), it["last_message"].clone()),
        (json!(3), json!("Saya bantu cek, kak."))
    );
    let l = get(
        &a,
        &f.owner,
        &format!("/api/v1/tenants/{}/conversations?status=AI", f.t),
    )
    .await;
    assert_eq!(l.json()["data"]["total"], 0);
    let l = get(
        &a,
        &f.owner,
        &format!("/api/v1/tenants/{}/conversations?status=open", f.t),
    )
    .await;
    assert_eq!(l.status, 422);
}

fn all_text(v: &Value) -> String {
    v.to_string()
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn viewer_role_sees_sensitive_fields_masked(pool: PgPool) {
    // AC-CNV-003
    let a = app(&pool).await;
    let f = fixture(&pool).await;
    let r = get(
        &a,
        &f.viewer,
        &format!("/api/v1/tenants/{}/conversations/{}", f.t, f.conv),
    )
    .await;
    assert_eq!(r.status, 200, "{}", String::from_utf8_lossy(&r.bytes));
    let d = &r.json()["data"];
    assert_eq!(d["masked"], true);
    assert_eq!(d["customer"]["name"], "B••• S•••");
    assert_eq!(d["customer"]["email"], "b•••@toko.id");
    assert_eq!(d["customer"]["phone"], "•••890");
    assert_eq!(
        d["timeline"][0]["body"],
        "Halo, email saya b•••@mail.co.id, WA •••890. Pesanan ORD-1234567?"
    );
    assert!(d["timeline"][1]["ai"]["tool_calls"][0]["args_summary"].is_null());
    assert_eq!(
        d["timeline"][1]["ai"]["tool_calls"][0]["name"],
        "get_order_status"
    );
    assert!(d["assignee_email"].as_str().unwrap().contains("•••@x.id"));
    assert_eq!(d["approvals"][0]["summary"], "Balasan untuk •••890");
    // Tidak ada nilai sensitif asli di mana pun dalam respons.
    let body = all_text(&r.json());
    for secret in [
        "Budi Santoso",
        "budi@toko.id",
        "budi.s@mail.co.id",
        "0812 3456 7890",
        "+6281234567890",
    ] {
        assert!(!body.contains(secret), "bocor: {secret}");
    }
    // Statistik AI tetap terlihat (bukan data sensitif).
    assert_eq!(d["timeline"][1]["ai"]["prompt_tokens"], 321);

    let l = get(
        &a,
        &f.viewer,
        &format!("/api/v1/tenants/{}/conversations", f.t),
    )
    .await;
    assert_eq!(l.json()["data"]["masked"], true);
    assert_eq!(
        l.json()["data"]["items"][0]["customer"]["email"],
        "b•••@toko.id"
    );

    // Role kustom: conversations.reply tanpa customers.read → tetap dimasking.
    let custom: Uuid = sqlx::query_scalar(
        "INSERT INTO roles (tenant_id, name) VALUES ($1, 'Chat Only') RETURNING id",
    )
    .bind(f.t)
    .fetch_one(&pool)
    .await
    .unwrap();
    sqlx::query("INSERT INTO role_permissions (role_id, permission_id) SELECT $1, id FROM permissions WHERE permission_key IN ('tenant.read', 'conversations.read', 'conversations.reply')")
        .bind(custom)
        .execute(&pool)
        .await
        .unwrap();
    let w = member(&pool, f.t, "Viewer").await;
    sqlx::query("UPDATE tenant_memberships SET role_id = $1 WHERE user_id = $2")
        .bind(custom)
        .bind(w.user)
        .execute(&pool)
        .await
        .unwrap();
    let r = get(
        &a,
        &w,
        &format!("/api/v1/tenants/{}/conversations/{}", f.t, f.conv),
    )
    .await;
    assert_eq!(
        (r.status, r.json()["data"]["masked"].clone()),
        (200, json!(true))
    );
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn permission_and_tenant_isolation(pool: PgPool) {
    let a = app(&pool).await;
    let f = fixture(&pool).await;
    // Tanpa conversations.read → 403.
    let r = get(
        &a,
        &f.catalog,
        &format!("/api/v1/tenants/{}/conversations", f.t),
    )
    .await;
    assert_eq!(r.status, 403);
    let r = get(
        &a,
        &f.catalog,
        &format!("/api/v1/tenants/{}/conversations/{}", f.t, f.conv),
    )
    .await;
    assert_eq!(r.status, 403);
    // Pemilik toko B: path toko A → 404 (bukan anggota); id percakapan A di path toko B → 404.
    let r = get(
        &a,
        &f.other,
        &format!("/api/v1/tenants/{}/conversations/{}", f.t, f.conv),
    )
    .await;
    assert_eq!(r.status, 404);
    let tb: Uuid =
        sqlx::query_scalar("SELECT tenant_id FROM tenant_memberships WHERE user_id = $1")
            .bind(f.other.user)
            .fetch_one(&pool)
            .await
            .unwrap();
    let r = get(
        &a,
        &f.other,
        &format!("/api/v1/tenants/{tb}/conversations/{}", f.conv),
    )
    .await;
    assert_eq!(r.status, 404);
    let r = get(&a, &f.other, &format!("/api/v1/tenants/{tb}/conversations")).await;
    assert_eq!(
        (r.status, r.json()["data"]["total"].clone()),
        (200, json!(0))
    );
}
