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
