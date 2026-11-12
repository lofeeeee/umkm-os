//! P1-10 (PRD.md §13.2) jalur HTTP + SSE, dengan pool ROLE APLIKASI (RLS aktif):
//! #1 baca milik sendiri, #2 tidak baca B, #3 tidak ubah B, #7 SSE tidak lintas tenant.

use std::{sync::Arc, time::Duration};

use axum::{
    Router,
    body::Body,
    http::{Method, Request, header},
};
use http_body_util::BodyExt;
use serde_json::{Value, json};
use sqlx::{PgPool, postgres::PgPoolOptions};
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
use uuid::Uuid;

const LIMITS: HttpLimits = HttpLimits {
    request_timeout: Duration::from_secs(30),
    body_limit_bytes: 16 * 1024,
};

async fn app_pool(pool: &PgPool) -> PgPool {
    PgPoolOptions::new()
        .max_connections(5)
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

async fn app(pool: &PgPool) -> Router {
    app_with_hub(pool).await.0
}

async fn app_with_hub(pool: &PgPool) -> (Router, umkm_api::realtime::RealtimeHub) {
    let client = RedisClient::open(&SecretString::new(
        std::env::var("REDIS_URL").expect("REDIS_URL"),
    ))
    .unwrap();
    let svc = AuthService {
        pool: app_pool(pool).await,
        limiter: RateLimiter::new(
            client.connection_manager_lazy().unwrap(),
            KeyNamespace::new(&format!("umkmtest:{}", Uuid::new_v4().simple())).unwrap(),
        ),
        hasher: Arc::new(Argon2idHasher::new(1024, 1, 1).unwrap()),
        policy: AuthPolicy::default(),
        legal: LegalVersions {
            privacy_policy: "v0".into(),
            terms_of_service: "v0".into(),
        },
        root_domain: "ryasai.my.id".into(),
        secure_cookies: true,
        allowed_origins: vec![],
        trust_forwarded_for: false,
    };
    let state = AppState::default().with_auth(Arc::new(svc));
    let hub = state.realtime.clone();
    (build_router(state, LIMITS), hub)
}

struct Owner {
    cookie: String,
    csrf: String,
    tenant: String,
}

async fn register(app: &Router, email: &str, name: &str) -> Owner {
    let body = json!({"email": email, "password": "rahasia-123", "business_name": name, "accept_terms": true, "accept_privacy": true});
    let res = app
        .clone()
        .oneshot(
            Request::post("/api/v1/auth/register")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(body.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), 201);
    let cookie = res.headers()[header::SET_COOKIE]
        .to_str()
        .unwrap()
        .split(';')
        .next()
        .unwrap()
        .to_owned();
    let v: Value =
        serde_json::from_slice(&res.into_body().collect().await.unwrap().to_bytes()).unwrap();
    Owner {
        cookie,
        csrf: v["data"]["csrf_token"].as_str().unwrap().into(),
        tenant: v["data"]["tenant"]["id"].as_str().unwrap().into(),
    }
}

async fn call(
    app: &Router,
    method: Method,
    uri: &str,
    who: &Owner,
    body: Option<Value>,
) -> (u16, Value) {
    let b = Request::builder()
        .method(method)
        .uri(uri)
        .header(header::COOKIE, &who.cookie)
        .header("x-csrf-token", &who.csrf);
    let req = match body {
        Some(v) => b
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(v.to_string())),
        None => b.body(Body::empty()),
    }
    .unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    let s = res.status().as_u16();
    (
        s,
        serde_json::from_slice(&res.into_body().collect().await.unwrap().to_bytes())
            .unwrap_or(Value::Null),
    )
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn t1_t2_t3_read_own_but_not_read_or_modify_other_tenant(pool: PgPool) {
    let app = app(&pool).await;
    let a = register(&app, "a@x.id", "Toko A").await;
    let b = register(&app, "b@x.id", "Toko B").await;

    // #1 Tenant A membaca resource miliknya
    let (s, v) = call(
        &app,
        Method::GET,
        &format!("/api/v1/tenants/{}", a.tenant),
        &a,
        None,
    )
    .await;
    assert_eq!((s, v["data"]["name"].as_str()), (200, Some("Toko A")));
    // #2 Tenant A tidak dapat membaca Tenant B
    assert_eq!(
        call(
            &app,
            Method::GET,
            &format!("/api/v1/tenants/{}", b.tenant),
            &a,
            None
        )
        .await
        .0,
        404
    );
    // #3 Tenant A tidak dapat mengubah Tenant B
    assert_eq!(
        call(
            &app,
            Method::PATCH,
            &format!("/api/v1/tenants/{}", b.tenant),
            &a,
            Some(json!({"name": "Diretas"}))
        )
        .await
        .0,
        404
    );
    let (_, v) = call(
        &app,
        Method::GET,
        &format!("/api/v1/tenants/{}", b.tenant),
        &b,
        None,
    )
    .await;
    assert_eq!(v["data"]["name"], "Toko B", "data B utuh");
    // Daftar tenant A tidak memuat B
    let (_, v) = call(&app, Method::GET, "/api/v1/tenants", &a, None).await;
    assert_eq!(v["data"].as_array().unwrap().len(), 1);
}

/// Baca frame SSE berikutnya; `None` bila timeout, `Some(None)` bila stream selesai.
async fn next_frame(body: &mut Body, wait: Duration) -> Option<Option<String>> {
    match tokio::time::timeout(wait, body.frame()).await {
        Err(_) => None,
        Ok(None) => Some(None),
        Ok(Some(f)) => Some(
            f.unwrap()
                .into_data()
                .ok()
                .map(|d| String::from_utf8_lossy(&d).into_owned()),
        ),
    }
}

async fn subscribe(app: &Router, who: &Owner, tenant: &str) -> (u16, Body) {
    let res = app
        .clone()
        .oneshot(
            Request::get(format!("/api/v1/tenants/{tenant}/events"))
                .header(header::COOKIE, &who.cookie)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    (res.status().as_u16(), res.into_body())
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn t7_sse_never_delivers_events_across_tenants(pool: PgPool) {
    let app = app(&pool).await;
    let a = register(&app, "a@x.id", "Toko A").await;
    let b = register(&app, "b@x.id", "Toko B").await;

    // B tidak dapat berlangganan stream A
    assert_eq!(subscribe(&app, &b, &a.tenant).await.0, 404);

    let (sa, mut stream_a) = subscribe(&app, &a, &a.tenant).await;
    let (sb, mut stream_b) = subscribe(&app, &b, &b.tenant).await;
    assert_eq!((sa, sb), (200, 200));

    assert_eq!(
        call(
            &app,
            Method::PATCH,
            &format!("/api/v1/tenants/{}", a.tenant),
            &a,
            Some(json!({"name": "Toko A Baru"}))
        )
        .await
        .0,
        200
    );

    let got = next_frame(&mut stream_a, Duration::from_secs(5))
        .await
        .expect("A harus menerima event")
        .expect("stream A hidup");
    assert!(
        got.contains("event: tenant.updated") && got.contains("Toko A Baru"),
        "{got}"
    );
    assert!(
        next_frame(&mut stream_b, Duration::from_millis(800))
            .await
            .is_none(),
        "B tidak boleh menerima event A"
    );

    // Event milik B hanya sampai ke B
    assert_eq!(
        call(
            &app,
            Method::PATCH,
            &format!("/api/v1/tenants/{}", b.tenant),
            &b,
            Some(json!({"name": "Toko B Baru"}))
        )
        .await
        .0,
        200
    );
    let got_b = next_frame(&mut stream_b, Duration::from_secs(5))
        .await
        .unwrap()
        .unwrap();
    assert!(got_b.contains("Toko B Baru"));
    assert!(
        next_frame(&mut stream_a, Duration::from_millis(800))
            .await
            .is_none(),
        "A tidak boleh menerima event B"
    );
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn t7_sse_stream_closes_when_membership_revoked(pool: PgPool) {
    let (app, hub) = app_with_hub(&pool).await;
    let a = register(&app, "a@x.id", "Toko A").await;
    let (status, mut stream) = subscribe(&app, &a, &a.tenant).await;
    assert_eq!(status, 200);
    assert_eq!(hub.open_connections(), 1);
    let tenant = umkm_shared::ids::TenantId::from_uuid(a.tenant.parse().unwrap());

    // Masih anggota: event diterima
    hub.publish(umkm_api::realtime::TenantEvent {
        tenant_id: tenant,
        kind: "tenant.updated".into(),
        data: json!({"n": 1}),
        user_id: None,
        permission: None,
    });
    assert!(
        next_frame(&mut stream, Duration::from_secs(5))
            .await
            .unwrap()
            .unwrap()
            .contains("\"n\":1")
    );

    // Membership dicabut -> event berikut TIDAK dikirim dan stream ditutup
    sqlx::query("UPDATE tenant_memberships SET status = 'suspended'")
        .execute(&pool)
        .await
        .unwrap();
    hub.publish(umkm_api::realtime::TenantEvent {
        tenant_id: tenant,
        kind: "tenant.updated".into(),
        data: json!({"n": 2}),
        user_id: None,
        permission: None,
    });
    assert_eq!(
        next_frame(&mut stream, Duration::from_secs(5)).await,
        Some(None),
        "stream harus berakhir tanpa data"
    );
    drop(stream);
    assert_eq!(
        hub.open_connections(),
        0,
        "hitungan koneksi turun saat stream selesai"
    );
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn t7_sse_connection_limit_enforced(pool: PgPool) {
    // Router dengan hub berbatas 2 koneksi (PRD.md §7.5: jumlah koneksi dibatasi)
    let (base, _) = app_with_hub(&pool).await;
    let a = register(&base, "a@x.id", "Toko A").await;
    let client =
        RedisClient::open(&SecretString::new(std::env::var("REDIS_URL").unwrap())).unwrap();
    let svc = AuthService {
        pool: app_pool(&pool).await,
        limiter: RateLimiter::new(
            client.connection_manager_lazy().unwrap(),
            KeyNamespace::new("umkmtest:limit").unwrap(),
        ),
        hasher: Arc::new(Argon2idHasher::new(1024, 1, 1).unwrap()),
        policy: AuthPolicy::default(),
        legal: LegalVersions {
            privacy_policy: "v0".into(),
            terms_of_service: "v0".into(),
        },
        root_domain: "ryasai.my.id".into(),
        secure_cookies: true,
        allowed_origins: vec![],
        trust_forwarded_for: false,
    };
    let mut state = AppState::default().with_auth(Arc::new(svc));
    state.realtime = umkm_api::realtime::RealtimeHub::new(16, 2);
    let hub = state.realtime.clone();
    let app = build_router(state, LIMITS);

    let (s1, b1) = subscribe(&app, &a, &a.tenant).await;
    let (s2, _b2) = subscribe(&app, &a, &a.tenant).await;
    let (s3, _) = subscribe(&app, &a, &a.tenant).await;
    assert_eq!((s1, s2, s3), (200, 200, 503));
    assert_eq!(
        hub.open_connections(),
        2,
        "penolakan tidak menambah hitungan"
    );
    drop(b1);
    assert_eq!(
        subscribe(&app, &a, &a.tenant).await.0,
        200,
        "slot kembali setelah koneksi ditutup"
    );
}
