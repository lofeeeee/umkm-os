//! P1-3 HTTP: register/login/session/logout lewat router nyata + Postgres + Redis.
//! Butuh `eval "$(scripts/dev-db.sh up)"` (DATABASE_URL + REDIS_URL).

use std::{sync::Arc, time::Duration};

use axum::{
    Router,
    body::Body,
    http::{HeaderMap, Method, Request, StatusCode, header},
};
use http_body_util::BodyExt;
use serde_json::{Value, json};
use sqlx::PgPool;
use tower::ServiceExt;
use umkm_api::{AppState, HttpLimits, auth::AuthService, build_router};
use umkm_application::onboarding::LegalVersions;
use umkm_database::MIGRATOR;
use umkm_domain::auth::AuthPolicy;
use umkm_infrastructure::{
    auth::Argon2idHasher,
    rate_limit::RateLimiter,
    redis::{KeyNamespace, RedisClient},
};
use umkm_shared::config::SecretString;

const LIMITS: HttpLimits = HttpLimits {
    request_timeout: Duration::from_secs(10),
    body_limit_bytes: 16 * 1024,
};
const APP: &str = "https://app.ryasai.my.id";

fn service(pool: PgPool, redis_url: &str, secure: bool) -> Arc<AuthService> {
    let client = RedisClient::open(&SecretString::new(redis_url)).unwrap();
    let ns = KeyNamespace::new(&format!("umkmtest:{}", uuid::Uuid::new_v4().simple())).unwrap();
    Arc::new(AuthService {
        pool,
        limiter: RateLimiter::new(client.connection_manager_lazy().unwrap(), ns),
        hasher: Arc::new(Argon2idHasher::new(1024, 1, 1).unwrap()),
        policy: AuthPolicy::default(),
        legal: LegalVersions {
            privacy_policy: "placeholder-v0".into(),
            terms_of_service: "placeholder-v0".into(),
        },
        root_domain: "ryasai.my.id".into(),
        secure_cookies: secure,
        allowed_origins: vec![APP.into()],
        trust_forwarded_for: false,
        keyring: None,
    })
}

fn app(pool: PgPool) -> Router {
    let url = std::env::var("REDIS_URL").expect("REDIS_URL wajib");
    build_router(
        AppState::default().with_auth(service(pool, &url, true)),
        LIMITS,
    )
}

struct Res {
    status: u16,
    headers: HeaderMap,
    body: Value,
}

async fn send(
    app: &Router,
    method: Method,
    uri: &str,
    body: Option<Value>,
    headers: &[(&str, &str)],
) -> Res {
    let mut b = Request::builder().method(method).uri(uri);
    for (k, v) in headers {
        b = b.header(*k, *v);
    }
    let req = match body {
        Some(v) => b
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(v.to_string())),
        None => b.body(Body::empty()),
    }
    .unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    let status = res.status().as_u16();
    let headers = res.headers().clone();
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    Res {
        status,
        headers,
        body: serde_json::from_slice(&bytes).unwrap_or(Value::Null),
    }
}

fn reg_body(email: &str, name: &str) -> Value {
    json!({"email": email, "password": "rahasia-123", "business_name": name, "accept_terms": true, "accept_privacy": true})
}

fn cookie_pair(res: &Res) -> String {
    let set = res.headers[header::SET_COOKIE].to_str().unwrap();
    set.split(';').next().unwrap().to_owned()
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn register_sets_secure_host_only_cookie_and_returns_tenant(pool: PgPool) {
    let app = app(pool);
    let r = send(
        &app,
        Method::POST,
        "/api/v1/auth/register",
        Some(reg_body("budi@x.id", "Toko Budi")),
        &[("origin", APP)],
    )
    .await;
    assert_eq!(r.status, 201, "{}", r.body);
    assert_eq!(
        r.body["data"]["tenant"]["hostname"],
        "toko-budi.ryasai.my.id"
    );
    assert_eq!(r.body["data"]["csrf_token"].as_str().unwrap().len(), 43);

    let set = r.headers[header::SET_COOKIE].to_str().unwrap();
    assert!(set.starts_with("__Host-umkm_session="), "{set}");
    for attr in [
        "Path=/",
        "HttpOnly",
        "SameSite=Lax",
        "Secure",
        "Max-Age=1209600",
    ] {
        assert!(set.contains(attr), "atribut {attr} hilang: {set}");
    }
    assert!(
        !set.to_lowercase().contains("domain="),
        "cookie harus host-only: {set}"
    );
    // Token sesi tidak pernah muncul di body
    let token = cookie_pair(&r).split_once('=').unwrap().1.to_owned();
    assert!(!r.body.to_string().contains(&token));
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn session_then_logout_requires_csrf_and_revokes(pool: PgPool) {
    let app = app(pool);
    let r = send(
        &app,
        Method::POST,
        "/api/v1/auth/register",
        Some(reg_body("budi@x.id", "Toko Budi")),
        &[],
    )
    .await;
    let cookie = cookie_pair(&r);
    let csrf = r.body["data"]["csrf_token"].as_str().unwrap().to_owned();

    let s = send(
        &app,
        Method::GET,
        "/api/v1/auth/session",
        None,
        &[("cookie", &cookie)],
    )
    .await;
    assert_eq!(s.status, 200);
    assert_eq!(s.body["data"]["csrf_token"], csrf.as_str());
    // Merchant biasa bukan staf platform → dashboard tidak memeriksa /platform/me.
    assert_eq!(s.body["data"]["platform_staff"], false);

    // Tanpa / dengan CSRF salah -> 403, sesi tetap hidup
    assert_eq!(
        send(
            &app,
            Method::POST,
            "/api/v1/auth/logout",
            None,
            &[("cookie", &cookie)]
        )
        .await
        .status,
        403
    );
    assert_eq!(
        send(
            &app,
            Method::POST,
            "/api/v1/auth/logout",
            None,
            &[("cookie", &cookie), ("x-csrf-token", "salah")]
        )
        .await
        .status,
        403
    );
    assert_eq!(
        send(
            &app,
            Method::GET,
            "/api/v1/auth/session",
            None,
            &[("cookie", &cookie)]
        )
        .await
        .status,
        200
    );

    let out = send(
        &app,
        Method::POST,
        "/api/v1/auth/logout",
        None,
        &[
            ("cookie", &cookie),
            ("x-csrf-token", &csrf),
            ("origin", APP),
        ],
    )
    .await;
    assert_eq!(out.status, 200);
    assert!(
        out.headers[header::SET_COOKIE]
            .to_str()
            .unwrap()
            .contains("Max-Age=0")
    );
    assert_eq!(
        send(
            &app,
            Method::GET,
            "/api/v1/auth/session",
            None,
            &[("cookie", &cookie)]
        )
        .await
        .status,
        401,
        "sesi harus dicabut"
    );
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn foreign_origin_is_rejected(pool: PgPool) {
    let app = app(pool);
    let r = send(
        &app,
        Method::POST,
        "/api/v1/auth/register",
        Some(reg_body("budi@x.id", "Toko Budi")),
        &[("origin", "https://toko-jahat.ryasai.my.id")],
    )
    .await;
    assert_eq!(r.status, 403);
    assert_eq!(r.body["error"]["code"], "FORBIDDEN");
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn login_rotates_session_and_failures_are_indistinguishable(pool: PgPool) {
    let app = app(pool);
    let reg = send(
        &app,
        Method::POST,
        "/api/v1/auth/register",
        Some(reg_body("budi@x.id", "Toko Budi")),
        &[],
    )
    .await;
    let first = cookie_pair(&reg);

    let wrong_pw = send(
        &app,
        Method::POST,
        "/api/v1/auth/login",
        Some(json!({"email": "budi@x.id", "password": "salah-123"})),
        &[],
    )
    .await;
    let no_user = send(
        &app,
        Method::POST,
        "/api/v1/auth/login",
        Some(json!({"email": "nobody@x.id", "password": "salah-123"})),
        &[],
    )
    .await;
    assert_eq!((wrong_pw.status, no_user.status), (401, 401));
    assert_eq!(
        wrong_pw.body["error"]["message"], no_user.body["error"]["message"],
        "pesan harus sama (anti enumeration)"
    );
    assert!(wrong_pw.headers.get(header::SET_COOKIE).is_none());

    let ok = send(
        &app,
        Method::POST,
        "/api/v1/auth/login",
        Some(json!({"email": "BUDI@x.id", "password": "rahasia-123"})),
        &[],
    )
    .await;
    assert_eq!(ok.status, 200, "{}", ok.body);
    let second = cookie_pair(&ok);
    assert_ne!(
        first, second,
        "login harus membuat token sesi baru (rotasi)"
    );
    assert_eq!(
        send(
            &app,
            Method::GET,
            "/api/v1/auth/session",
            None,
            &[("cookie", &second)]
        )
        .await
        .status,
        200
    );
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn validation_and_conflicts_map_to_422_409(pool: PgPool) {
    let app = app(pool);
    let mut no_consent = reg_body("a@x.id", "Toko A");
    no_consent["accept_terms"] = json!(false);
    let r = send(
        &app,
        Method::POST,
        "/api/v1/auth/register",
        Some(no_consent),
        &[],
    )
    .await;
    assert_eq!(
        (r.status, r.body["error"]["details"]["field"].as_str()),
        (422, Some("accept_terms"))
    );

    let mut weak = reg_body("a@x.id", "Toko A");
    weak["password"] = json!("pendek");
    let r = send(&app, Method::POST, "/api/v1/auth/register", Some(weak), &[]).await;
    assert_eq!(
        (r.status, r.body["error"]["details"]["field"].as_str()),
        (422, Some("password"))
    );

    let mut reserved = reg_body("a@x.id", "Toko A");
    reserved["slug"] = json!("admin");
    let r = send(
        &app,
        Method::POST,
        "/api/v1/auth/register",
        Some(reserved),
        &[],
    )
    .await;
    assert_eq!(
        (r.status, r.body["error"]["details"]["field"].as_str()),
        (409, Some("slug"))
    );

    assert_eq!(
        send(
            &app,
            Method::POST,
            "/api/v1/auth/register",
            Some(reg_body("a@x.id", "Toko A")),
            &[]
        )
        .await
        .status,
        201
    );
    let dup = send(
        &app,
        Method::POST,
        "/api/v1/auth/register",
        Some(reg_body("A@X.id", "Toko B")),
        &[],
    )
    .await;
    assert_eq!(
        (dup.status, dup.body["error"]["details"]["field"].as_str()),
        (409, Some("email"))
    );

    let unknown_field = send(
        &app,
        Method::POST,
        "/api/v1/auth/register",
        Some(json!({"email": "x@x.id", "tenant_id": "00000000-0000-0000-0000-000000000000"})),
        &[],
    )
    .await;
    assert!(
        unknown_field.status == 400 || unknown_field.status == 422,
        "{}",
        unknown_field.status
    );
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn register_rate_limited_per_ip_with_retry_after(pool: PgPool) {
    let app = app(pool);
    for i in 0..5 {
        let r = send(
            &app,
            Method::POST,
            "/api/v1/auth/register",
            Some(reg_body(&format!("u{i}@x.id"), "Toko")),
            &[],
        )
        .await;
        assert_eq!(r.status, 201, "{i}: {}", r.body);
    }
    let r = send(
        &app,
        Method::POST,
        "/api/v1/auth/register",
        Some(reg_body("u9@x.id", "Toko")),
        &[],
    )
    .await;
    assert_eq!(r.status, 429);
    assert_eq!(r.body["error"]["code"], "RATE_LIMITED");
    assert!(
        r.headers[header::RETRY_AFTER]
            .to_str()
            .unwrap()
            .parse::<u64>()
            .unwrap()
            > 0
    );
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn redis_down_fails_closed_with_503(pool: PgPool) {
    let app = build_router(
        AppState::default().with_auth(service(pool, "redis://127.0.0.1:1/0", true)),
        LIMITS,
    );
    let r = send(
        &app,
        Method::POST,
        "/api/v1/auth/login",
        Some(json!({"email": "budi@x.id", "password": "rahasia-123"})),
        &[],
    )
    .await;
    assert_eq!(r.status, 503, "{}", r.body);
    let r = send(
        &app,
        Method::POST,
        "/api/v1/auth/register",
        Some(reg_body("budi@x.id", "Toko Budi")),
        &[],
    )
    .await;
    assert_eq!(r.status, 503);
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn cors_allows_only_allowlisted_origin_with_credentials(pool: PgPool) {
    let app = app(pool);
    let pre = |origin: &'static str| {
        Request::builder()
            .method(Method::OPTIONS)
            .uri("/api/v1/auth/login")
            .header("origin", origin)
            .header("access-control-request-method", "POST")
            .header(
                "access-control-request-headers",
                "content-type,x-csrf-token",
            )
            .body(Body::empty())
            .unwrap()
    };
    let ok = app.clone().oneshot(pre(APP)).await.unwrap();
    assert_eq!(ok.headers()["access-control-allow-origin"], APP);
    assert_eq!(ok.headers()["access-control-allow-credentials"], "true");
    let bad = app
        .clone()
        .oneshot(pre("https://evil.example"))
        .await
        .unwrap();
    assert!(bad.headers().get("access-control-allow-origin").is_none());
    // UAT 15 Nov sec #2: preflight origin asing ditolak, tanpa header CORS sama sekali.
    assert_eq!(bad.status(), StatusCode::FORBIDDEN);
    assert!(
        bad.headers()
            .get("access-control-allow-credentials")
            .is_none()
    );
    assert!(bad.headers().get("access-control-allow-methods").is_none());

    // Request biasa dari origin asing: tidak ada ACAC; dari origin allowlist: ada.
    let get = |origin: &'static str| {
        Request::builder()
            .uri("/healthz")
            .header("origin", origin)
            .body(Body::empty())
            .unwrap()
    };
    let foreign = app
        .clone()
        .oneshot(get("https://evil.example"))
        .await
        .unwrap();
    assert!(
        foreign
            .headers()
            .get("access-control-allow-credentials")
            .is_none()
    );
    assert!(
        foreign
            .headers()
            .get("access-control-allow-origin")
            .is_none()
    );
    let own = app.clone().oneshot(get(APP)).await.unwrap();
    assert_eq!(own.headers()["access-control-allow-credentials"], "true");
    // Tanpa header Origin (same-origin/non-browser): tidak ada header CORS.
    let none = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/healthz")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert!(
        none.headers()
            .get("access-control-allow-credentials")
            .is_none()
    );
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn development_cookie_has_no_host_prefix_or_secure(pool: PgPool) {
    let url = std::env::var("REDIS_URL").unwrap();
    let app = build_router(
        AppState::default().with_auth(service(pool, &url, false)),
        LIMITS,
    );
    let r = send(
        &app,
        Method::POST,
        "/api/v1/auth/register",
        Some(reg_body("budi@x.id", "Toko Budi")),
        &[],
    )
    .await;
    let set = r.headers[header::SET_COOKIE].to_str().unwrap();
    assert!(
        set.starts_with("umkm_session=") && !set.contains("Secure"),
        "{set}"
    );
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn verify_email_end_to_end_via_outbox_worker(pool: PgPool) {
    use umkm_infrastructure::{
        metrics::Metrics,
        outbox::{CapturingEmailSender, WorkerConfig, process_once},
    };
    let app = app(pool.clone());
    let r = send(
        &app,
        Method::POST,
        "/api/v1/auth/register",
        Some(reg_body("budi@x.id", "Toko Budi")),
        &[],
    )
    .await;
    assert_eq!(r.status, 201);

    let sender = CapturingEmailSender::default();
    let cfg = WorkerConfig {
        worker_id: "t".into(),
        batch: 10,
        lease: Duration::from_secs(30),
        max_attempts: 3,
        base_backoff: Duration::from_secs(10),
        verify_url_base: "https://app.ryasai.my.id/verify-email".into(),
        policy: AuthPolicy::default(),
    };
    assert_eq!(
        process_once(&pool, &sender, &cfg, &Metrics::new())
            .await
            .unwrap()
            .sent,
        1
    );
    let body = sender.sent.lock().unwrap()[0].text_body.clone();
    let token = body
        .split("#token=")
        .nth(1)
        .unwrap()
        .split_whitespace()
        .next()
        .unwrap()
        .to_owned();

    let ok = send(
        &app,
        Method::POST,
        "/api/v1/auth/verify-email",
        Some(json!({"token": token})),
        &[("origin", APP)],
    )
    .await;
    assert_eq!(
        (ok.status, ok.body["data"]["verified"].as_bool()),
        (200, Some(true)),
        "{}",
        ok.body
    );
    let again = send(
        &app,
        Method::POST,
        "/api/v1/auth/verify-email",
        Some(json!({"token": token})),
        &[],
    )
    .await;
    assert_eq!(
        (
            again.status,
            again.body["error"]["details"]["field"].as_str()
        ),
        (422, Some("token")),
        "sekali pakai"
    );
    let junk = send(
        &app,
        Method::POST,
        "/api/v1/auth/verify-email",
        Some(json!({"token": "x"})),
        &[],
    )
    .await;
    assert_eq!(junk.status, 422);
    let verified: bool = sqlx::query_scalar(
        "SELECT email_verified_at IS NOT NULL FROM users WHERE lower(email) = 'budi@x.id'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert!(verified);
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn login_attempts_are_audited_with_request_id_and_hashed_email(pool: PgPool) {
    let app = app(pool.clone());
    send(
        &app,
        Method::POST,
        "/api/v1/auth/register",
        Some(reg_body("budi@x.id", "Toko Budi")),
        &[],
    )
    .await;
    let bad = send(
        &app,
        Method::POST,
        "/api/v1/auth/login",
        Some(json!({"email": "budi@x.id", "password": "salah-123"})),
        &[],
    )
    .await;
    let good = send(
        &app,
        Method::POST,
        "/api/v1/auth/login",
        Some(json!({"email": "budi@x.id", "password": "rahasia-123"})),
        &[],
    )
    .await;
    assert_eq!((bad.status, good.status), (401, 200));

    let rows: Vec<(String, Option<String>, String)> =
        sqlx::query_as("SELECT action, request_id, summary::text FROM audit_logs WHERE action LIKE 'auth.login.%' ORDER BY created_at")
            .fetch_all(&pool)
            .await
            .unwrap();
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0].0, "auth.login.failed");
    assert_eq!(
        rows[0].1.as_deref(),
        bad.headers["x-request-id"].to_str().ok()
    );
    assert!(
        rows[0].2.contains("email_sha256") && !rows[0].2.contains("budi@x.id"),
        "{}",
        rows[0].2
    );
    assert_eq!(rows[1].0, "auth.login.succeeded");
    assert_eq!(
        rows[1].1.as_deref(),
        good.headers["x-request-id"].to_str().ok()
    );
    let leaked: i64 = sqlx::query_scalar("SELECT count(*) FROM audit_logs WHERE summary::text LIKE '%rahasia%' OR summary::text LIKE '%salah-123%'")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(leaked, 0);
}

/// Masuk sebagai `email` (sudah terdaftar) → (cookie, csrf).
async fn login_as(app: &Router, email: &str, password: &str, ua: &str) -> (String, String) {
    let r = send(
        app,
        Method::POST,
        "/api/v1/auth/login",
        Some(json!({"email": email, "password": password})),
        &[("origin", APP), ("user-agent", ua)],
    )
    .await;
    assert_eq!(r.status, 200, "{}", r.body);
    (
        cookie_pair(&r),
        r.body["data"]["csrf_token"].as_str().unwrap().to_owned(),
    )
}

/// UAT 15 Nov F7 / AUTH-006: daftar sesi aktif, cabut satu & semua sesi lain, isolasi antar akun.
#[sqlx::test(migrator = "MIGRATOR")]
async fn sessions_can_be_listed_and_revoked_per_device(pool: PgPool) {
    let app = app(pool);
    let r = send(
        &app,
        Method::POST,
        "/api/v1/auth/register",
        Some(reg_body("sesi@x.id", "Toko Sesi")),
        &[("origin", APP)],
    )
    .await;
    assert_eq!(r.status, 201);
    let (c1, k1) = login_as(&app, "sesi@x.id", "rahasia-123", "Laptop").await;
    let (c2, _) = login_as(&app, "sesi@x.id", "rahasia-123", "Ponsel").await;
    let (c3, _) = login_as(&app, "sesi@x.id", "rahasia-123", "Tablet").await;

    let list = send(
        &app,
        Method::GET,
        "/api/v1/auth/sessions",
        None,
        &[("cookie", &c1)],
    )
    .await;
    assert_eq!(list.status, 200);
    let items = list.body["data"].as_array().unwrap().clone();
    assert_eq!(items.len(), 4, "register + 3 login: {}", list.body);
    assert_eq!(items.iter().filter(|s| s["current"] == true).count(), 1);
    let current = items.iter().find(|s| s["current"] == true).unwrap();
    assert_eq!(current["user_agent"], "Laptop");
    assert!(
        !list.body.to_string().contains("token"),
        "hash token tidak bocor"
    );
    let phone = items.iter().find(|s| s["user_agent"] == "Ponsel").unwrap()["id"]
        .as_str()
        .unwrap()
        .to_owned();
    let me = current["id"].as_str().unwrap().to_owned();

    // Tanpa CSRF → 403; sesi sendiri → 409.
    let uri = format!("/api/v1/auth/sessions/{phone}/revoke");
    assert_eq!(
        send(
            &app,
            Method::POST,
            &uri,
            None,
            &[("cookie", &c1), ("origin", APP)]
        )
        .await
        .status,
        403
    );
    let own = format!("/api/v1/auth/sessions/{me}/revoke");
    assert_eq!(
        send(
            &app,
            Method::POST,
            &own,
            None,
            &[("cookie", &c1), ("origin", APP), ("x-csrf-token", &k1)]
        )
        .await
        .status,
        409
    );

    // Cabut ponsel → sesi ponsel mati, yang lain hidup; cabut lagi → 404.
    assert_eq!(
        send(
            &app,
            Method::POST,
            &uri,
            None,
            &[("cookie", &c1), ("origin", APP), ("x-csrf-token", &k1)]
        )
        .await
        .status,
        200
    );
    assert_eq!(
        send(
            &app,
            Method::GET,
            "/api/v1/auth/session",
            None,
            &[("cookie", &c2)]
        )
        .await
        .status,
        401
    );
    assert_eq!(
        send(
            &app,
            Method::GET,
            "/api/v1/auth/session",
            None,
            &[("cookie", &c3)]
        )
        .await
        .status,
        200
    );
    assert_eq!(
        send(
            &app,
            Method::POST,
            &uri,
            None,
            &[("cookie", &c1), ("origin", APP), ("x-csrf-token", &k1)]
        )
        .await
        .status,
        404
    );

    // Akun lain tidak bisa mencabut sesi akun ini (404 seragam).
    let r = send(
        &app,
        Method::POST,
        "/api/v1/auth/register",
        Some(reg_body("lain@x.id", "Toko Lain")),
        &[("origin", APP)],
    )
    .await;
    let (co, ko) = (
        cookie_pair(&r),
        r.body["data"]["csrf_token"].as_str().unwrap().to_owned(),
    );
    let tablet = items.iter().find(|s| s["user_agent"] == "Tablet").unwrap()["id"]
        .as_str()
        .unwrap()
        .to_owned();
    let t_uri = format!("/api/v1/auth/sessions/{tablet}/revoke");
    assert_eq!(
        send(
            &app,
            Method::POST,
            &t_uri,
            None,
            &[("cookie", &co), ("origin", APP), ("x-csrf-token", &ko)]
        )
        .await
        .status,
        404
    );
    assert_eq!(
        send(
            &app,
            Method::GET,
            "/api/v1/auth/session",
            None,
            &[("cookie", &c3)]
        )
        .await
        .status,
        200
    );

    // Keluar dari semua perangkat lain: tablet + sesi register dicabut, sesi ini tetap.
    let o = send(
        &app,
        Method::POST,
        "/api/v1/auth/sessions/revoke-others",
        None,
        &[("cookie", &c1), ("origin", APP), ("x-csrf-token", &k1)],
    )
    .await;
    assert_eq!(o.status, 200);
    assert_eq!(o.body["data"]["revoked"], 2);
    assert_eq!(
        send(
            &app,
            Method::GET,
            "/api/v1/auth/session",
            None,
            &[("cookie", &c3)]
        )
        .await
        .status,
        401
    );
    let list = send(
        &app,
        Method::GET,
        "/api/v1/auth/sessions",
        None,
        &[("cookie", &c1)],
    )
    .await;
    assert_eq!(list.body["data"].as_array().unwrap().len(), 1);
}

/// Ubah kata sandi: wajib kata sandi lama, kebijakan kata sandi, sesi lain dicabut.
#[sqlx::test(migrator = "MIGRATOR")]
async fn change_password_requires_current_and_revokes_other_sessions(pool: PgPool) {
    let app = app(pool);
    let r = send(
        &app,
        Method::POST,
        "/api/v1/auth/register",
        Some(reg_body("pw@x.id", "Toko Pw")),
        &[("origin", APP)],
    )
    .await;
    assert_eq!(r.status, 201);
    let (c1, k1) = login_as(&app, "pw@x.id", "rahasia-123", "Laptop").await;
    let (c2, _) = login_as(&app, "pw@x.id", "rahasia-123", "Ponsel").await;
    let h = [
        ("cookie", c1.as_str()),
        ("origin", APP),
        ("x-csrf-token", k1.as_str()),
    ];
    let post = |b: Value| send(&app, Method::POST, "/api/v1/auth/password", Some(b), &h);

    let wrong =
        post(json!({"current_password": "salah-sekali", "new_password": "baru-rahasia-456"})).await;
    assert_eq!(wrong.status, 422);
    assert_eq!(wrong.body["error"]["details"]["field"], "current_password");
    let weak = post(json!({"current_password": "rahasia-123", "new_password": "pendek"})).await;
    assert_eq!(
        (weak.status, weak.body["error"]["details"]["field"].as_str()),
        (422, Some("new_password"))
    );
    let same =
        post(json!({"current_password": "rahasia-123", "new_password": "rahasia-123"})).await;
    assert_eq!(same.status, 422);
    // Ditolak tidak mengubah apa pun: sesi lain masih hidup.
    assert_eq!(
        send(
            &app,
            Method::GET,
            "/api/v1/auth/session",
            None,
            &[("cookie", &c2)]
        )
        .await
        .status,
        200
    );

    let ok =
        post(json!({"current_password": "rahasia-123", "new_password": "baru-rahasia-456"})).await;
    assert_eq!(ok.status, 200, "{}", ok.body);
    assert_eq!(ok.body["data"]["other_sessions_revoked"], 2);
    assert_eq!(
        send(
            &app,
            Method::GET,
            "/api/v1/auth/session",
            None,
            &[("cookie", &c1)]
        )
        .await
        .status,
        200
    );
    assert_eq!(
        send(
            &app,
            Method::GET,
            "/api/v1/auth/session",
            None,
            &[("cookie", &c2)]
        )
        .await
        .status,
        401
    );
    // Kata sandi lama tidak berlaku, yang baru berlaku.
    let old = send(
        &app,
        Method::POST,
        "/api/v1/auth/login",
        Some(json!({"email": "pw@x.id", "password": "rahasia-123"})),
        &[("origin", APP)],
    )
    .await;
    assert_eq!(old.status, 401);
    login_as(&app, "pw@x.id", "baru-rahasia-456", "Laptop").await;
}
