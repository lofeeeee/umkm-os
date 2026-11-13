//! P1-5 HTTP: deny-by-default + object-level pada /api/v1/tenants (Postgres + Redis nyata).

use std::{sync::Arc, time::Duration};

use axum::{
    Router,
    body::Body,
    http::{Method, Request, header},
};
use http_body_util::BodyExt;
use serde_json::{Value, json};
use sqlx::PgPool;
use tower::ServiceExt;
use umkm_api::{
    AppState, HttpLimits,
    auth::{AuthService, csrf_token},
    build_router,
};
use umkm_application::onboarding::LegalVersions;
use umkm_database::MIGRATOR;
use umkm_domain::auth::AuthPolicy;
use umkm_infrastructure::{
    auth::{Argon2idHasher, new_token},
    auth_repo,
    rate_limit::RateLimiter,
    redis::{KeyNamespace, RedisClient},
};
use umkm_shared::config::SecretString;
use uuid::Uuid;

const LIMITS: HttpLimits = HttpLimits {
    request_timeout: Duration::from_secs(10),
    body_limit_bytes: 16 * 1024,
};

fn app(pool: PgPool) -> Router {
    let client = RedisClient::open(&SecretString::new(
        std::env::var("REDIS_URL").expect("REDIS_URL"),
    ))
    .unwrap();
    let svc = AuthService {
        pool,
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
        keyring: None,
    };
    build_router(AppState::default().with_auth(Arc::new(svc)), LIMITS)
}

/// Sesi untuk user tertentu: (cookie header, csrf).
struct Who {
    cookie: String,
    csrf: String,
}

async fn session_for(pool: &PgPool, user: Uuid) -> Who {
    let (token, hash) = new_token().unwrap();
    auth_repo::create_session(pool, user, hash, Duration::from_secs(3600), None)
        .await
        .unwrap();
    Who {
        cookie: format!("__Host-umkm_session={token}"),
        csrf: csrf_token(&token),
    }
}

async fn user_with_role(pool: &PgPool, tenant: Uuid, role: &str) -> Uuid {
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
    u
}

async fn tenant(pool: &PgPool, slug: &str) -> Uuid {
    sqlx::query_scalar("INSERT INTO tenants (name, slug) VALUES ('Toko', $1) RETURNING id")
        .bind(slug)
        .fetch_one(pool)
        .await
        .unwrap()
}

async fn call(
    app: &Router,
    method: Method,
    uri: &str,
    who: Option<&Who>,
    csrf: bool,
    body: Option<Value>,
) -> (u16, Value) {
    let mut b = Request::builder().method(method).uri(uri);
    if let Some(w) = who {
        b = b.header(header::COOKIE, &w.cookie);
        if csrf {
            b = b.header("x-csrf-token", &w.csrf);
        }
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
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    (
        status,
        serde_json::from_slice(&bytes).unwrap_or(Value::Null),
    )
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn owner_reads_and_updates_with_csrf(pool: PgPool) {
    let t = tenant(&pool, "toko-a").await;
    let owner = session_for(&pool, user_with_role(&pool, t, "Owner").await).await;
    let app = app(pool);
    let uri = format!("/api/v1/tenants/{t}");

    let (s, body) = call(&app, Method::GET, &uri, Some(&owner), false, None).await;
    assert_eq!(s, 200, "{body}");
    assert_eq!(body["data"]["my_role"], "Owner");
    assert_eq!(body["data"]["my_permissions"].as_array().unwrap().len(), 37);

    let (s, _) = call(
        &app,
        Method::PATCH,
        &uri,
        Some(&owner),
        false,
        Some(json!({"name": "Baru"})),
    )
    .await;
    assert_eq!(s, 403, "PATCH tanpa CSRF");
    let (s, body) = call(
        &app,
        Method::PATCH,
        &uri,
        Some(&owner),
        true,
        Some(json!({"name": "Toko Baru"})),
    )
    .await;
    assert_eq!((s, body["data"]["name"].as_str()), (200, Some("Toko Baru")));
    let (s, body) = call(
        &app,
        Method::PATCH,
        &uri,
        Some(&owner),
        true,
        Some(json!({"name": "  "})),
    )
    .await;
    assert_eq!(
        (s, body["error"]["details"]["field"].as_str()),
        (422, Some("name"))
    );
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn roles_without_tenant_update_get_403_with_required_permission(pool: PgPool) {
    let t = tenant(&pool, "toko-a").await;
    let mut who = vec![];
    for role in [
        "Manager",
        "Admin",
        "Support Agent",
        "Catalog Manager",
        "Viewer",
    ] {
        who.push((
            role,
            session_for(&pool, user_with_role(&pool, t, role).await).await,
        ));
    }
    let app = app(pool);
    let uri = format!("/api/v1/tenants/{t}");
    for (role, w) in &who {
        let (s, _) = call(&app, Method::GET, &uri, Some(w), false, None).await;
        assert_eq!(s, 200, "{role} punya tenant.read");
        let (s, body) = call(
            &app,
            Method::PATCH,
            &uri,
            Some(w),
            true,
            Some(json!({"name": "Hack"})),
        )
        .await;
        assert_eq!(s, 403, "{role} tidak boleh tenant.update");
        assert_eq!(
            body["error"]["details"]["required_permission"],
            "tenant.update"
        );
    }
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn non_member_and_unknown_tenants_are_404(pool: PgPool) {
    let (a, b) = (tenant(&pool, "toko-a").await, tenant(&pool, "toko-b").await);
    user_with_role(&pool, a, "Owner").await;
    let owner_b = session_for(&pool, user_with_role(&pool, b, "Owner").await).await;
    let app = app(pool);
    // Owner tenant B mencoba tenant A: tidak bocor -> 404, baik GET maupun PATCH
    assert_eq!(
        call(
            &app,
            Method::GET,
            &format!("/api/v1/tenants/{a}"),
            Some(&owner_b),
            false,
            None
        )
        .await
        .0,
        404
    );
    assert_eq!(
        call(
            &app,
            Method::PATCH,
            &format!("/api/v1/tenants/{a}"),
            Some(&owner_b),
            true,
            Some(json!({"name": "X"}))
        )
        .await
        .0,
        404
    );
    assert_eq!(
        call(
            &app,
            Method::GET,
            &format!("/api/v1/tenants/{}", Uuid::new_v4()),
            Some(&owner_b),
            false,
            None
        )
        .await
        .0,
        404
    );
    assert_eq!(
        call(
            &app,
            Method::GET,
            "/api/v1/tenants/not-a-uuid",
            Some(&owner_b),
            false,
            None
        )
        .await
        .0,
        404
    );
    // Tanpa sesi
    assert_eq!(
        call(
            &app,
            Method::GET,
            &format!("/api/v1/tenants/{b}"),
            None,
            false,
            None
        )
        .await
        .0,
        401
    );
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn list_shows_only_own_memberships(pool: PgPool) {
    let (a, b, c) = (
        tenant(&pool, "toko-a").await,
        tenant(&pool, "toko-b").await,
        tenant(&pool, "toko-c").await,
    );
    let u = user_with_role(&pool, a, "Owner").await;
    sqlx::query("INSERT INTO tenant_memberships (tenant_id, user_id, role_id) SELECT $1, $2, id FROM roles WHERE tenant_id IS NULL AND name = 'Viewer'")
        .bind(b)
        .bind(u)
        .execute(&pool)
        .await
        .unwrap();
    user_with_role(&pool, c, "Owner").await;
    let who = session_for(&pool, u).await;
    let app = app(pool);
    let (s, body) = call(
        &app,
        Method::GET,
        "/api/v1/tenants",
        Some(&who),
        false,
        None,
    )
    .await;
    assert_eq!(s, 200);
    let mut got: Vec<(String, String)> = body["data"]
        .as_array()
        .unwrap()
        .iter()
        .map(|m| {
            (
                m["slug"].as_str().unwrap().to_owned(),
                m["role"].as_str().unwrap().to_owned(),
            )
        })
        .collect();
    got.sort();
    assert_eq!(
        got,
        [
            ("toko-a".into(), "Owner".into()),
            ("toko-b".into(), "Viewer".into())
        ]
    );
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn suspending_membership_revokes_access_immediately(pool: PgPool) {
    let t = tenant(&pool, "toko-a").await;
    let u = user_with_role(&pool, t, "Manager").await;
    let who = session_for(&pool, u).await;
    let app = app(pool.clone());
    let uri = format!("/api/v1/tenants/{t}");
    assert_eq!(
        call(&app, Method::GET, &uri, Some(&who), false, None)
            .await
            .0,
        200
    );
    sqlx::query("UPDATE tenant_memberships SET status = 'suspended' WHERE user_id = $1")
        .bind(u)
        .execute(&pool)
        .await
        .unwrap();
    assert_eq!(
        call(&app, Method::GET, &uri, Some(&who), false, None)
            .await
            .0,
        404
    );
}
