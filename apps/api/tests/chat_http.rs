//! P6-2: web chat publik (ADR-0006, D-12 consent, `PRD.md §7.5`). Role aplikasi + RLS, provider AI
//! tiruan HTTP lokal (provider platform trusted internal), tenant dari Host.

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
use umkm_domain::conversation::ConversationStatus;
use umkm_infrastructure::conversation_repo::{self as repo, Actor};
use umkm_shared::ids::TenantId;
use uuid::Uuid;

const HOST_A: &str = "toko-a.ryasai.my.id";
const HOST_B: &str = "toko-b.ryasai.my.id";
const REPLY: &str = "Kopi Gayo tersedia, harga Rp85.000.";

/// Provider OpenAI-compatible tiruan; menghitung panggilan.
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

/// TestApp dengan runtime AI yang menunjuk provider tiruan.
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

/// Chatbot aktif dengan provider platform `m1`.
async fn enable_chat(pool: &PgPool, t: Uuid, mode: &str, with_provider: bool, welcome: &str) {
    let prov: Option<Uuid> = if with_provider {
        Some(
            sqlx::query_scalar(
                "INSERT INTO ai_providers (tenant_id, name, source, provider_type, model, capabilities)
                 VALUES ($1, 'Platform', 'platform', 'platform', 'm1', '{\"chat\":true,\"usage_reporting\":true}') RETURNING id",
            )
            .bind(t)
            .fetch_one(pool)
            .await
            .unwrap(),
        )
    } else {
        None
    };
    sqlx::query(
        "INSERT INTO chatbot_configs (tenant_id, enabled, mode, primary_provider_id, welcome_message, timeout_ms)
         VALUES ($1, true, $2, $3, $4, 5000)
         ON CONFLICT (tenant_id) DO UPDATE SET enabled = true, mode = $2, primary_provider_id = $3, welcome_message = $4",
    )
    .bind(t)
    .bind(mode)
    .bind(prov)
    .bind(welcome)
    .execute(pool)
    .await
    .unwrap();
}

async fn req(
    a: &TestApp,
    m: Method,
    host: &str,
    path: &str,
    cookie: Option<&str>,
    origin: Option<&str>,
    body: Option<Value>,
) -> Res {
    let mut b = Request::builder()
        .method(m)
        .uri(path)
        .header(header::HOST, host)
        .header(header::CONTENT_TYPE, "application/json");
    if let Some(c) = cookie {
        b = b.header(header::COOKIE, c);
    }
    if let Some(o) = origin {
        b = b.header(header::ORIGIN, o);
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

fn origin(host: &str) -> String {
    format!("https://{host}")
}

/// Mulai percakapan di `host`; kembalikan (cookie, response JSON).
async fn start(a: &TestApp, host: &str) -> (String, Value) {
    let cfg = req(
        a,
        Method::GET,
        host,
        "/api/v1/public/chat/config",
        None,
        None,
        None,
    )
    .await;
    let version = cfg.json()["data"]["consent_version"]
        .as_str()
        .unwrap()
        .to_owned();
    let r = req(
        a,
        Method::POST,
        host,
        "/api/v1/public/chat/conversations",
        None,
        Some(&origin(host)),
        Some(json!({ "consent_version": version, "accept": true })),
    )
    .await;
    assert_eq!(r.status, 201, "{}", String::from_utf8_lossy(&r.bytes));
    let set = r.headers[header::SET_COOKIE].to_str().unwrap().to_owned();
    let cookie = set.split(';').next().unwrap().to_owned();
    (cookie, r.json())
}

async fn say(a: &TestApp, host: &str, cookie: &str, text: &str) -> Res {
    req(
        a,
        Method::POST,
        host,
        "/api/v1/public/chat/messages",
        Some(cookie),
        Some(&origin(host)),
        Some(json!({ "body": text })),
    )
    .await
}

fn senders(v: &Value) -> Vec<String> {
    v["data"]["messages"]
        .as_array()
        .unwrap()
        .iter()
        .map(|m| m["sender"].as_str().unwrap().to_owned())
        .collect()
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn consent_gate_cookie_and_tenant_scope(pool: PgPool) {
    let (port, _) = provider().await;
    let a = chat_app(&pool, port).await;
    let ta = tenant(&pool, "toko-a").await;
    tenant(&pool, "toko-b").await;

    // Chat belum diaktifkan pemilik → config enabled=false, mulai 409.
    let cfg = req(
        &a,
        Method::GET,
        HOST_A,
        "/api/v1/public/chat/config",
        None,
        None,
        None,
    )
    .await;
    assert_eq!(cfg.status, 200);
    assert_eq!(cfg.json()["data"]["enabled"], false);
    let text = cfg.json()["data"]["consent_text"]
        .as_str()
        .unwrap()
        .to_owned();
    for e in umkm_domain::ai::consent::REQUIRED_ELEMENTS {
        assert!(text.contains(e), "D-12: teks pemberitahuan memuat {e}");
    }
    let version = umkm_domain::ai::consent::VERSION;
    let body = json!({ "consent_version": version, "accept": true });
    let p = "/api/v1/public/chat/conversations";
    let r = req(
        &a,
        Method::POST,
        HOST_A,
        p,
        None,
        Some(&origin(HOST_A)),
        Some(body.clone()),
    )
    .await;
    assert_eq!(
        (r.status, r.json()["error"]["details"]["reason"].clone()),
        (409, json!("chat_disabled"))
    );

    enable_chat(&pool, ta, "hybrid", true, "Halo! Ada yang bisa dibantu?").await;
    // Tanpa persetujuan / versi lama → 422; Origin toko lain → 403.
    let r = req(
        &a,
        Method::POST,
        HOST_A,
        p,
        None,
        Some(&origin(HOST_A)),
        Some(json!({ "consent_version": version, "accept": false })),
    )
    .await;
    assert_eq!(r.status, 422);
    let r = req(
        &a,
        Method::POST,
        HOST_A,
        p,
        None,
        Some(&origin(HOST_A)),
        Some(json!({ "consent_version": "v-lama", "accept": true })),
    )
    .await;
    assert_eq!(r.status, 422);
    let r = req(
        &a,
        Method::POST,
        HOST_A,
        p,
        None,
        Some(&origin(HOST_B)),
        Some(body.clone()),
    )
    .await;
    assert_eq!(r.status, 403);
    let n: i64 = sqlx::query_scalar("SELECT count(*) FROM conversations")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(n, 0, "tanpa persetujuan tidak ada percakapan");

    // Mulai: cookie host-only HttpOnly Secure, pesan sambutan SYSTEM, versi consent tersimpan.
    let r = req(
        &a,
        Method::POST,
        HOST_A,
        p,
        None,
        Some(&origin(HOST_A)),
        Some(body),
    )
    .await;
    assert_eq!(r.status, 201);
    let set = r.headers[header::SET_COOKIE].to_str().unwrap();
    assert!(set.starts_with("__Host-umkm_chat="), "{set}");
    for attr in ["HttpOnly", "Secure", "SameSite=Lax", "Path=/"] {
        assert!(set.contains(attr), "{attr}");
    }
    assert!(!set.contains("Domain="), "host-only");
    assert_eq!(r.json()["data"]["status"], "AI");
    assert_eq!(senders(&r.json()), ["SYSTEM"]);
    let cookie = set.split(';').next().unwrap().to_owned();
    let token = cookie.split_once('=').unwrap().1.to_owned();
    assert!(
        !r.bytes.windows(token.len()).any(|w| w == token.as_bytes()),
        "token tidak ada di body"
    );
    let (v, consented): (String, bool) =
        sqlx::query_as("SELECT consent_version, consented_at IS NOT NULL FROM conversations")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!((v.as_str(), consented), (version, true));

    // Cookie hanya berlaku di toko ini; tanpa cookie 401.
    let g = "/api/v1/public/chat/conversation";
    assert_eq!(
        req(&a, Method::GET, HOST_A, g, Some(&cookie), None, None)
            .await
            .status,
        200
    );
    assert_eq!(
        req(&a, Method::GET, HOST_A, g, None, None, None)
            .await
            .status,
        401
    );
    assert_eq!(
        req(&a, Method::GET, HOST_B, g, Some(&cookie), None, None)
            .await
            .status,
        401
    );
    assert_eq!(say(&a, HOST_B, &cookie, "halo").await.status, 401);
    // Isi tidak valid → 422 field body.
    let r = say(&a, HOST_A, &cookie, "   ").await;
    assert_eq!(
        (
            r.status,
            r.json()["error"]["details"]["errors"][0]["path"].clone()
        ),
        (422, json!("body"))
    );
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn ai_answers_and_run_is_linked_to_conversation(pool: PgPool) {
    let (port, hits) = provider().await;
    let a = chat_app(&pool, port).await;
    let t = tenant(&pool, "toko-a").await;
    enable_chat(&pool, t, "auto", true, "").await;
    let (cookie, started) = start(&a, HOST_A).await;
    assert!(started["data"]["messages"].as_array().unwrap().is_empty());
    let conv = started["data"]["conversation_id"]
        .as_str()
        .unwrap()
        .to_owned();

    let r = say(&a, HOST_A, &cookie, "ada kopi gayo?").await;
    assert_eq!(r.status, 200, "{}", String::from_utf8_lossy(&r.bytes));
    assert_eq!(senders(&r.json()), ["CUSTOMER", "AI"]);
    assert_eq!(r.json()["data"]["messages"][1]["body"], REPLY);
    assert_eq!(r.json()["data"]["status"], "AI");
    assert_eq!(hits.load(Ordering::SeqCst), 1);
    // Pelanggan tidak melihat id staf / run AI.
    let m = &r.json()["data"]["messages"][1];
    assert!(m.get("sender_user_id").is_none() && m.get("ai_run_id").is_none());

    // ai_runs terhubung ke percakapan + latensi tercatat; pesan AI menunjuk run itu.
    let (run_conv, purpose, latency): (Option<Uuid>, String, i32) =
        sqlx::query_as("SELECT conversation_id, purpose, latency_ms FROM ai_runs")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(run_conv.map(|u| u.to_string()), Some(conv.clone()));
    assert_eq!(purpose, "chat");
    assert!(latency >= 0);
    let linked: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM conversation_messages m JOIN ai_runs r ON r.id = m.ai_run_id WHERE m.sender_type = 'AI'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(linked, 1);

    // Riwayat percakapan dikirim ke giliran berikutnya; pesan dapat diambil ulang setelah seq.
    let r2 = say(&a, HOST_A, &cookie, "kirim ke Bandung bisa?").await;
    assert_eq!(senders(&r2.json()), ["CUSTOMER", "AI"]);
    let first_seq = r.json()["data"]["messages"][1]["seq"].as_i64().unwrap();
    let g = req(
        &a,
        Method::GET,
        HOST_A,
        &format!("/api/v1/public/chat/conversation?after={first_seq}"),
        Some(&cookie),
        None,
        None,
    )
    .await;
    assert_eq!(senders(&g.json()), ["CUSTOMER", "AI"]);
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn handover_and_takeover_stop_auto_response(pool: PgPool) {
    let (port, hits) = provider().await;
    let a = chat_app(&pool, port).await;
    let t = tenant(&pool, "toko-a").await;
    enable_chat(&pool, t, "hybrid", true, "").await;

    // Frasa yang dijanjikan di pemberitahuan AI → handover tanpa memanggil provider.
    let (cookie, _) = start(&a, HOST_A).await;
    let r = say(&a, HOST_A, &cookie, "Saya mau bicara dengan admin").await;
    assert_eq!(r.json()["data"]["status"], "WAITING_HUMAN");
    assert_eq!(senders(&r.json()).last().unwrap(), "SYSTEM");
    assert_eq!(hits.load(Ordering::SeqCst), 0);
    // Menunggu staf: pesan berikutnya tersimpan tanpa balasan AI.
    let r = say(&a, HOST_A, &cookie, "halo?").await;
    assert_eq!(
        (senders(&r.json()), r.json()["data"]["status"].clone()),
        (vec!["CUSTOMER".to_owned()], json!("WAITING_HUMAN"))
    );
    assert_eq!(hits.load(Ordering::SeqCst), 0);

    // Tombol handover; staf mengambil alih → AI berhenti.
    let (cookie2, s2) = start(&a, HOST_A).await;
    let conv2 = Uuid::parse_str(s2["data"]["conversation_id"].as_str().unwrap()).unwrap();
    let h = req(
        &a,
        Method::POST,
        HOST_A,
        "/api/v1/public/chat/handover",
        Some(&cookie2),
        Some(&origin(HOST_A)),
        Some(json!({})),
    )
    .await;
    assert_eq!(
        (h.status, h.json()["data"]["status"].clone()),
        (200, json!("WAITING_HUMAN"))
    );
    let h = req(
        &a,
        Method::POST,
        HOST_A,
        "/api/v1/public/chat/handover",
        Some(&cookie2),
        Some(&origin(HOST_A)),
        Some(json!({})),
    )
    .await;
    assert_eq!(h.status, 409, "sudah di staf");
    let staff = member(&pool, t, "Support Agent").await;
    repo::transition(
        &app_pool(&pool).await,
        TenantId::from_uuid(t),
        conv2,
        ConversationStatus::Human,
        Actor::Staff(staff.user),
    )
    .await
    .unwrap();
    let r = say(&a, HOST_A, &cookie2, "ada promo?").await;
    assert_eq!(
        (senders(&r.json()), r.json()["data"]["status"].clone()),
        (vec!["CUSTOMER".to_owned()], json!("HUMAN"))
    );
    assert_eq!(
        hits.load(Ordering::SeqCst),
        0,
        "takeover menghentikan auto-response"
    );

    // Kontrol: percakapan AI biasa memang memanggil provider.
    let (cookie3, _) = start(&a, HOST_A).await;
    say(&a, HOST_A, &cookie3, "ada kopi?").await;
    assert_eq!(hits.load(Ordering::SeqCst), 1);
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn human_only_and_unconfigured_never_call_ai(pool: PgPool) {
    let (port, hits) = provider().await;
    let a = chat_app(&pool, port).await;
    let ta = tenant(&pool, "toko-a").await;
    let tb = tenant(&pool, "toko-b").await;
    // AC-AIC-004: Human Only → mulai WAITING_HUMAN, AI tidak pernah membalas.
    enable_chat(&pool, ta, "human_only", true, "").await;
    let (cookie, s) = start(&a, HOST_A).await;
    assert_eq!(s["data"]["status"], "WAITING_HUMAN");
    let r = say(&a, HOST_A, &cookie, "ada kopi?").await;
    assert_eq!(senders(&r.json()), ["CUSTOMER"]);
    // Provider dinonaktifkan → diteruskan ke staf, chat tidak macet.
    enable_chat(&pool, tb, "hybrid", true, "").await;
    sqlx::query("UPDATE ai_providers SET status = 'disabled' WHERE tenant_id = $1")
        .bind(tb)
        .execute(&pool)
        .await
        .unwrap();
    let (cookie_b, _) = start(&a, HOST_B).await;
    let r = say(&a, HOST_B, &cookie_b, "ada kopi?").await;
    assert_eq!(r.status, 200);
    assert_eq!(senders(&r.json()), ["CUSTOMER", "SYSTEM"]);
    assert_eq!(r.json()["data"]["status"], "WAITING_HUMAN");
    assert_eq!(hits.load(Ordering::SeqCst), 0);
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn end_closes_conversation_and_revokes_session(pool: PgPool) {
    let (port, _) = provider().await;
    let a = chat_app(&pool, port).await;
    let t = tenant(&pool, "toko-a").await;
    enable_chat(&pool, t, "hybrid", true, "").await;
    let (cookie, _) = start(&a, HOST_A).await;
    let e = "/api/v1/public/chat/end";
    assert_eq!(
        req(
            &a,
            Method::POST,
            HOST_A,
            e,
            Some(&cookie),
            Some(&origin(HOST_B)),
            Some(json!({}))
        )
        .await
        .status,
        403
    );
    let r = req(
        &a,
        Method::POST,
        HOST_A,
        e,
        Some(&cookie),
        Some(&origin(HOST_A)),
        Some(json!({})),
    )
    .await;
    assert_eq!(r.status, 204);
    assert!(
        r.headers[header::SET_COOKIE]
            .to_str()
            .unwrap()
            .contains("Max-Age=0")
    );
    assert_eq!(
        req(
            &a,
            Method::GET,
            HOST_A,
            "/api/v1/public/chat/conversation",
            Some(&cookie),
            None,
            None
        )
        .await
        .status,
        401
    );
    assert_eq!(say(&a, HOST_A, &cookie, "halo").await.status, 401);
    let st: String = sqlx::query_scalar("SELECT status FROM conversations")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(st, "CLOSED");
}

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

async fn open_stream(a: &TestApp, host: &str, cookie: Option<&str>) -> (u16, Body) {
    let mut b = Request::builder()
        .uri("/api/v1/public/chat/stream")
        .header(header::HOST, host);
    if let Some(c) = cookie {
        b = b.header(header::COOKIE, c);
    }
    let res = a
        .router
        .clone()
        .oneshot(b.body(Body::empty()).unwrap())
        .await
        .unwrap();
    (res.status().as_u16(), res.into_body())
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn stream_delivers_only_own_conversation(pool: PgPool) {
    let (port, _) = provider().await;
    let a = chat_app(&pool, port).await;
    let ta = tenant(&pool, "toko-a").await;
    let tb = tenant(&pool, "toko-b").await;
    enable_chat(&pool, ta, "auto", true, "").await;
    enable_chat(&pool, tb, "hybrid", true, "").await;
    let (c1, _) = start(&a, HOST_A).await;
    let (c2, _) = start(&a, HOST_A).await;
    let (cb, _) = start(&a, HOST_B).await;

    assert_eq!(open_stream(&a, HOST_A, None).await.0, 401);
    let (st, mut s1) = open_stream(&a, HOST_A, Some(&c1)).await;
    assert_eq!(st, 200);
    // Pesan percakapan lain (tenant sama & tenant lain) tidak sampai ke stream c1.
    say(&a, HOST_A, &c2, "rahasia percakapan 2").await;
    say(&a, HOST_B, &cb, "rahasia toko B").await;
    assert!(
        next_event(&mut s1, Duration::from_millis(700))
            .await
            .is_none()
    );
    // Pesan sendiri + balasan AI sampai lewat SSE.
    say(&a, HOST_A, &c1, "ada kopi?").await;
    let mut got = String::new();
    while let Some(f) = next_event(&mut s1, Duration::from_secs(3)).await {
        got.push_str(&f);
        if got.contains(REPLY) {
            break;
        }
    }
    assert!(got.contains("event: chat.message"), "{got}");
    assert!(got.contains("ada kopi?") && got.contains(REPLY), "{got}");
    assert!(!got.contains("rahasia"), "{got}");
    assert!(
        !got.contains("conversation_id"),
        "pelanggan hanya menerima isi pesan"
    );

    // Sesi diakhiri → event berikutnya menutup stream.
    let (st, mut s2) = open_stream(&a, HOST_A, Some(&c2)).await;
    assert_eq!(st, 200);
    sqlx::query("UPDATE conversation_sessions SET revoked_at = now()")
        .execute(&pool)
        .await
        .unwrap();
    say(&a, HOST_A, &c1, "halo lagi").await; // c1 juga dicabut → 401, tidak ada event
    let conv2: Uuid = sqlx::query_scalar(
        "SELECT conversation_id FROM conversation_sessions ORDER BY created_at OFFSET 1 LIMIT 1",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    // Event untuk percakapan 2 dikirim langsung ke hub: stream harus berakhir, bukan meneruskannya.
    let mut ev = umkm_api::realtime::TenantEvent::tenant(
        TenantId::from_uuid(ta),
        "chat.message",
        json!({ "conversation_id": conv2.to_string(), "customer": { "body": "setelah dicabut" } }),
    );
    ev.permission = Some("conversations.read".into());
    a.state.realtime.publish(ev);
    assert!(
        next_event(&mut s2, Duration::from_millis(700))
            .await
            .is_none()
    );
}
