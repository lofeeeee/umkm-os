//! P5-8: runtime AI end-to-end terhadap provider tiruan HTTP lokal (provider platform
//! trusted internal) dan PostgreSQL nyata (role aplikasi): tool backend membaca katalog tenant,
//! metering tercatat, kuota, circuit breaker, timeout, Human Only, chatbot nonaktif.

use std::{
    sync::{
        Arc, Mutex,
        atomic::{AtomicUsize, Ordering},
    },
    time::{Duration, Instant},
};

use axum::{
    Json, Router,
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
    routing::post,
};
use serde_json::{Value, json};
use sqlx::PgPool;
use umkm_application::ai::orchestrator::{ProviderUsed, TurnInput};
use umkm_database::MIGRATOR;
use umkm_domain::ai::Decision;
use umkm_infrastructure::{
    ai::{
        circuit_breaker::CircuitBreaker,
        runtime::{AiRunError, AiRuntime, BREAKER_THRESHOLD},
    },
    crypto::Keyring,
    http_client::{Egress, EgressPolicy, Resolver},
};
use umkm_shared::{config::PlatformAiSettings, ids::TenantId};
use uuid::Uuid;

#[derive(Clone, Default)]
struct Seen {
    hits: Arc<AtomicUsize>,
    bodies: Arc<Mutex<Vec<Value>>>,
}

async fn provider() -> (u16, Seen) {
    let seen = Seen::default();
    async fn h(
        State(s): State<Seen>,
        Path((mode, rest)): Path<(String, String)>,
        Json(b): Json<Value>,
    ) -> axum::response::Response {
        s.hits.fetch_add(1, Ordering::SeqCst);
        s.bodies.lock().unwrap().push(b.clone());
        match (mode.as_str(), rest.as_str()) {
            ("down", _) => StatusCode::BAD_GATEWAY.into_response(),
            ("slow", _) => {
                tokio::time::sleep(Duration::from_secs(3)).await;
                Json(json!({})).into_response()
            }
            (_, "embeddings") => Json(json!({ "data": b["input"].as_array().unwrap().iter().enumerate().map(|(i, _)| json!({ "index": i, "embedding": [1.0, 0.0, 0.0, 0.0] })).collect::<Vec<_>>() })).into_response(),
            _ => {
                let has_tool_result = b["messages"].as_array().unwrap().iter().any(|m| m["role"] == "tool");
                if b.get("tools").is_some() && !has_tool_result {
                    Json(json!({ "model": "m1", "choices": [{ "message": { "content": null, "tool_calls": [
                        { "id": "c1", "type": "function", "function": { "name": "search_products", "arguments": "{\"query\":\"kopi\"}" } }] } }],
                        "usage": { "prompt_tokens": 300, "completion_tokens": 20 } })).into_response()
                } else {
                    Json(json!({ "model": "m1", "choices": [{ "message": { "content": "Kopi Gayo tersedia, harga Rp85.000." } }],
                        "usage": { "prompt_tokens": 400, "completion_tokens": 15 } })).into_response()
                }
            }
        }
    }
    let app = Router::new()
        .route("/{mode}/v1/{*rest}", post(h))
        .with_state(seen.clone());
    let l = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = l.local_addr().unwrap().port();
    tokio::spawn(async move { axum::serve(l, app).await.unwrap() });
    (port, seen)
}

async fn app_pool(pool: &PgPool) -> PgPool {
    sqlx::postgres::PgPoolOptions::new()
        .max_connections(4)
        .after_connect(|c, _| {
            Box::pin(async move {
                sqlx::query("SET ROLE umkm_app_role")
                    .execute(c)
                    .await
                    .map(|_| ())
            })
        })
        .connect_with(pool.connect_options().as_ref().clone())
        .await
        .unwrap()
}

fn runtime(app: PgPool, port: u16, mode: &str, quota: u64) -> AiRuntime {
    AiRuntime {
        pool: app,
        keyring: Keyring::parse("v1:a2VrLXJhaGFzaWEta2VrLXJhaGFzaWEta2VrLXJhaGE=").unwrap(),
        platform: Some(PlatformAiSettings {
            enabled: true,
            base_url: format!("http://127.0.0.1:{port}/{mode}/v1"),
            models: vec!["m1".into()],
            embedding_model: "e1".into(),
            tool_calling: true,
            embeddings: true,
            monthly_quota: quota,
            trusted_internal: true,
            ..Default::default()
        }),
        platform_key: None,
        tenant_egress: Egress::new(
            EgressPolicy::tenant(Duration::from_secs(5)),
            Resolver::System,
            4,
        ),
        operator_egress: Egress::new(
            EgressPolicy::operator(Duration::from_secs(5)),
            Resolver::System,
            4,
        ),
        breaker: Arc::new(CircuitBreaker::new(
            BREAKER_THRESHOLD,
            Duration::from_secs(30),
        )),
    }
}

/// Tenant + produk + provider platform + config aktif (mode, timeout).
async fn setup(pool: &PgPool, mode: &str, enabled: bool, timeout_ms: i32) -> TenantId {
    let t: Uuid = sqlx::query_scalar(
        "INSERT INTO tenants (name, slug) VALUES ('Kopi Senja', $1) RETURNING id",
    )
    .bind(format!("t{}", &Uuid::new_v4().simple().to_string()[..8]))
    .fetch_one(pool)
    .await
    .unwrap();
    // Paket Pro aktif: kuota AI platform berasal dari paket (P7-2, ADR-0016).
    sqlx::query("UPDATE subscriptions SET plan_code = 'pro', status = 'active', current_period_end = now() + interval '30 days' WHERE tenant_id = $1")
        .bind(t)
        .execute(pool)
        .await
        .unwrap();
    let p: Uuid = sqlx::query_scalar("INSERT INTO products (tenant_id, name, slug, status, available) VALUES ($1, 'Kopi Gayo 250g', 'kopi-gayo', 'active', true) RETURNING id")
        .bind(t)
        .fetch_one(pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO product_variants (tenant_id, product_id, sku, name, price, is_active, position) VALUES ($1, $2, 'KG-250', '250g', 85000, true, 0)")
        .bind(t)
        .bind(p)
        .execute(pool)
        .await
        .unwrap();
    let prov: Uuid = sqlx::query_scalar(
        "INSERT INTO ai_providers (tenant_id, name, source, provider_type, model, capabilities)
         VALUES ($1, 'Platform', 'platform', 'platform', 'm1', '{\"chat\":true,\"tool_calling\":true,\"usage_reporting\":true,\"embeddings\":true}') RETURNING id",
    )
    .bind(t)
    .fetch_one(pool)
    .await
    .unwrap();
    sqlx::query("INSERT INTO chatbot_configs (tenant_id, enabled, mode, primary_provider_id, timeout_ms) VALUES ($1, $2, $3, $4, $5)")
        .bind(t)
        .bind(enabled)
        .bind(mode)
        .bind(prov)
        .bind(timeout_ms)
        .execute(pool)
        .await
        .unwrap();
    TenantId::from_uuid(t)
}

fn ask(m: &str) -> TurnInput {
    TurnInput {
        message: m.into(),
        history: vec![],
        customer: None,
    }
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn answers_from_real_catalog_and_meters_the_run(pool: PgPool) {
    let (port, seen) = provider().await;
    let rt = runtime(app_pool(&pool).await, port, "ok", 1_000_000);
    let t = setup(&pool, "hybrid", true, 10_000).await;
    let r = rt
        .answer(t, "Kopi Senja", ask("ada kopi gayo?"), "chat")
        .await
        .unwrap();
    assert_eq!(
        (r.outcome.decision, r.outcome.provider),
        (Decision::SendAuto, ProviderUsed::Primary),
        "{:?}",
        r.outcome
    );
    assert_eq!(
        r.outcome.reply.as_deref(),
        Some("Kopi Gayo tersedia, harga Rp85.000.")
    );
    // Hasil tool berasal dari katalog tenant di DB nyata dan dikirim sebagai pesan role tool.
    let bodies = seen.bodies.lock().unwrap().clone();
    let tool_msg = bodies
        .iter()
        .flat_map(|b| b["messages"].as_array().cloned().unwrap_or_default())
        .find(|m| m["role"] == "tool")
        .unwrap();
    assert!(
        tool_msg["content"]
            .as_str()
            .unwrap()
            .contains("Kopi Gayo 250g")
            && tool_msg["content"].as_str().unwrap().contains("85000")
    );
    let (status, decision, tokens, source, purpose): (String, String, i32, String, String) =
        sqlx::query_as("SELECT status, decision, prompt_tokens + completion_tokens, source, purpose FROM ai_runs WHERE id = $1")
            .bind(r.run_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(
        (
            status.as_str(),
            decision.as_str(),
            tokens,
            source.as_str(),
            purpose.as_str()
        ),
        ("ok", "send_auto", 735, "platform", "chat")
    );
    let calls: Vec<(String, String)> =
        sqlx::query_as("SELECT tool_name, result_status FROM ai_tool_calls WHERE ai_run_id = $1")
            .bind(r.run_id)
            .fetch_all(&pool)
            .await
            .unwrap();
    assert_eq!(calls, vec![("search_products".to_owned(), "ok".to_owned())]);
    assert_eq!(
        rt.month_usage(t, umkm_domain::ai::ProviderSource::Platform)
            .await
            .unwrap(),
        735
    );
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn quota_exhausted_hands_over_without_calling_provider(pool: PgPool) {
    let (port, seen) = provider().await;
    let rt = runtime(app_pool(&pool).await, port, "ok", 700);
    let t = setup(&pool, "auto", true, 10_000).await;
    let first = rt
        .answer(t, "Kopi Senja", ask("ada kopi?"), "chat")
        .await
        .unwrap();
    assert_eq!(first.outcome.status, "ok");
    let hits = seen.hits.load(Ordering::SeqCst);
    let second = rt
        .answer(t, "Kopi Senja", ask("ada kopi lagi?"), "chat")
        .await
        .unwrap();
    assert_eq!(
        (second.outcome.status, second.outcome.decision),
        ("quota_exceeded", Decision::Handover)
    );
    assert_eq!(
        second.outcome.reply.as_deref(),
        Some("Maaf, pertanyaan Anda akan kami teruskan ke tim kami.")
    );
    assert_eq!(
        seen.hits.load(Ordering::SeqCst),
        hits,
        "provider tidak dipanggil setelah kuota habis"
    );
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn breaker_opens_after_repeated_failures_and_slow_provider_is_cut_off(pool: PgPool) {
    let (port, seen) = provider().await;
    let rt = runtime(app_pool(&pool).await, port, "down", 1_000_000);
    let t = setup(&pool, "hybrid", true, 10_000).await;
    for _ in 0..BREAKER_THRESHOLD {
        let r = rt
            .answer(t, "Kopi Senja", ask("halo"), "chat")
            .await
            .unwrap();
        assert_eq!(
            (r.outcome.status, r.outcome.error_code, r.outcome.decision),
            ("error", Some("upstream"), Decision::Handover)
        );
    }
    let hits = seen.hits.load(Ordering::SeqCst);
    // Embedding retrieval + chat per giliran: provider dihubungi, lalu breaker terbuka.
    assert!(hits >= BREAKER_THRESHOLD as usize);
    let r = rt
        .answer(t, "Kopi Senja", ask("halo"), "chat")
        .await
        .unwrap();
    assert_eq!(
        (r.outcome.status, r.outcome.decision),
        ("circuit_open", Decision::Handover)
    );
    assert_eq!(
        seen.hits.load(Ordering::SeqCst),
        hits,
        "breaker terbuka: tanpa panggilan"
    );
    // Provider lambat: dipotong timeout config (1 s), pemanggil tidak tertahan 3 s.
    let slow = runtime(app_pool(&pool).await, port, "slow", 1_000_000);
    let t2 = setup(&pool, "hybrid", true, 1_000).await;
    let t0 = Instant::now();
    let r = slow
        .answer(t2, "Kopi Senja", ask("halo"), "chat")
        .await
        .unwrap();
    assert!(
        t0.elapsed() < Duration::from_millis(2_800),
        "{:?}",
        t0.elapsed()
    );
    assert_eq!(
        (r.outcome.status, r.outcome.decision),
        ("timeout", Decision::Handover)
    );
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn human_only_and_disabled_chatbot(pool: PgPool) {
    let (port, seen) = provider().await;
    let rt = runtime(app_pool(&pool).await, port, "ok", 1_000_000);
    let t = setup(&pool, "human_only", true, 10_000).await;
    let r = rt
        .answer(t, "Kopi Senja", ask("ada kopi?"), "chat")
        .await
        .unwrap();
    assert_eq!(
        (r.outcome.decision, r.outcome.reply.clone()),
        (Decision::HumanQueue, None)
    );
    assert_eq!(seen.hits.load(Ordering::SeqCst), 0);
    let d: String = sqlx::query_scalar("SELECT decision FROM ai_runs WHERE id = $1")
        .bind(r.run_id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(d, "human_queue");
    let off = setup(&pool, "hybrid", false, 10_000).await;
    assert!(matches!(
        rt.answer(off, "Kopi Senja", ask("halo"), "chat").await,
        Err(AiRunError::Disabled)
    ));
    // Uji coba admin tetap jalan saat nonaktif, dicatat sebagai purpose test.
    let r = rt
        .answer(off, "Kopi Senja", ask("ada kopi?"), "test")
        .await
        .unwrap();
    let p: String = sqlx::query_scalar("SELECT purpose FROM ai_runs WHERE id = $1")
        .bind(r.run_id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(p, "test");
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn worker_indexes_pending_knowledge_and_answers_use_it(pool: PgPool) {
    let (port, seen) = provider().await;
    let rt = runtime(app_pool(&pool).await, port, "ok", 1_000_000);
    let t = setup(&pool, "hybrid", true, 10_000).await;
    sqlx::query("INSERT INTO knowledge_documents (tenant_id, title, body) VALUES ($1, 'Pengiriman', 'Pengiriman ke Bandung memakan 2 hari kerja.')")
        .bind(t.as_uuid())
        .execute(&pool)
        .await
        .unwrap();
    // Tenant tanpa provider: dokumennya gagal dengan kode jelas, tidak menghentikan antrean.
    let lonely: Uuid = sqlx::query_scalar(
        "INSERT INTO tenants (name, slug) VALUES ('X', 'tanpa-ai') RETURNING id",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO knowledge_documents (tenant_id, title, body) VALUES ($1, 'Doc', 'Isi')",
    )
    .bind(lonely)
    .execute(&pool)
    .await
    .unwrap();
    assert_eq!(rt.index_pending(10).await.unwrap(), (1, 1));
    let st: Vec<(String, Option<String>, i32)> = sqlx::query_as(
        "SELECT status, error_code, chunk_count FROM knowledge_documents ORDER BY title DESC",
    )
    .fetch_all(&pool)
    .await
    .unwrap();
    assert_eq!(
        st,
        vec![
            ("indexed".into(), None, 1),
            ("failed".into(), Some("no_embedding_provider".into()), 0)
        ]
    );
    assert_eq!(
        rt.index_pending(10).await.unwrap(),
        (0, 0),
        "tidak diproses ulang"
    );
    // Pertanyaan berikutnya mendapat potongan itu sebagai KONTEKS di system prompt.
    seen.bodies.lock().unwrap().clear();
    rt.answer(
        t,
        "Kopi Senja",
        ask("berapa lama kirim ke bandung?"),
        "chat",
    )
    .await
    .unwrap();
    let sys = seen
        .bodies
        .lock()
        .unwrap()
        .iter()
        .find_map(|b| {
            b["messages"]
                .as_array()
                .map(|m| m[0]["content"].as_str().unwrap().to_owned())
        })
        .unwrap();
    assert!(
        sys.contains("<<<KONTEKS") && sys.contains("2 hari kerja"),
        "{sys}"
    );
}

/// AC-SUB-002: setiap panggilan AI menyimpan token input/output DAN estimasi biaya.
#[sqlx::test(migrator = "MIGRATOR")]
async fn every_run_stores_tokens_and_cost_estimate(pool: PgPool) {
    use umkm_application::ai::llm::Usage;
    use umkm_domain::ai::ProviderSource;
    use umkm_infrastructure::ai::runtime::estimate_cost_idr;

    let (port, _) = provider().await;
    let mut rt = runtime(app_pool(&pool).await, port, "ok", 1_000_000);
    if let Some(p) = rt.platform.as_mut() {
        p.input_cost_per_million_idr = 15_000.0;
        p.output_cost_per_million_idr = 60_000.0;
    }
    let t = setup(&pool, "hybrid", true, 10_000).await;
    rt.answer(t, "Kopi Senja", ask("ada kopi gayo?"), "chat")
        .await
        .unwrap();
    let (pt, ct, cost): (i32, i32, Option<f64>) = sqlx::query_as(
        "SELECT prompt_tokens, completion_tokens, estimated_cost_idr::float8 FROM ai_runs WHERE tenant_id = $1",
    )
    .bind(t.as_uuid())
    .fetch_one(&pool)
    .await
    .unwrap();
    assert!(pt > 0 && ct > 0, "token input/output tersimpan");
    let want = (f64::from(pt) * 15_000.0 + f64::from(ct) * 60_000.0) / 1_000_000.0;
    assert!((cost.unwrap() - want).abs() < 0.0001, "{cost:?} vs {want}");

    // Harga tidak diketahui / BYOK → None (tidak dianggap gratis).
    let u = Usage {
        prompt_tokens: 1000,
        completion_tokens: 100,
    };
    let mut p = rt.platform.clone().unwrap();
    assert_eq!(estimate_cost_idr(Some(&p), ProviderSource::Byok, &u), None);
    assert_eq!(estimate_cost_idr(None, ProviderSource::Platform, &u), None);
    p.input_cost_per_million_idr = 0.0;
    p.output_cost_per_million_idr = 0.0;
    assert_eq!(
        estimate_cost_idr(Some(&p), ProviderSource::Platform, &u),
        None
    );
    p.input_cost_per_million_idr = 10_000.0;
    assert_eq!(
        estimate_cost_idr(Some(&p), ProviderSource::Platform, &u),
        Some(10.0)
    );
}
