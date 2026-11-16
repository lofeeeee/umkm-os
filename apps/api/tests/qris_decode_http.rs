mod common;

use common::*;
use sqlx::PgPool;
use umkm_database::MIGRATOR;

#[sqlx::test(migrator = "MIGRATOR")]
async fn qris_payload_column_exists(pool: PgPool) {
    let exists: bool = sqlx::query_scalar(
        "SELECT EXISTS (
             SELECT 1 FROM information_schema.columns
             WHERE table_schema = 'public'
               AND table_name = 'commerce_settings'
               AND column_name = 'qris_static_payload'
           )",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert!(exists, "migration 0039 belum menambah qris_static_payload");
}

use axum::http::Method;
use serde_json::json;
use umkm_infrastructure::gopay::{GopaySession, poller::now_ms};
use umkm_infrastructure::gopay_repo;
use umkm_shared::ids::TenantId;

async fn connect_gopay(a: &TestApp, t: uuid::Uuid) {
    let session = GopaySession {
        phone_e164: "+6281200000000".into(),
        merchant_id: Some("M1".into()),
        outlet_name: Some("Toko".into()),
        access_token: "access".into(),
        refresh_token: "refresh".into(),
        expires_at_ms: now_ms() + 86_400_000,
    };
    let keyring = a.state.auth.as_ref().unwrap().keyring.as_ref().unwrap();
    gopay_repo::put_session(
        &a.state.db.clone().unwrap(),
        TenantId::from_uuid(t),
        &session,
        keyring,
    )
    .await
    .unwrap();
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn static_qris_endpoint_is_gone(pool: PgPool) {
    let a = app(&pool).await;
    let t = tenant(&pool, "qris-gone").await;
    let o = member(&pool, t, "Owner").await;
    let r = json(
        &a.router,
        Method::PUT,
        &format!("/api/v1/tenants/{t}/gopay/static-qris"),
        Some(&o),
        Some(json!({ "qris": qris_payload() })),
    )
    .await;
    assert_eq!(r.status, 404);
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn gopay_status_uses_decoded_qris_payload(pool: PgPool) {
    let a = app(&pool).await;
    let t = tenant(&pool, "qris-status").await;
    let o = member(&pool, t, "Owner").await;
    let id = upload(&a.router, t, &o, qris_png()).await.json()["data"]["id"]
        .as_str()
        .unwrap()
        .to_owned();
    let url = format!("/api/v1/tenants/{t}/commerce/settings");
    let saved = json(
        &a.router,
        Method::PUT,
        &url,
        Some(&o),
        Some(json!({
            "qris": format!("asset:{id}"), "qris_receiver_name": "Toko", "checkout_enabled": true
        })),
    )
    .await;
    assert_eq!(saved.status, 200, "{}", saved.json());
    connect_gopay(&a, t).await;
    let status = json(
        &a.router,
        Method::GET,
        &format!("/api/v1/tenants/{t}/gopay"),
        Some(&o),
        None,
    )
    .await;
    assert_eq!(status.status, 200, "{}", status.json());
    assert_eq!(status.json()["data"]["connected"], true);
    assert_eq!(status.json()["data"]["has_static_qris"], true);
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn settings_save_decodes_uploaded_qris(pool: PgPool) {
    let a = app(&pool).await;
    let t = tenant(&pool, "qris-valid").await;
    let o = member(&pool, t, "Owner").await;
    let id = upload(&a.router, t, &o, qris_png()).await.json()["data"]["id"]
        .as_str()
        .unwrap()
        .to_owned();
    let url = format!("/api/v1/tenants/{t}/commerce/settings");
    let r = json(
        &a.router,
        Method::PUT,
        &url,
        Some(&o),
        Some(json!({
            "qris": format!("asset:{id}"),
            "qris_receiver_name": "Toko A",
            "checkout_enabled": true,
            "delivery_fee": "0"
        })),
    )
    .await;
    assert_eq!(r.status, 200, "{}", r.json());
    assert_eq!(r.json()["data"]["qris_static_ready"], true);
    let payload: Option<String> = sqlx::query_scalar(
        "SELECT qris_static_payload FROM commerce_settings WHERE tenant_id = $1",
    )
    .bind(t)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(payload.as_deref(), Some(qris_payload().as_str()));
}
