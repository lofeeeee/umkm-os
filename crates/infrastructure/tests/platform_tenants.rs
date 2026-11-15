//! P11: fungsi pengawasan per-toko untuk admin platform (omzet/volume per UMKM, drill-down).
//! Memvalidasi SQL SECURITY DEFINER app.platform_tenants / app.platform_tenant_detail.

use sqlx::PgPool;
use umkm_database::{MIGRATOR, begin_tenant_tx};
use umkm_infrastructure::analytics_repo;
use umkm_shared::ids::TenantId;
use uuid::Uuid;

async fn tenant(pool: &PgPool, slug: &str) -> TenantId {
    let id: Uuid =
        sqlx::query_scalar("INSERT INTO tenants (name, slug) VALUES ($1, $2) RETURNING id")
            .bind(format!("Toko {slug}"))
            .bind(slug)
            .fetch_one(pool)
            .await
            .unwrap();
    TenantId::from_uuid(id)
}

/// Buat pesanan. Order wajib mulai AWAITING_PAYMENT (trigger); bila `paid`, dipindah ke PAID
/// (transisi valid, dan PAID termasuk status omzet).
async fn order(pool: &PgPool, t: TenantId, key: &str, paid: bool, total: i64) {
    let mut tx = begin_tenant_tx(pool, t).await.unwrap();
    let c: Uuid = sqlx::query_scalar(
        "INSERT INTO customers (tenant_id, name, email) VALUES ($1, 'S', 's@x.id') RETURNING id",
    )
    .bind(t.as_uuid())
    .fetch_one(&mut *tx)
    .await
    .unwrap();
    let oid: Uuid = sqlx::query_scalar(
        "INSERT INTO orders (tenant_id, code, customer_id, status, fulfillment, customer_name,
             subtotal, delivery_fee, total, access_token_hash, idempotency_key, request_hash,
             privacy_version, terms_version, consented_at, expires_at)
         VALUES ($1, $2, $3, 'AWAITING_PAYMENT', 'pickup', 'S', $4, 0, $4,
             sha256(gen_random_uuid()::text::bytea), $5, sha256(gen_random_uuid()::text::bytea),
             'v0', 'v0', now(), now() + interval '1 hour') RETURNING id",
    )
    .bind(t.as_uuid())
    .bind(format!("ORD-{}", key.to_uppercase()))
    .bind(c)
    .bind(total)
    .bind(key)
    .fetch_one(&mut *tx)
    .await
    .unwrap();
    if paid {
        sqlx::query("UPDATE orders SET status = 'PAID' WHERE tenant_id = $1 AND id = $2")
            .bind(t.as_uuid())
            .bind(oid)
            .execute(&mut *tx)
            .await
            .unwrap();
    }
    tx.commit().await.unwrap();
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn tenants_list_shows_gmv_and_volume(pool: PgPool) {
    let a = tenant(&pool, "toko-a").await;
    let _b = tenant(&pool, "toko-beta").await;
    // Langganan untuk A.
    // Tenant otomatis punya langganan trialing; jadikan A 'active'.
    sqlx::query(
        "UPDATE subscriptions SET plan_code = 'starter', status = 'active',
             current_period_end = now() + interval '30 days' WHERE tenant_id = $1",
    )
    .bind(a.as_uuid())
    .execute(&pool)
    .await
    .unwrap();
    // A: 1 pesanan selesai (omzet) + 1 menunggu bayar (tidak dihitung omzet).
    order(&pool, a, "aaaa0001", true, 100).await;
    order(&pool, a, "aaaa0002", false, 50).await;

    let (_f, _t, v) = analytics_repo::platform_tenants(&pool, None, None, "", 20, 0)
        .await
        .unwrap();
    assert_eq!(v["total"].as_i64().unwrap(), 2);
    let items = v["items"].as_array().unwrap();
    let row_a = items.iter().find(|i| i["slug"] == "toko-a").unwrap();
    assert_eq!(row_a["orders"].as_i64().unwrap(), 2);
    assert_eq!(row_a["paid_orders"].as_i64().unwrap(), 1);
    assert_eq!(row_a["gmv"].as_str().unwrap(), "100.00");
    assert_eq!(row_a["plan"], "starter");
    assert_eq!(row_a["sub_status"], "active");
    let row_b = items.iter().find(|i| i["slug"] == "toko-beta").unwrap();
    assert_eq!(row_b["gmv"].as_str().unwrap(), "0");
    assert_eq!(row_b["sub_status"], "trialing");

    // Pencarian menyaring.
    let (_f, _t, v2) = analytics_repo::platform_tenants(&pool, None, None, "beta", 20, 0)
        .await
        .unwrap();
    assert_eq!(v2["total"].as_i64().unwrap(), 1);
    assert_eq!(v2["items"][0]["slug"], "toko-beta");
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn tenant_detail_and_missing(pool: PgPool) {
    let a = tenant(&pool, "toko-a").await;
    order(&pool, a, "aaaa0001", true, 250).await;
    order(&pool, a, "aaaa0002", true, 150).await;

    let (_f, _t, v) = analytics_repo::platform_tenant_detail(&pool, *a.as_uuid(), None, None)
        .await
        .unwrap();
    let d = v.expect("toko ada");
    assert_eq!(d["slug"], "toko-a");
    assert_eq!(d["range"]["orders"].as_i64().unwrap(), 2);
    assert_eq!(d["range"]["paid_orders"].as_i64().unwrap(), 2);
    assert_eq!(d["range"]["gmv"].as_str().unwrap(), "400.00");
    assert_eq!(d["range"]["by_status"]["PAID"].as_i64().unwrap(), 2);

    // Toko tidak ada → None.
    let (_f, _t, none) = analytics_repo::platform_tenant_detail(&pool, Uuid::new_v4(), None, None)
        .await
        .unwrap();
    assert!(none.is_none());
}
