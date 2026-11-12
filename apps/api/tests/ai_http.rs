//! P5-2: pengaturan AI lewat HTTP — kunci BYOK tidak pernah keluar (AC-AIC-003: respons, list,
//! audit, DB), rotasi/nonaktif, SSRF saat simpan, batas parameter (AC-AIC-001), provider platform,
//! izin, isolasi tenant, tool.

mod common;

use axum::http::Method;
use common::*;
use serde_json::{Value, json};
use sqlx::PgPool;
use umkm_database::MIGRATOR;
use uuid::Uuid;

const KEY: &str = "sk-live-SUPERSECRET-abcdef123456";
const KEY2: &str = "sk-live-ROTATED-SECRET-zyxw9876";

async fn call(a: &TestApp, w: &Who, m: Method, t: Uuid, path: &str, body: Option<Value>) -> Res {
    json(
        &a.router,
        m,
        &format!("/api/v1/tenants/{t}/ai{path}"),
        Some(w),
        body,
    )
    .await
}

fn byok(name: &str, url: &str) -> Value {
    json!({ "name": name, "source": "byok", "provider_type": "openai", "base_url": url, "model": "gpt-4o-mini", "api_key": KEY })
}

fn config(extra: Value) -> Value {
    let mut c = json!({
        "enabled": false, "mode": "hybrid", "primary_provider_id": null, "fallback_provider_id": null,
        "temperature": 0.3, "max_output_tokens": 512, "timeout_ms": 20000, "system_prompt": "Ramah dan singkat.",
        "welcome_message": "Halo!", "fallback_message": "Kami teruskan ke tim.", "allowed_topics": ["produk"],
        "restricted_topics": ["politik"], "handover_policy": {}, "operating_schedule": {}, "monthly_token_budget": null
    });
    for (k, v) in extra.as_object().unwrap() {
        c[k] = v.clone();
    }
    c
}

fn field(r: &Res) -> (String, String) {
    let e = &r.json()["error"]["details"]["errors"][0];
    (
        e["path"].as_str().unwrap_or_default().into(),
        e["code"].as_str().unwrap_or_default().into(),
    )
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn byok_key_never_leaves_and_can_be_rotated_and_disabled(pool: PgPool) {
    let a = app(&pool).await;
    let t = tenant(&pool, "toko-ai").await;
    let owner = member(&pool, t, "Owner").await;
    let r = call(
        &a,
        &owner,
        Method::POST,
        t,
        "/providers",
        Some(byok("OpenAI saya", "https://api.openai.com/v1/")),
    )
    .await;
    assert_eq!(r.status, 201, "{}", r.json());
    let p = r.json()["data"].clone();
    assert_eq!(
        (p["key_hint"].as_str(), p["has_key"].as_bool()),
        (Some("••••3456"), Some(true))
    );
    assert_eq!(p["base_url"], "https://api.openai.com/v1");
    assert_eq!(p["capabilities"]["tool_calling"], true);
    let pid = p["id"].as_str().unwrap().to_owned();
    let list = call(&a, &owner, Method::GET, t, "/providers", None).await;
    assert_eq!(list.status, 200);
    // Rotasi + nonaktif.
    let r = call(
        &a,
        &owner,
        Method::PATCH,
        t,
        &format!("/providers/{pid}"),
        Some(json!({ "api_key": KEY2, "active": false })),
    )
    .await;
    assert_eq!(r.status, 200, "{}", r.json());
    assert_eq!(
        (
            r.json()["data"]["key_hint"].as_str(),
            r.json()["data"]["status"].as_str()
        ),
        (Some("••••9876"), Some("disabled"))
    );
    // AC-AIC-003: tidak ada kunci di respons mana pun, audit, maupun kolom DB yang dapat dibaca.
    let audit = call_audit(&a, &owner, t).await;
    let db: String = sqlx::query_scalar(
        "SELECT coalesce(string_agg(row_to_json(p)::text || row_to_json(s)::text, ''), '') FROM ai_providers p
         JOIN ai_provider_secrets s ON s.provider_id = p.id",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    for (what, text) in [
        ("create", p.to_string()),
        ("list", list.json().to_string()),
        ("patch", r.json().to_string()),
        ("audit", audit.clone()),
        ("db", db),
    ] {
        for secret in ["SUPERSECRET", "ROTATED-SECRET", "abcdef123456"] {
            assert!(!text.contains(secret), "{what} membocorkan kunci");
        }
    }
    for action in [
        "ai.provider_created",
        "ai.key_rotated",
        "ai.provider_updated",
    ] {
        assert!(audit.contains(action), "audit {action}");
    }
    // Rahasia di DB dapat dibuka dengan KEK dan AAD (tenant, provider) — kunci terbaru.
    let k = umkm_infrastructure::crypto::Keyring::parse(TEST_KEK).unwrap();
    let key = umkm_infrastructure::ai_repo::load_key(
        &a.state.db.clone().unwrap(),
        umkm_shared::ids::TenantId::from_uuid(t),
        pid.parse().unwrap(),
        &k,
    )
    .await
    .unwrap()
    .unwrap();
    assert_eq!(key.expose_str().unwrap(), KEY2);
}

async fn call_audit(a: &TestApp, w: &Who, t: Uuid) -> String {
    let r = json(
        &a.router,
        Method::GET,
        &format!("/api/v1/tenants/{t}/audit-logs?limit=100"),
        Some(w),
        None,
    )
    .await;
    assert_eq!(r.status, 200);
    r.json().to_string()
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn unsafe_endpoints_and_bad_keys_are_rejected_on_save(pool: PgPool) {
    let a = app(&pool).await;
    let t = tenant(&pool, "toko-ai").await;
    let owner = member(&pool, t, "Owner").await;
    for (url, code) in [
        ("http://api.example.com/v1", "scheme_not_allowed"),
        ("https://127.0.0.1/v1", "forbidden_address"),
        ("https://169.254.169.254/latest", "forbidden_address"),
        ("https://localhost:11434/v1", "forbidden_host"),
        ("https://ollama.internal/v1", "forbidden_host"),
        ("https://u:p@api.example.com/v1", "userinfo_in_url"),
    ] {
        let r = call(
            &a,
            &owner,
            Method::POST,
            t,
            "/providers",
            Some(byok("x", url)),
        )
        .await;
        assert_eq!(
            (r.status, field(&r)),
            (422, ("base_url".into(), code.into())),
            "{url}"
        );
    }
    let mut no_key = byok("x", "https://api.example.com/v1");
    no_key["api_key"] = json!("short");
    assert_eq!(
        field(&call(&a, &owner, Method::POST, t, "/providers", Some(no_key)).await).1,
        "invalid_api_key"
    );
    let mut bad_type = byok("x", "https://api.example.com/v1");
    bad_type["provider_type"] = json!("platform");
    assert_eq!(
        field(&call(&a, &owner, Method::POST, t, "/providers", Some(bad_type)).await).1,
        "invalid_provider_type"
    );
    let n: i64 = sqlx::query_scalar("SELECT count(*) FROM ai_providers")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(n, 0);
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn config_bounds_are_enforced_ac_aic_001(pool: PgPool) {
    let a = app(&pool).await;
    let t = tenant(&pool, "toko-ai").await;
    let owner = member(&pool, t, "Owner").await;
    let get = call(&a, &owner, Method::GET, t, "/config", None).await;
    assert_eq!(get.status, 200);
    assert_eq!(
        get.json()["data"]["config"]["mode"],
        "hybrid",
        "default Hybrid"
    );
    assert_eq!(
        get.json()["data"]["platform"]["models"][0],
        "platform-small"
    );
    for (extra, path) in [
        (json!({"temperature": 2.5}), "temperature"),
        (json!({"temperature": -1}), "temperature"),
        (json!({"max_output_tokens": 5000}), "max_output_tokens"),
        (json!({"max_output_tokens": 0}), "max_output_tokens"),
        (json!({"timeout_ms": 100}), "timeout_ms"),
        (json!({"timeout_ms": 120000}), "timeout_ms"),
        (json!({"mode": "yolo"}), "mode"),
        (json!({"system_prompt": "a".repeat(4001)}), "system_prompt"),
        (json!({"fallback_message": "  "}), "fallback_message"),
        (json!({"enabled": true}), "primary_provider_id"),
        (json!({"handover_policy": []}), "handover_policy"),
        (json!({"monthly_token_budget": 0}), "monthly_token_budget"),
    ] {
        let r = call(
            &a,
            &owner,
            Method::PUT,
            t,
            "/config",
            Some(config(extra.clone())),
        )
        .await;
        assert_eq!((r.status, field(&r).0), (422, path.to_owned()), "{extra}");
    }
    let ok = call(&a, &owner, Method::PUT, t, "/config", Some(config(json!({"mode": "human_only", "enabled": true, "temperature": 2.0, "max_output_tokens": 4096, "timeout_ms": 60000})))).await;
    assert_eq!(ok.status, 200, "{}", ok.json());
    assert_eq!(
        (
            ok.json()["data"]["config"]["mode"].as_str(),
            ok.json()["data"]["config"]["temperature"].as_f64()
        ),
        (Some("human_only"), Some(2.0))
    );
    // Prompt tidak masuk audit (hanya panjang).
    let audit = call_audit(&a, &owner, t).await;
    assert!(audit.contains("ai.config_updated") && !audit.contains("Ramah dan singkat"));
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn platform_provider_permissions_isolation_and_tools(pool: PgPool) {
    let a = app(&pool).await;
    let (t, u) = (
        tenant(&pool, "toko-ai").await,
        tenant(&pool, "toko-lain").await,
    );
    let owner = member(&pool, t, "Owner").await;
    let other = member(&pool, u, "Owner").await;
    let manager = member(&pool, t, "Manager").await;
    // Platform: URL/kunci tidak boleh diberikan; hanya model yang ditawarkan operator.
    let plat = |model: &str| json!({ "name": "AI Platform", "source": "platform", "model": model });
    let r = call(
        &a,
        &owner,
        Method::POST,
        t,
        "/providers",
        Some(plat("gpt-5")),
    )
    .await;
    assert_eq!(field(&r), ("model".into(), "model_not_offered".into()));
    let mut with_url = plat("platform-small");
    with_url["base_url"] = json!("https://x.example.com");
    assert_eq!(
        call(&a, &owner, Method::POST, t, "/providers", Some(with_url))
            .await
            .status,
        422
    );
    let r = call(
        &a,
        &owner,
        Method::POST,
        t,
        "/providers",
        Some(plat("platform-small")),
    )
    .await;
    assert_eq!(r.status, 201, "{}", r.json());
    assert_eq!(
        (
            r.json()["data"]["base_url"].clone(),
            r.json()["data"]["has_key"].clone()
        ),
        (Value::Null, json!(false))
    );
    let pid = r.json()["data"]["id"].as_str().unwrap().to_owned();
    assert_eq!(
        call(
            &a,
            &owner,
            Method::POST,
            t,
            "/providers",
            Some(plat("platform-large"))
        )
        .await
        .status,
        409,
        "nama ganda"
    );
    // Manager: boleh baca, tidak boleh ubah.
    assert_eq!(
        call(&a, &manager, Method::GET, t, "/config", None)
            .await
            .status,
        200
    );
    assert_eq!(
        call(&a, &manager, Method::GET, t, "/providers", None)
            .await
            .status,
        200
    );
    assert_eq!(
        call(
            &a,
            &manager,
            Method::PUT,
            t,
            "/config",
            Some(config(json!({})))
        )
        .await
        .status,
        403
    );
    assert_eq!(
        call(
            &a,
            &manager,
            Method::POST,
            t,
            "/providers",
            Some(plat("platform-small"))
        )
        .await
        .status,
        403
    );
    // Tenant lain: provider ini tidak terlihat, tidak dapat diubah atau dirujuk.
    assert_eq!(
        call(
            &a,
            &other,
            Method::PATCH,
            u,
            &format!("/providers/{pid}"),
            Some(json!({"active": false}))
        )
        .await
        .status,
        404
    );
    let r = call(
        &a,
        &other,
        Method::PUT,
        u,
        "/config",
        Some(config(json!({"enabled": true, "primary_provider_id": pid}))),
    )
    .await;
    assert_eq!(
        field(&r),
        (
            "primary_provider_id".into(),
            "provider_not_available".into()
        )
    );
    assert_eq!(
        call(&a, &owner, Method::GET, t, "/providers", None)
            .await
            .json()["data"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    assert_eq!(
        call(&a, &other, Method::GET, u, "/providers", None)
            .await
            .json()["data"]
            .as_array()
            .unwrap()
            .len(),
        0
    );
    // Aktifkan chatbot dengan provider sendiri; provider yang dipakai tidak dapat dihapus.
    let r = call(
        &a,
        &owner,
        Method::PUT,
        t,
        "/config",
        Some(config(json!({"enabled": true, "primary_provider_id": pid}))),
    )
    .await;
    assert_eq!(r.status, 200, "{}", r.json());
    assert_eq!(
        call(
            &a,
            &owner,
            Method::DELETE,
            t,
            &format!("/providers/{pid}"),
            None
        )
        .await
        .status,
        409
    );
    // Tool: nama tak dikenal ditolak; default create_order_draft nonaktif lalu diaktifkan.
    let r = call(
        &a,
        &owner,
        Method::PUT,
        t,
        "/tools",
        Some(json!({"tools": [{"name": "run_sql", "enabled": true}]})),
    )
    .await;
    assert_eq!(r.status, 422);
    let draft = |v: &Value| {
        v.as_array()
            .unwrap()
            .iter()
            .find(|x| x["name"] == "create_order_draft")
            .unwrap()["enabled"]
            .clone()
    };
    assert_eq!(
        draft(
            &call(&a, &owner, Method::GET, t, "/config", None)
                .await
                .json()["data"]["tools"]
        ),
        json!(false)
    );
    let r = call(
        &a,
        &owner,
        Method::PUT,
        t,
        "/tools",
        Some(json!({"tools": [{"name": "create_order_draft", "enabled": true}]})),
    )
    .await;
    assert_eq!((r.status, draft(&r.json()["data"])), (200, json!(true)));
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn test_turn_usage_and_knowledge_endpoints(pool: PgPool) {
    let a = app(&pool).await;
    let t = tenant(&pool, "toko-ai").await;
    let owner = member(&pool, t, "Owner").await;
    let manager = member(&pool, t, "Manager").await;
    // Tanpa provider utama → 409 jelas.
    let r = call(
        &a,
        &owner,
        Method::POST,
        t,
        "/test",
        Some(json!({"message": "halo"})),
    )
    .await;
    assert_eq!(
        (r.status, r.json()["error"]["details"]["reason"].as_str()),
        (409, Some("ai_not_configured")),
        "{}",
        r.json()
    );
    let p = call(
        &a,
        &owner,
        Method::POST,
        t,
        "/providers",
        Some(json!({"name": "P", "source": "platform", "model": "platform-small"})),
    )
    .await;
    let pid = p.json()["data"]["id"].as_str().unwrap().to_owned();
    assert_eq!(
        call(
            &a,
            &owner,
            Method::PUT,
            t,
            "/config",
            Some(config(json!({"primary_provider_id": pid})))
        )
        .await
        .status,
        200
    );
    // Endpoint platform uji tidak dapat dihubungi (DNS) → giliran tetap selesai cepat sebagai handover, bukan 5xx.
    let r = call(
        &a,
        &owner,
        Method::POST,
        t,
        "/test",
        Some(json!({"message": "ada kopi?"})),
    )
    .await;
    assert_eq!(r.status, 200, "{}", r.json());
    let d = &r.json()["data"];
    assert_eq!(
        (d["decision"].as_str(), d["status"].as_str()),
        (Some("handover"), Some("error")),
        "{d}"
    );
    assert_eq!(d["reply"], "Kami teruskan ke tim.");
    assert_eq!(
        call(
            &a,
            &manager,
            Method::POST,
            t,
            "/test",
            Some(json!({"message": "x"}))
        )
        .await
        .status,
        403
    );
    let u = call(&a, &manager, Method::GET, t, "/usage", None).await;
    assert_eq!(
        (u.status, u.json()["data"]["platform_quota"].as_u64()),
        (200, Some(200_000))
    );
    let runs: i64 = sqlx::query_scalar("SELECT count(*) FROM ai_runs WHERE purpose = 'test'")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(runs, 1);
    // Basis pengetahuan.
    assert_eq!(
        call(
            &a,
            &owner,
            Method::POST,
            t,
            "/knowledge",
            Some(json!({"title": " ", "body": "x"}))
        )
        .await
        .status,
        422
    );
    let r = call(
        &a,
        &owner,
        Method::POST,
        t,
        "/knowledge",
        Some(json!({"title": "Ongkir", "body": "Gratis di atas 100rb."})),
    )
    .await;
    assert_eq!(r.status, 201);
    let doc = r.json()["data"].as_str().unwrap().to_owned();
    let list = call(&a, &manager, Method::GET, t, "/knowledge", None).await;
    assert_eq!(list.json()["data"][0]["status"], "pending");
    assert_eq!(
        call(
            &a,
            &manager,
            Method::POST,
            t,
            "/knowledge",
            Some(json!({"title": "x", "body": "y"}))
        )
        .await
        .status,
        403
    );
    assert_eq!(
        call(
            &a,
            &owner,
            Method::POST,
            t,
            &format!("/knowledge/{doc}/reindex"),
            None
        )
        .await
        .status,
        202
    );
    assert_eq!(
        call(
            &a,
            &owner,
            Method::DELETE,
            t,
            &format!("/knowledge/{doc}"),
            None
        )
        .await
        .status,
        204
    );
    assert_eq!(
        call(
            &a,
            &owner,
            Method::DELETE,
            t,
            &format!("/knowledge/{doc}"),
            None
        )
        .await
        .status,
        404
    );
}
