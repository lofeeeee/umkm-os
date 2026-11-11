//! P1-4: resolver tenant dari `Host` (PRD.md §4.2, TEN-009) di Postgres nyata.

use std::time::Duration;

use axum::{Router, body::Body, extract::Query, http::Request, routing::get};
use http_body_util::BodyExt;
use serde_json::Value;
use sqlx::PgPool;
use tower::ServiceExt;
use umkm_api::{AppState, HttpLimits, middleware::HostTenant, with_middleware};
use umkm_database::MIGRATOR;
use uuid::Uuid;

const LIMITS: HttpLimits = HttpLimits {
    request_timeout: Duration::from_secs(5),
    body_limit_bytes: 1024,
};

/// Route uji: mengembalikan tenant hasil resolusi. Query `tenant_id` sengaja diterima
/// dan DIABAIKAN untuk membuktikan tidak dipakai sebagai identitas.
async fn probe(
    HostTenant(t): HostTenant,
    Query(_q): Query<std::collections::HashMap<String, String>>,
) -> String {
    t.to_string()
}

fn app(pool: PgPool) -> Router {
    with_middleware(
        Router::new().route("/probe", get(probe)),
        LIMITS,
        Default::default(),
    )
    .with_state(AppState::default().with_db(pool))
}

async fn tenant(pool: &PgPool, slug: &str, host: &str, ty: &str, domain_status: &str) -> Uuid {
    let id: Uuid =
        sqlx::query_scalar("INSERT INTO tenants (name, slug) VALUES ('T', $1) RETURNING id")
            .bind(slug)
            .fetch_one(pool)
            .await
            .unwrap();
    sqlx::query(
        "INSERT INTO tenant_domains (tenant_id, hostname, type, status, verified_at)
         VALUES ($1, $2, $3, $4, CASE WHEN $4 = 'active' THEN now() END)",
    )
    .bind(id)
    .bind(host)
    .bind(ty)
    .bind(domain_status)
    .execute(pool)
    .await
    .unwrap();
    id
}

async fn fetch(app: &Router, uri: &str, headers: &[(&str, &str)]) -> (u16, String) {
    let mut b = Request::get(uri);
    for (k, v) in headers {
        b = b.header(*k, *v);
    }
    let res = app
        .clone()
        .oneshot(b.body(Body::empty()).unwrap())
        .await
        .unwrap();
    let status = res.status().as_u16();
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    (status, String::from_utf8_lossy(&bytes).into_owned())
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn active_host_resolves_to_its_tenant_with_normalization(pool: PgPool) {
    let a = tenant(
        &pool,
        "toko-a",
        "toko-a.ryasai.my.id",
        "subdomain",
        "active",
    )
    .await;
    let b = tenant(
        &pool,
        "toko-b",
        "toko-b.ryasai.my.id",
        "subdomain",
        "active",
    )
    .await;
    let app = app(pool);
    for host in [
        "toko-a.ryasai.my.id",
        "TOKO-A.ryasai.my.id",
        "toko-a.ryasai.my.id:443",
        "toko-a.ryasai.my.id.",
    ] {
        assert_eq!(
            fetch(&app, "/probe", &[("host", host)]).await,
            (200, a.to_string()),
            "{host}"
        );
    }
    assert_eq!(
        fetch(&app, "/probe", &[("host", "toko-b.ryasai.my.id")]).await,
        (200, b.to_string())
    );
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn spoofing_attempts_do_not_change_tenant(pool: PgPool) {
    let a = tenant(
        &pool,
        "toko-a",
        "toko-a.ryasai.my.id",
        "subdomain",
        "active",
    )
    .await;
    let b = tenant(
        &pool,
        "toko-b",
        "toko-b.ryasai.my.id",
        "subdomain",
        "active",
    )
    .await;
    let app = app(pool);
    // tenant_id di query diabaikan
    assert_eq!(
        fetch(
            &app,
            &format!("/probe?tenant_id={b}"),
            &[("host", "toko-a.ryasai.my.id")]
        )
        .await,
        (200, a.to_string())
    );
    // X-Forwarded-Host tidak dipercaya: Host tak dikenal tetap 404 walau XFH valid
    assert_eq!(
        fetch(
            &app,
            "/probe",
            &[
                ("host", "evil.example"),
                ("x-forwarded-host", "toko-b.ryasai.my.id")
            ]
        )
        .await
        .0,
        404
    );
    // ...dan tidak menimpa Host yang valid
    assert_eq!(
        fetch(
            &app,
            "/probe",
            &[
                ("host", "toko-a.ryasai.my.id"),
                ("x-forwarded-host", "toko-b.ryasai.my.id")
            ]
        )
        .await,
        (200, a.to_string())
    );
    // Sufiks/prefiks mirip tidak cocok
    for host in [
        "toko-a.ryasai.my.id.evil.example",
        "xtoko-a.ryasai.my.id",
        "ryasai.my.id",
        "api.ryasai.my.id",
        "127.0.0.1",
        "[::1]",
    ] {
        assert_eq!(
            fetch(&app, "/probe", &[("host", host)]).await.0,
            404,
            "{host}"
        );
    }
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn non_active_domains_and_tenants_are_not_served(pool: PgPool) {
    tenant(
        &pool,
        "toko-s",
        "toko-s.ryasai.my.id",
        "subdomain",
        "suspended",
    )
    .await;
    tenant(
        &pool,
        "toko-d",
        "toko-d.ryasai.my.id",
        "subdomain",
        "disabled",
    )
    .await;
    tenant(
        &pool,
        "toko-p",
        "tokop.com",
        "custom",
        "pending_verification",
    )
    .await;
    let t = tenant(
        &pool,
        "toko-t",
        "toko-t.ryasai.my.id",
        "subdomain",
        "active",
    )
    .await;
    let c = tenant(
        &pool,
        "toko-c",
        "toko-c.ryasai.my.id",
        "subdomain",
        "active",
    )
    .await;
    sqlx::query("UPDATE tenants SET status = 'suspended' WHERE id = $1")
        .bind(t)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("UPDATE tenants SET status = 'closed', closed_at = now() WHERE id = $1")
        .bind(c)
        .execute(&pool)
        .await
        .unwrap();
    let app = app(pool);
    for host in [
        "toko-s.ryasai.my.id",
        "toko-d.ryasai.my.id",
        "tokop.com",
        "toko-t.ryasai.my.id",
        "toko-c.ryasai.my.id",
    ] {
        let (status, body) = fetch(&app, "/probe", &[("host", host)]).await;
        assert_eq!(status, 404, "{host} tidak boleh dilayani");
        let v: Value = serde_json::from_str(&body).unwrap();
        assert_eq!(
            v["error"]["code"], "RESOURCE_NOT_FOUND",
            "alasan tidak dibedakan"
        );
    }
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn domain_suspension_takes_effect_immediately(pool: PgPool) {
    let a = tenant(
        &pool,
        "toko-a",
        "toko-a.ryasai.my.id",
        "subdomain",
        "active",
    )
    .await;
    let app = app(pool.clone());
    assert_eq!(
        fetch(&app, "/probe", &[("host", "toko-a.ryasai.my.id")]).await,
        (200, a.to_string())
    );
    sqlx::query("UPDATE tenant_domains SET status = 'suspended' WHERE tenant_id = $1")
        .bind(a)
        .execute(&pool)
        .await
        .unwrap();
    assert_eq!(
        fetch(&app, "/probe", &[("host", "toko-a.ryasai.my.id")])
            .await
            .0,
        404,
        "tanpa cache basi"
    );
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn resolver_function_exposes_only_ids_and_statuses(pool: PgPool) {
    let cols: Vec<String> = sqlx::query_scalar(
        "SELECT unnest(proargnames) FROM pg_proc WHERE proname = 'resolve_host'",
    )
    .fetch_all(&pool)
    .await
    .unwrap();
    assert_eq!(
        cols,
        ["p_hostname", "tenant_id", "domain_status", "tenant_status"]
    );
    let secdef: bool =
        sqlx::query_scalar("SELECT prosecdef FROM pg_proc WHERE proname = 'resolve_host'")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert!(secdef);
    let public_exec: bool = sqlx::query_scalar(
        "SELECT has_function_privilege('public', 'app.resolve_host(text)', 'EXECUTE')",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert!(!public_exec, "PUBLIC tidak boleh EXECUTE");
}
