//! Harness HTTP Phase 2: router lengkap di atas pool ROLE APLIKASI (RLS aktif), Redis nyata.
//! Butuh `eval "$(scripts/dev-db.sh up)"` (DATABASE_URL + REDIS_URL).
#![allow(dead_code)]

use std::{sync::Arc, time::Duration};

use axum::{
    Router,
    body::Body,
    http::{Method, Request, header},
};
use http_body_util::BodyExt;
use serde_json::Value;
use sqlx::{PgPool, postgres::PgPoolOptions};
use tower::ServiceExt;
use umkm_api::{
    AppState, HttpLimits,
    auth::{AuthService, csrf_token},
    build_router,
    state::{AiService, FileService},
};
use umkm_application::onboarding::LegalVersions;
use umkm_domain::auth::AuthPolicy;
use umkm_infrastructure::{
    auth::{Argon2idHasher, new_token},
    auth_repo,
    rate_limit::RateLimiter,
    redis::{KeyNamespace, RedisClient},
    storage::{PgObjectStorage, StorageLimits, UrlSigner},
};
use umkm_shared::config::SecretString;
use uuid::Uuid;

pub const MAX_OBJECT: usize = 64 * 1024;
pub const MAX_OBJECTS: i64 = 5;
pub const SIGNING_KEY: &str = "test-signing-key-0123456789-abcdefghijklmnop";
/// KEK uji envelope encryption (32 byte).
pub const TEST_KEK: &str = "v1:a2VrLXJhaGFzaWEta2VrLXJhaGFzaWEta2VrLXJhaGE=";

pub const LIMITS: HttpLimits = HttpLimits {
    request_timeout: Duration::from_secs(30),
    body_limit_bytes: 2 * MAX_OBJECT,
};

pub async fn app_pool(pool: &PgPool) -> PgPool {
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

pub struct TestApp {
    pub router: Router,
    pub state: AppState,
    /// Pool owner (superuser) untuk setup/inspeksi; JANGAN dipakai untuk membuktikan isolasi.
    pub owner: PgPool,
    pub signer: UrlSigner,
}

pub async fn app(pool: &PgPool) -> TestApp {
    let app_db = app_pool(pool).await;
    let client = RedisClient::open(&SecretString::new(
        std::env::var("REDIS_URL").expect("REDIS_URL"),
    ))
    .unwrap();
    let svc = AuthService {
        pool: app_db.clone(),
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
        allowed_origins: vec!["https://app.ryasai.my.id".into()],
        trust_forwarded_for: false,
        keyring: Some(umkm_infrastructure::crypto::Keyring::parse(TEST_KEK).unwrap()),
    };
    let signer = UrlSigner::new(SecretString::new(SIGNING_KEY), 3600);
    let app_db_ai = app_db.clone();
    let state = AppState::default()
        .with_auth(Arc::new(svc))
        .with_db(app_db.clone())
        .with_files(Arc::new(FileService {
            store: PgObjectStorage::new(
                app_db,
                StorageLimits {
                    max_object_bytes: MAX_OBJECT,
                    max_objects_per_tenant: MAX_OBJECTS,
                },
            ),
            signer: signer.clone(),
        }))
        .with_ai(Arc::new(AiService::new(
            app_db_ai,
            umkm_infrastructure::crypto::Keyring::parse(TEST_KEK).unwrap(),
            Some(umkm_shared::config::PlatformAiSettings {
                enabled: true,
                base_url: "https://llm.platform.test/v1".into(),
                models: vec!["platform-small".into(), "platform-large".into()],
                embedding_model: "platform-embed".into(),
                ..Default::default()
            }),
            None,
            4,
        )));
    TestApp {
        router: build_router(state.clone(), LIMITS),
        state,
        owner: pool.clone(),
        signer,
    }
}

/// Sesi untuk user tertentu.
pub struct Who {
    pub user: Uuid,
    pub cookie: String,
    pub csrf: String,
}

pub async fn tenant(pool: &PgPool, slug: &str) -> Uuid {
    let id: Uuid =
        sqlx::query_scalar("INSERT INTO tenants (name, slug) VALUES ('Toko', $1) RETURNING id")
            .bind(slug)
            .fetch_one(pool)
            .await
            .unwrap();
    sqlx::query("INSERT INTO business_profiles (tenant_id, name) VALUES ($1, 'Toko')")
        .bind(id)
        .execute(pool)
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO tenant_domains (tenant_id, hostname, type, status)
         VALUES ($1, $2 || '.ryasai.my.id', 'subdomain', 'active')",
    )
    .bind(id)
    .bind(slug)
    .execute(pool)
    .await
    .unwrap();
    id
}

pub async fn member(pool: &PgPool, tenant: Uuid, role: &str) -> Who {
    let u: Uuid = sqlx::query_scalar(
        "INSERT INTO users (email, password_hash) VALUES ($1, '$argon2id$x') RETURNING id",
    )
    .bind(format!("{}@x.id", Uuid::new_v4().simple()))
    .fetch_one(pool)
    .await
    .unwrap();
    sqlx::query("INSERT INTO tenant_memberships (tenant_id, user_id, role_id) SELECT $1, $2, id FROM roles WHERE tenant_id IS NULL AND name = $3")
        .bind(tenant)
        .bind(u)
        .bind(role)
        .execute(pool)
        .await
        .unwrap();
    let (token, hash) = new_token().unwrap();
    auth_repo::create_session(pool, u, hash, Duration::from_secs(3600), None)
        .await
        .unwrap();
    Who {
        user: u,
        cookie: format!("__Host-umkm_session={token}"),
        csrf: csrf_token(&token),
    }
}

pub struct Res {
    pub status: u16,
    pub headers: axum::http::HeaderMap,
    pub bytes: Vec<u8>,
}

impl Res {
    pub fn json(&self) -> Value {
        serde_json::from_slice(&self.bytes).unwrap_or(Value::Null)
    }
}

pub async fn send(
    app: &Router,
    method: Method,
    uri: &str,
    who: Option<&Who>,
    host: Option<&str>,
    content_type: Option<&str>,
    body: Vec<u8>,
) -> Res {
    let mut b = Request::builder().method(method).uri(uri);
    if let Some(w) = who {
        b = b
            .header(header::COOKIE, &w.cookie)
            .header("x-csrf-token", &w.csrf);
    }
    if let Some(h) = host {
        b = b.header(header::HOST, h);
    }
    if let Some(ct) = content_type {
        b = b.header(header::CONTENT_TYPE, ct);
    }
    let res = app
        .clone()
        .oneshot(b.body(Body::from(body)).unwrap())
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

pub async fn json(
    app: &Router,
    method: Method,
    uri: &str,
    who: Option<&Who>,
    body: Option<Value>,
) -> Res {
    let (ct, b) = match body {
        Some(v) => (Some("application/json"), v.to_string().into_bytes()),
        None => (None, vec![]),
    };
    send(app, method, uri, who, None, ct, b).await
}

// ------------------------------------------------------------------ fixture gambar

pub fn png() -> Vec<u8> {
    let mut v = b"\x89PNG\r\n\x1a\n".to_vec();
    let mut chunk = |kind: &[u8], data: &[u8]| {
        v.extend((data.len() as u32).to_be_bytes());
        v.extend(kind);
        v.extend(data);
        v.extend([0, 0, 0, 0]);
    };
    chunk(b"IHDR", &[0, 0, 0, 1, 0, 0, 0, 1, 8, 6, 0, 0, 0]);
    chunk(b"IDAT", &[0x78, 0x9c, 0x63, 0, 1, 0, 0, 5, 0, 1]);
    chunk(b"IEND", &[]);
    v
}

pub fn jpeg() -> Vec<u8> {
    vec![
        0xFF, 0xD8, 0xFF, 0xE0, 0, 16, b'J', b'F', b'I', b'F', 0, 1, 0xFF, 0xD9,
    ]
}

pub async fn upload(app: &Router, tenant: Uuid, who: &Who, bytes: Vec<u8>) -> Res {
    send(
        app,
        Method::POST,
        &format!("/api/v1/tenants/{tenant}/uploads"),
        Some(who),
        None,
        Some("image/png"),
        bytes,
    )
    .await
}

/// Langganan Pro aktif 30 hari (P7, ADR-0016) — untuk uji yang memerlukan kuota AI platform / banyak anggota.
pub async fn pro(pool: &PgPool, tenant: Uuid) {
    sqlx::query(
        "UPDATE subscriptions SET plan_code = 'pro', status = 'active', current_period_end = now() + interval '30 days'
          WHERE tenant_id = $1",
    )
    .bind(tenant)
    .execute(pool)
    .await
    .unwrap();
}

/// Super admin platform; `mfa` = sesi sudah lolos langkah kedua (AUTH-011).
pub async fn platform_admin(pool: &PgPool, mfa: bool) -> Who {
    let u: Uuid = sqlx::query_scalar(
        "INSERT INTO users (email, password_hash, platform_role) VALUES ($1, '$argon2id$x', 'super_admin') RETURNING id",
    )
    .bind(format!("admin-{}@x.id", Uuid::new_v4().simple()))
    .fetch_one(pool)
    .await
    .unwrap();
    let (token, hash) = new_token().unwrap();
    auth_repo::create_session(pool, u, hash, Duration::from_secs(3600), None)
        .await
        .unwrap();
    if mfa {
        sqlx::query("UPDATE user_sessions SET mfa_verified_at = now() WHERE user_id = $1")
            .bind(u)
            .execute(pool)
            .await
            .unwrap();
    }
    Who {
        user: u,
        cookie: format!("__Host-umkm_session={token}"),
        csrf: csrf_token(&token),
    }
}
