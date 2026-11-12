//! P5-4: adapter OpenAI-compatible terhadap provider tiruan (server HTTP lokal nyata): matriks capability
//! pada body yang benar-benar diterima provider, pemetaan error, embedding, dan jalur SSRF.

use std::{
    net::SocketAddr,
    sync::{Arc, Mutex},
    time::Duration,
};

use axum::{
    Json, Router,
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
    routing::post,
};
use serde_json::{Value, json};
use umkm_application::ai::llm::{ChatMessage, ChatRequest, LlmError, LlmPort, Role, ToolSpec};
use umkm_domain::ai::Capabilities;
use umkm_infrastructure::{
    ai::provider_adapter::{ApiKey, OpenAiCompatible},
    http_client::{Egress, EgressPolicy, Resolver},
};

type Log = Arc<Mutex<Vec<(String, Value, Option<String>)>>>;

async fn provider() -> (SocketAddr, Log) {
    let log: Log = Arc::default();
    async fn handle(
        State(log): State<Log>,
        Path((mode, rest)): Path<(String, String)>,
        h: HeaderMap,
        Json(body): Json<Value>,
    ) -> axum::response::Response {
        let auth = h
            .get("authorization")
            .map(|v| v.to_str().unwrap().to_owned());
        log.lock()
            .unwrap()
            .push((format!("{mode}/{rest}"), body.clone(), auth));
        match (mode.as_str(), rest.as_str()) {
            ("ok", "chat/completions") => Json(json!({"model": "m-1", "choices": [{"message": {"content": "Stok kopi ada 3",
                "tool_calls": [{"id": "c1", "type": "function", "function": {"name": "check_stock", "arguments": "{\"sku\":\"K1\"}"}}]}}],
                "usage": {"prompt_tokens": 40, "completion_tokens": 7}})).into_response(),
            ("ok", "embeddings") => Json(json!({"data": [
                {"index": 1, "embedding": [0.0, 1.0]}, {"index": 0, "embedding": [1.0, 0.0]}]})).into_response(),
            ("e401", _) => StatusCode::UNAUTHORIZED.into_response(),
            ("e429", _) => StatusCode::TOO_MANY_REQUESTS.into_response(),
            ("e500", _) => StatusCode::BAD_GATEWAY.into_response(),
            _ => "bukan json".into_response(),
        }
    }
    let app = Router::new()
        .route("/{mode}/v1/{*rest}", post(handle))
        .with_state(log.clone());
    let l = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = l.local_addr().unwrap();
    tokio::spawn(async move { axum::serve(l, app).await.unwrap() });
    (addr, log)
}

fn req() -> ChatRequest {
    ChatRequest {
        model: "m".into(),
        messages: vec![
            ChatMessage::new(Role::System, "Anda asisten toko"),
            ChatMessage::new(Role::User, "stok kopi?"),
        ],
        temperature: 0.2,
        max_output_tokens: 128,
        tools: vec![ToolSpec {
            name: "check_stock",
            description: "Cek stok",
            parameters: json!({"type": "object"}),
        }],
    }
}

fn adapter(
    addr: SocketAddr,
    mode: &str,
    caps: Capabilities,
    policy: EgressPolicy,
) -> OpenAiCompatible {
    OpenAiCompatible::new(
        Egress::new(policy, Resolver::System, 4),
        &format!("http://127.0.0.1:{}/{mode}/v1/", addr.port()),
        Some(ApiKey::new("sk-uji-123".into())),
        caps,
    )
}

fn op() -> EgressPolicy {
    EgressPolicy::operator(Duration::from_secs(3))
}

#[tokio::test]
async fn capability_matrix_on_the_wire() {
    let (addr, log) = provider().await;
    let full = adapter(addr, "ok", Capabilities::defaults_for("openai"), op());
    let r = full.chat(req()).await.unwrap();
    assert_eq!(r.content.as_deref(), Some("Stok kopi ada 3"));
    assert_eq!(
        (r.tool_calls.len(), r.usage.unwrap().prompt_tokens),
        (1, 40)
    );
    let chat_only = adapter(addr, "ok", Capabilities::defaults_for("custom"), op());
    let r = chat_only.chat(req()).await.unwrap();
    assert!(
        r.tool_calls.is_empty(),
        "tool call dari provider tanpa capability diabaikan"
    );
    let log = log.lock().unwrap();
    assert_eq!(log[0].1["tools"][0]["function"]["name"], "check_stock");
    assert!(log[1].1.get("tools").is_none(), "AC-AIC-002: {}", log[1].1);
    assert_eq!(log[0].2.as_deref(), Some("Bearer sk-uji-123"));
    assert_eq!(
        (
            log[0].1["max_tokens"].as_u64(),
            log[0].1["stream"].as_bool()
        ),
        (Some(128), Some(false))
    );
}

#[tokio::test]
async fn errors_are_normalized_and_embeddings_ordered() {
    let (addr, _) = provider().await;
    let caps = Capabilities::defaults_for("openai");
    for (mode, want) in [
        ("e401", LlmError::Unauthorized),
        ("e429", LlmError::RateLimited),
        ("e500", LlmError::Upstream),
        ("garbage", LlmError::InvalidResponse),
    ] {
        assert_eq!(
            adapter(addr, mode, caps, op())
                .chat(req())
                .await
                .unwrap_err(),
            want,
            "{mode}"
        );
    }
    assert!(LlmError::RateLimited.is_transient() && !LlmError::Unauthorized.is_transient());
    let e = adapter(addr, "ok", caps, op())
        .embed("emb", vec!["a".into(), "b".into()])
        .await
        .unwrap();
    assert_eq!(
        e,
        vec![vec![1.0, 0.0], vec![0.0, 1.0]],
        "diurutkan menurut index"
    );
    let no_emb = adapter(addr, "ok", Capabilities::defaults_for("groq"), op());
    assert_eq!(
        no_emb.embed("emb", vec!["a".into()]).await.unwrap_err(),
        LlmError::Unsupported
    );
}

#[tokio::test]
async fn tenant_endpoint_to_local_network_is_blocked_before_any_request() {
    let (addr, log) = provider().await;
    let a = adapter(
        addr,
        "ok",
        Capabilities::defaults_for("openai"),
        EgressPolicy::tenant(Duration::from_secs(3)),
    );
    assert_eq!(
        a.chat(req()).await.unwrap_err(),
        LlmError::Network("scheme_not_allowed")
    );
    assert!(log.lock().unwrap().is_empty());
    assert!(!format!("{a:?}").contains("sk-uji"));
}
