//! P6-4: handover, balasan staf, draft Assisted + persetujuan chat (PRD.md §4.9, §4.10, §4.12).
//! Pelanggan lewat endpoint publik (Host + cookie chat), staf lewat dashboard API. Provider AI tiruan.

mod common;

use std::{
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    time::Duration,
};

use axum::{
    Json, Router,
    body::Body,
    http::{Method, Request, header},
    routing::post,
};
use common::*;
use http_body_util::BodyExt as _;
use serde_json::{Value, json};
use sqlx::PgPool;
use tower::ServiceExt as _;
use umkm_api::{router::build_router, state::AiService};
use umkm_database::MIGRATOR;
use uuid::Uuid;

const HOST: &str = "toko-a.ryasai.my.id";
const REPLY: &str = "Kopi Gayo tersedia, harga Rp85.000.";

async fn provider() -> (u16, Arc<AtomicUsize>) {
    let hits = Arc::new(AtomicUsize::new(0));
    let h = hits.clone();
    let app = Router::new().route(
        "/v1/{*rest}",
        post(move |Json(_b): Json<Value>| {
            let h = h.clone();
            async move {
                h.fetch_add(1, Ordering::SeqCst);
                Json(
                    json!({ "model": "m1", "choices": [{ "message": { "content": REPLY } }],
                    "usage": { "prompt_tokens": 40, "completion_tokens": 10 } }),
                )
            }
        }),
    );
    let l = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = l.local_addr().unwrap().port();
    tokio::spawn(async move { axum::serve(l, app).await.unwrap() });
    (port, hits)
}

async fn chat_app(pool: &PgPool, port: u16) -> TestApp {
    let mut a = app(pool).await;
    let ai = AiService::new(
        app_pool(pool).await,
        umkm_infrastructure::crypto::Keyring::parse(TEST_KEK).unwrap(),
        Some(umkm_shared::config::PlatformAiSettings {
            enabled: true,
            base_url: format!("http://127.0.0.1:{port}/v1"),
            models: vec!["m1".into()],
            embedding_model: "e1".into(),
            tool_calling: false,
            embeddings: false,
            monthly_quota: 1_000_000,
            trusted_internal: true,
        }),
        None,
        4,
    );
    a.state = a.state.clone().with_ai(Arc::new(ai));
    a.router = build_router(a.state.clone(), LIMITS);
    a
}

async fn enable_chat(pool: &PgPool, t: Uuid, mode: &str) {
    pro(pool, t).await; // kuota AI platform (ADR-0016: uji coba Starter = 0 token)
    let prov: Uuid = sqlx::query_scalar(
        "INSERT INTO ai_providers (tenant_id, name, source, provider_type, model, capabilities)
         VALUES ($1, 'Platform', 'platform', 'platform', 'm1', '{\"chat\":true}') RETURNING id",
    )
    .bind(t)
    .fetch_one(pool)
    .await
    .unwrap();
    sqlx::query("INSERT INTO chatbot_configs (tenant_id, enabled, mode, primary_provider_id, timeout_ms) VALUES ($1, true, $2, $3, 5000)")
        .bind(t)
        .bind(mode)
        .bind(prov)
        .execute(pool)
        .await
        .unwrap();
}

async fn public(
    a: &TestApp,
    m: Method,
    path: &str,
    cookie: Option<&str>,
    body: Option<Value>,
) -> Res {
    let mut b = Request::builder()
        .method(m)
        .uri(path)
        .header(header::HOST, HOST)
        .header(header::ORIGIN, format!("https://{HOST}"))
        .header(header::CONTENT_TYPE, "application/json");
    if let Some(c) = cookie {
        b = b.header(header::COOKIE, c);
    }
    let res = a
        .router
        .clone()
        .oneshot(
            b.body(Body::from(body.map(|v| v.to_string()).unwrap_or_default()))
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

/// Pelanggan memulai percakapan; kembalikan (cookie, conversation_id).
async fn customer(a: &TestApp) -> (String, Uuid) {
    let r = public(
        a,
        Method::POST,
        "/api/v1/public/chat/conversations",
        None,
        Some(json!({ "consent_version": umkm_domain::ai::consent::VERSION, "accept": true })),
    )
    .await;
    assert_eq!(r.status, 201, "{}", String::from_utf8_lossy(&r.bytes));
    let cookie = r.headers[header::SET_COOKIE]
        .to_str()
        .unwrap()
        .split(';')
        .next()
        .unwrap()
        .to_owned();
    let id = Uuid::parse_str(r.json()["data"]["conversation_id"].as_str().unwrap()).unwrap();
    (cookie, id)
}

async fn say(a: &TestApp, cookie: &str, text: &str) -> Res {
    public(
        a,
        Method::POST,
        "/api/v1/public/chat/messages",
        Some(cookie),
        Some(json!({ "body": text })),
    )
    .await
}

async fn seen_by_customer(a: &TestApp, cookie: &str) -> Vec<(String, String)> {
    let r = public(
        a,
        Method::GET,
        "/api/v1/public/chat/conversation",
        Some(cookie),
        None,
    )
    .await;
    r.json()["data"]["messages"]
        .as_array()
        .unwrap()
        .iter()
        .map(|m| {
            (
                m["sender"].as_str().unwrap().to_owned(),
                m["body"].as_str().unwrap().to_owned(),
            )
        })
        .collect()
}

fn url(t: Uuid, conv: Uuid, rest: &str) -> String {
    format!("/api/v1/tenants/{t}/conversations/{conv}{rest}")
}

async fn staff(a: &TestApp, who: &Who, path: &str, body: Option<Value>) -> Res {
    json(
        &a.router,
        Method::POST,
        path,
        Some(who),
        Some(body.unwrap_or(json!({}))),
    )
    .await
}

fn reason(r: &Res) -> Value {
    r.json()["error"]["details"]["reason"].clone()
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn takeover_stops_ai_and_staff_reply_reaches_customer(pool: PgPool) {
    let (port, hits) = provider().await;
    let a = chat_app(&pool, port).await;
    let t = tenant(&pool, "toko-a").await;
    enable_chat(&pool, t, "auto").await;
    let agent = member(&pool, t, "Support Agent").await;
    let agent2 = member(&pool, t, "Support Agent").await;
    let viewer = member(&pool, t, "Viewer").await;
    let (cookie, conv) = customer(&a).await;
    say(&a, &cookie, "ada kopi?").await;
    assert_eq!(hits.load(Ordering::SeqCst), 1);

    // Viewer tidak boleh membalas / mengambil alih.
    assert_eq!(
        staff(
            &a,
            &viewer,
            &url(t, conv, "/messages"),
            Some(json!({"body": "x"}))
        )
        .await
        .status,
        403
    );
    assert_eq!(
        staff(&a, &viewer, &url(t, conv, "/takeover"), None)
            .await
            .status,
        403
    );

    // Balasan staf dari status AI = ambil alih; pelanggan menerimanya.
    let r = staff(
        &a,
        &agent,
        &url(t, conv, "/messages"),
        Some(json!({"body": "Halo kak, saya Rina."})),
    )
    .await;
    assert_eq!(r.status, 200, "{}", String::from_utf8_lossy(&r.bytes));
    assert_eq!(
        (
            r.json()["data"]["status"].clone(),
            r.json()["data"]["message"]["sender"].clone()
        ),
        (json!("HUMAN"), json!("ADMIN"))
    );
    let seen = seen_by_customer(&a, &cookie).await;
    assert_eq!(
        seen.last().unwrap(),
        &("ADMIN".to_owned(), "Halo kak, saya Rina.".to_owned())
    );
    // Takeover menghentikan auto-response.
    let r = say(&a, &cookie, "oke, ada promo?").await;
    assert_eq!(r.json()["data"]["status"], "HUMAN");
    assert_eq!(
        hits.load(Ordering::SeqCst),
        1,
        "AI tidak dipanggil setelah takeover"
    );

    // Staf lain tidak bisa menyela.
    let r = staff(
        &a,
        &agent2,
        &url(t, conv, "/messages"),
        Some(json!({"body": "x"})),
    )
    .await;
    assert_eq!((r.status, reason(&r)), (409, json!("assigned_to_other")));
    let r = staff(&a, &agent2, &url(t, conv, "/release"), None).await;
    assert_eq!((r.status, reason(&r)), (409, json!("assigned_to_other")));
    // Isi tidak valid.
    assert_eq!(
        staff(
            &a,
            &agent,
            &url(t, conv, "/messages"),
            Some(json!({"body": " "}))
        )
        .await
        .status,
        422
    );

    // Kembalikan ke AI → AI menjawab lagi.
    let r = staff(&a, &agent, &url(t, conv, "/release"), None).await;
    assert_eq!(r.json()["data"]["status"], "AI");
    let r = say(&a, &cookie, "ada kopi lagi?").await;
    assert_eq!(r.json()["data"]["messages"][1]["sender"], "AI");
    assert_eq!(hits.load(Ordering::SeqCst), 2);

    // Takeover eksplisit idempoten untuk penangan yang sama; tutup → pelanggan tidak bisa menulis.
    assert_eq!(
        staff(&a, &agent2, &url(t, conv, "/takeover"), None)
            .await
            .status,
        200
    );
    assert_eq!(
        staff(&a, &agent2, &url(t, conv, "/takeover"), None)
            .await
            .status,
        200
    );
    assert_eq!(
        staff(&a, &agent2, &url(t, conv, "/close"), None)
            .await
            .json()["data"]["status"],
        "CLOSED"
    );
    assert_eq!(say(&a, &cookie, "halo?").await.status, 409);
    let r = staff(
        &a,
        &agent,
        &url(t, conv, "/messages"),
        Some(json!({"body": "x"})),
    )
    .await;
    assert_eq!((r.status, reason(&r)), (409, json!("conversation_closed")));

    // Riwayat lengkap di viewer.
    let d = json(
        &a.router,
        Method::GET,
        &url(t, conv, ""),
        Some(&agent),
        None,
    )
    .await;
    let kinds: Vec<String> = d.json()["data"]["history"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| e["kind"].as_str().unwrap().to_owned())
        .collect();
    assert_eq!(
        kinds,
        ["created", "taken_over", "released", "taken_over", "closed"]
    );
    // Percakapan tenant lain tidak dapat disentuh.
    let tb = tenant(&pool, "toko-b").await;
    let ob = member(&pool, tb, "Owner").await;
    assert_eq!(
        staff(
            &a,
            &ob,
            &format!("/api/v1/tenants/{tb}/conversations/{conv}/messages"),
            Some(json!({"body": "x"}))
        )
        .await
        .status,
        404
    );
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn assisted_drafts_are_private_until_staff_decides(pool: PgPool) {
    let (port, hits) = provider().await;
    let a = chat_app(&pool, port).await;
    let t = tenant(&pool, "toko-a").await;
    enable_chat(&pool, t, "assisted").await;
    let agent = member(&pool, t, "Support Agent").await;
    let viewer = member(&pool, t, "Viewer").await;
    let (cookie, conv) = customer(&a).await;

    // Assisted: AI membuat draft, pelanggan hanya melihat pemberitahuan menunggu staf.
    let r = say(&a, &cookie, "ada kopi? WA saya 081234567890").await;
    assert_eq!(r.json()["data"]["status"], "WAITING_HUMAN");
    assert_eq!(hits.load(Ordering::SeqCst), 1);
    assert!(
        !seen_by_customer(&a, &cookie)
            .await
            .iter()
            .any(|(_, b)| b == REPLY),
        "draft tidak terlihat pelanggan"
    );
    let d = json(
        &a.router,
        Method::GET,
        &url(t, conv, ""),
        Some(&agent),
        None,
    )
    .await
    .json();
    let draft = d["data"]["drafts"][0].clone();
    assert_eq!(
        (draft["status"].clone(), draft["body"].clone()),
        (json!("pending"), json!(REPLY))
    );
    assert!(draft["ai_run_id"].is_string());
    let draft_id = draft["id"].as_str().unwrap().to_owned();

    // Viewer tidak boleh memutus draft.
    assert_eq!(
        staff(
            &a,
            &viewer,
            &url(t, conv, &format!("/drafts/{draft_id}/send")),
            None
        )
        .await
        .status,
        403
    );

    // Kirim draft diedit → pesan ADMIN atas nama staf; ambil alih otomatis.
    let edited = "Kopi Gayo ada kak, Rp85.000 per 250g.";
    let r = staff(
        &a,
        &agent,
        &url(t, conv, &format!("/drafts/{draft_id}/send")),
        Some(json!({"body": edited})),
    )
    .await;
    assert_eq!(r.status, 200, "{}", String::from_utf8_lossy(&r.bytes));
    assert_eq!(
        (
            r.json()["data"]["message"]["sender"].clone(),
            r.json()["data"]["status"].clone()
        ),
        (json!("ADMIN"), json!("HUMAN"))
    );
    assert_eq!(
        seen_by_customer(&a, &cookie).await.last().unwrap(),
        &("ADMIN".to_owned(), edited.to_owned())
    );
    // Sekali saja.
    let r = staff(
        &a,
        &agent,
        &url(t, conv, &format!("/drafts/{draft_id}/send")),
        None,
    )
    .await;
    assert_eq!((r.status, reason(&r)), (409, json!("draft_decided")));
    assert_eq!(
        staff(
            &a,
            &agent,
            &url(t, conv, &format!("/drafts/{draft_id}/discard")),
            None
        )
        .await
        .status,
        409
    );

    // Draft berikutnya dikirim apa adanya → pesan AI dengan jejak run.
    staff(&a, &agent, &url(t, conv, "/release"), None).await;
    say(&a, &cookie, "jam buka?").await;
    let d = json(
        &a.router,
        Method::GET,
        &url(t, conv, ""),
        Some(&agent),
        None,
    )
    .await
    .json();
    let d2 = d["data"]["drafts"]
        .as_array()
        .unwrap()
        .iter()
        .find(|x| x["status"] == "pending")
        .unwrap()["id"]
        .as_str()
        .unwrap()
        .to_owned();
    let r = staff(
        &a,
        &agent,
        &url(t, conv, &format!("/drafts/{d2}/send")),
        None,
    )
    .await;
    assert_eq!(r.json()["data"]["message"]["sender"], "AI");
    let d = json(
        &a.router,
        Method::GET,
        &url(t, conv, ""),
        Some(&agent),
        None,
    )
    .await
    .json();
    let last = d["data"]["timeline"]
        .as_array()
        .unwrap()
        .last()
        .unwrap()
        .clone();
    assert_eq!(
        (last["sender"].clone(), last["body"].clone()),
        (json!("AI"), json!(REPLY))
    );
    assert!(last["ai"]["run_id"].is_string());

    // Draft ketiga dibuang → tidak pernah sampai ke pelanggan.
    staff(&a, &agent, &url(t, conv, "/release"), None).await;
    let before = seen_by_customer(&a, &cookie).await.len();
    say(&a, &cookie, "ongkir?").await;
    let d = json(
        &a.router,
        Method::GET,
        &url(t, conv, ""),
        Some(&agent),
        None,
    )
    .await
    .json();
    let d3 = d["data"]["drafts"]
        .as_array()
        .unwrap()
        .iter()
        .find(|x| x["status"] == "pending")
        .unwrap()["id"]
        .as_str()
        .unwrap()
        .to_owned();
    assert_eq!(
        staff(
            &a,
            &agent,
            &url(t, conv, &format!("/drafts/{d3}/discard")),
            None
        )
        .await
        .status,
        204
    );
    let after = seen_by_customer(&a, &cookie).await;
    assert_eq!(
        after.len(),
        before + 2,
        "hanya pesan pelanggan + pemberitahuan SYSTEM"
    );
    assert!(after.iter().filter(|(s, _)| s == "AI").count() == 1);

    let kinds: Vec<String> = d["data"]["history"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| e["kind"].as_str().unwrap().to_owned())
        .collect();
    assert!(
        kinds.contains(&"draft_created".to_owned()) && kinds.contains(&"draft_sent".to_owned())
    );
    // Viewer melihat draft dengan isi dimasking.
    let v = json(
        &a.router,
        Method::GET,
        &url(t, conv, ""),
        Some(&viewer),
        None,
    )
    .await
    .json();
    assert_eq!(v["data"]["masked"], true);
    assert!(!v.to_string().contains("081234567890"));
    assert_eq!(v["data"]["drafts"].as_array().unwrap().len(), 3);
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn chat_approval_policy_holds_draft_until_approved(pool: PgPool) {
    let (port, _) = provider().await;
    let a = chat_app(&pool, port).await;
    let t = tenant(&pool, "toko-a").await;
    enable_chat(&pool, t, "assisted").await;
    let owner = member(&pool, t, "Owner").await;
    let agent = member(&pool, t, "Support Agent").await;
    let manager = member(&pool, t, "Manager").await;
    let manager_role: Uuid =
        sqlx::query_scalar("SELECT id FROM roles WHERE tenant_id IS NULL AND name = 'Manager'")
            .fetch_one(&pool)
            .await
            .unwrap();
    let r = json(
        &a.router,
        Method::PUT,
        &format!("/api/v1/tenants/{t}/approval-policies/ai_response"),
        Some(&owner),
        Some(
            json!({"enabled": true, "steps": [{"role_ids": [manager_role]}], "timeout_hours": 24}),
        ),
    )
    .await;
    assert_eq!(r.status, 200, "{}", String::from_utf8_lossy(&r.bytes));

    let (cookie, conv) = customer(&a).await;
    say(&a, &cookie, "ada kopi?").await;
    let d = json(
        &a.router,
        Method::GET,
        &url(t, conv, ""),
        Some(&agent),
        None,
    )
    .await
    .json();
    let draft_id = d["data"]["drafts"][0]["id"].as_str().unwrap().to_owned();
    let r = staff(
        &a,
        &agent,
        &url(t, conv, &format!("/drafts/{draft_id}/send")),
        None,
    )
    .await;
    assert_eq!(r.status, 202, "{}", String::from_utf8_lossy(&r.bytes));
    let req = r.json()["data"]["approval_request_id"]
        .as_str()
        .unwrap()
        .to_owned();
    // Menunggu: pelanggan belum melihat; draft approval_pending; viewer memuat approval.
    assert!(
        !seen_by_customer(&a, &cookie)
            .await
            .iter()
            .any(|(_, b)| b == REPLY)
    );
    let d = json(
        &a.router,
        Method::GET,
        &url(t, conv, ""),
        Some(&agent),
        None,
    )
    .await
    .json();
    assert_eq!(d["data"]["drafts"][0]["status"], "approval_pending");
    assert_eq!(d["data"]["approvals"][0]["status"], "pending");
    // Tidak bisa dikirim ulang lewat jalan pintas saat menunggu.
    let r = staff(
        &a,
        &agent,
        &url(t, conv, &format!("/drafts/{draft_id}/send")),
        None,
    )
    .await;
    assert_eq!(r.status, 409);

    // Manager menyetujui → executor mengirim atas nama pemohon.
    let r = json(
        &a.router,
        Method::POST,
        &format!("/api/v1/tenants/{t}/approvals/{req}/approve"),
        Some(&manager),
        Some(json!({"step": 0})),
    )
    .await;
    assert_eq!(r.status, 200, "{}", String::from_utf8_lossy(&r.bytes));
    assert_eq!(
        seen_by_customer(&a, &cookie).await.last().unwrap(),
        &("AI".to_owned(), REPLY.to_owned())
    );
    let d = json(
        &a.router,
        Method::GET,
        &url(t, conv, ""),
        Some(&agent),
        None,
    )
    .await
    .json();
    assert_eq!(d["data"]["drafts"][0]["status"], "sent");
    assert_eq!(d["data"]["approvals"][0]["status"], "approved");
    assert_eq!(
        d["data"]["assignee_user_id"],
        json!(agent.user),
        "pemohon menjadi penangan"
    );
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn staff_sse_carries_no_message_content(pool: PgPool) {
    let (port, _) = provider().await;
    let a = chat_app(&pool, port).await;
    let t = tenant(&pool, "toko-a").await;
    enable_chat(&pool, t, "auto").await;
    let viewer = member(&pool, t, "Viewer").await;
    let res = a
        .router
        .clone()
        .oneshot(
            Request::builder()
                .uri(format!("/api/v1/tenants/{t}/events"))
                .header(header::COOKIE, &viewer.cookie)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status().as_u16(), 200);
    let mut body = res.into_body();
    let (cookie, conv) = customer(&a).await;
    say(&a, &cookie, "rahasia 081234567890").await;
    let mut got = String::new();
    let deadline = tokio::time::Instant::now() + Duration::from_secs(3);
    while let Some(left) = deadline.checked_duration_since(tokio::time::Instant::now()) {
        match tokio::time::timeout(left, body.frame()).await {
            Ok(Some(Ok(f))) => {
                if let Ok(d) = f.into_data() {
                    got.push_str(&String::from_utf8_lossy(&d));
                }
                if got.contains(REPLY) || got.matches("chat.message").count() >= 2 {
                    break;
                }
            }
            _ => break,
        }
    }
    assert!(got.contains("event: chat.message"), "{got}");
    assert!(
        got.contains(&conv.to_string()),
        "ping membawa conversation_id"
    );
    assert!(
        !got.contains("rahasia") && !got.contains("081234567890") && !got.contains(REPLY),
        "{got}"
    );
}
