//! P3-4 / P3-6: skema commerce — paritas state machine Rust ↔ trigger DB, snapshot & event
//! append-only, invariant total, status awal. Dijalankan sebagai role aplikasi (RLS aktif).

use sqlx::{PgPool, Row};
use umkm_database::{MIGRATOR, begin_tenant_tx};
use umkm_domain::commerce::state::{ALL_STATUSES, Fulfillment};
use umkm_shared::ids::TenantId;
use uuid::Uuid;

async fn app_pool(pool: &PgPool) -> PgPool {
    sqlx::postgres::PgPoolOptions::new()
        .max_connections(4)
        .after_connect(|conn, _| {
            Box::pin(async move {
                sqlx::query("SET ROLE umkm_app_role").execute(conn).await?;
                Ok(())
            })
        })
        .connect_with(pool.connect_options().as_ref().clone())
        .await
        .unwrap()
}

async fn tenant(pool: &PgPool) -> TenantId {
    let id: Uuid =
        sqlx::query_scalar("INSERT INTO tenants (name, slug) VALUES ('T', 'toko-a') RETURNING id")
            .fetch_one(pool)
            .await
            .unwrap();
    TenantId::from_uuid(id)
}

/// Order minimal AWAITING_PAYMENT; kembalikan id.
async fn order(app: &PgPool, t: TenantId, fulfillment: &str, key: &str) -> Uuid {
    let mut tx = begin_tenant_tx(app, t).await.unwrap();
    let c: Uuid = sqlx::query_scalar(
        "INSERT INTO customers (tenant_id, name, email) VALUES ($1, 'S', 's@x.id') RETURNING id",
    )
    .bind(t.as_uuid())
    .fetch_one(&mut *tx)
    .await
    .unwrap();
    let addr = (fulfillment == "delivery").then_some("Jl. Mawar 1");
    let id: Uuid = sqlx::query_scalar(
        "INSERT INTO orders (tenant_id, code, customer_id, status, fulfillment, customer_name, delivery_address,
             subtotal, delivery_fee, total, access_token_hash, idempotency_key, request_hash,
             privacy_version, terms_version, consented_at, expires_at)
         VALUES ($1, $2, $3, 'AWAITING_PAYMENT', $4, 'S', $5, 100, 0, 100, sha256(gen_random_uuid()::text::bytea), $6,
             sha256(gen_random_uuid()::text::bytea), 'v0', 'v0', now(), now() + interval '1 hour') RETURNING id",
    )
    .bind(t.as_uuid())
    .bind(format!("ORD-{}", key[..6].to_uppercase()))
    .bind(c)
    .bind(fulfillment)
    .bind(addr)
    .bind(key)
    .fetch_one(&mut *tx)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO order_items (tenant_id, order_id, position, product_name, variant_name, sku, unit_price, quantity, line_total)
         VALUES ($1, $2, 0, 'Kopi', 'Default', 'K-1', 50, 2, 100)",
    )
    .bind(t.as_uuid())
    .bind(id)
    .execute(&mut *tx)
    .await
    .unwrap();
    sqlx::query("INSERT INTO order_events (tenant_id, order_id, to_status, actor_type) VALUES ($1, $2, 'AWAITING_PAYMENT', 'customer')")
        .bind(t.as_uuid())
        .bind(id)
        .execute(&mut *tx)
        .await
        .unwrap();
    tx.commit().await.unwrap();
    id
}

fn code(e: &sqlx::Error) -> String {
    e.as_database_error()
        .and_then(|d| d.code())
        .map(|c| c.into_owned())
        .unwrap_or_default()
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn db_transition_table_matches_domain(pool: PgPool) {
    // AC-ORD-004: 13 × 13 × 2 kombinasi identik.
    let mut checked = 0;
    for a in ALL_STATUSES {
        for b in ALL_STATUSES {
            for f in [Fulfillment::Pickup, Fulfillment::Delivery] {
                let db: bool =
                    sqlx::query_scalar("SELECT app.order_transition_allowed($1, $2, $3)")
                        .bind(a.as_str())
                        .bind(b.as_str())
                        .bind(f.as_str())
                        .fetch_one(&pool)
                        .await
                        .unwrap();
                assert_eq!(db, a.can_transition(b, f), "{a:?} -> {b:?} ({f:?})");
                checked += 1;
            }
        }
    }
    assert_eq!(checked, 338);
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn trigger_rejects_invalid_transitions_and_mutations(pool: PgPool) {
    let app = app_pool(&pool).await;
    let t = tenant(&pool).await;
    let o = order(&app, t, "pickup", "key-00000001").await;
    let set = |status: &'static str| {
        let app = app.clone();
        async move {
            let mut tx = begin_tenant_tx(&app, t).await.unwrap();
            let r = sqlx::query("UPDATE orders SET status = $2 WHERE tenant_id = $1 AND id = $3")
                .bind(t.as_uuid())
                .bind(status)
                .bind(o)
                .execute(&mut *tx)
                .await;
            if r.is_ok() {
                tx.commit().await.unwrap();
            }
            r.map(|_| ())
        }
    };
    assert_eq!(
        code(&set("PROCESSING").await.unwrap_err()),
        "23514",
        "melompati pembayaran"
    );
    set("PAID").await.unwrap();
    assert_eq!(
        code(&set("CANCELLED").await.unwrap_err()),
        "23514",
        "dibayar tidak bisa dibatalkan"
    );
    set("PROCESSING").await.unwrap();
    assert_eq!(
        code(&set("SHIPPED").await.unwrap_err()),
        "23514",
        "pickup tidak dikirim"
    );
    set("READY_FOR_PICKUP").await.unwrap();
    set("COMPLETED").await.unwrap();
    assert_eq!(
        code(&set("PENDING").await.unwrap_err()),
        "23514",
        "COMPLETED -> PENDING"
    );

    let mut tx = begin_tenant_tx(&app, t).await.unwrap();
    let e = sqlx::query("UPDATE orders SET total = 1, subtotal = 1 WHERE id = $1")
        .bind(o)
        .execute(&mut *tx)
        .await
        .unwrap_err();
    assert_eq!(code(&e), "23514", "total tidak dapat diubah");
    drop(tx);

    // snapshot item, event: append-only; order tidak dapat dihapus
    for sql in [
        "UPDATE order_items SET unit_price = 1 WHERE order_id = $1",
        "DELETE FROM order_items WHERE order_id = $1",
        "UPDATE order_events SET to_status = 'X' WHERE order_id = $1",
        "DELETE FROM order_events WHERE order_id = $1",
        "DELETE FROM orders WHERE id = $1",
    ] {
        let mut tx = begin_tenant_tx(&app, t).await.unwrap();
        let e = sqlx::query(sqlx::AssertSqlSafe(sql))
            .bind(o)
            .execute(&mut *tx)
            .await
            .unwrap_err();
        assert_eq!(code(&e), "42501", "{sql}");
    }
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn invariants_on_insert(pool: PgPool) {
    let app = app_pool(&pool).await;
    let t = tenant(&pool).await;
    let o = order(&app, t, "delivery", "key-00000002").await;
    let mut tx = begin_tenant_tx(&app, t).await.unwrap();
    // status awal selain PENDING/AWAITING_PAYMENT ditolak
    let e = sqlx::query(
        "INSERT INTO orders (tenant_id, code, customer_id, status, fulfillment, customer_name, subtotal, delivery_fee, total,
             access_token_hash, idempotency_key, request_hash, privacy_version, terms_version, consented_at, expires_at)
         SELECT tenant_id, 'ORD-PAIDX', customer_id, 'PAID', 'pickup', 'S', 1, 0, 1, sha256(gen_random_uuid()::text::bytea), 'key-00000003',
             sha256(gen_random_uuid()::text::bytea), 'v0', 'v0', now(), now() FROM orders WHERE id = $1",
    )
    .bind(o)
    .execute(&mut *tx)
    .await
    .unwrap_err();
    assert_eq!(code(&e), "23514");
    drop(tx);
    // line_total harus = unit_price × quantity; total = subtotal + ongkir
    let mut tx = begin_tenant_tx(&app, t).await.unwrap();
    let e = sqlx::query(
        "INSERT INTO order_items (tenant_id, order_id, position, product_name, variant_name, sku, unit_price, quantity, line_total)
         VALUES ($1, $2, 1, 'X', 'Default', 'X', 50, 2, 1)",
    )
    .bind(t.as_uuid())
    .bind(o)
    .execute(&mut *tx)
    .await
    .unwrap_err();
    assert_eq!(code(&e), "23514");
    drop(tx);
    // payment: confirmed wajib amount_received + verifier; rejected wajib alasan
    let mut tx = begin_tenant_tx(&app, t).await.unwrap();
    sqlx::query("INSERT INTO payments (tenant_id, order_id, amount) VALUES ($1, $2, 100)")
        .bind(t.as_uuid())
        .bind(o)
        .execute(&mut *tx)
        .await
        .unwrap();
    let e = sqlx::query("UPDATE payments SET status = 'confirmed' WHERE order_id = $1")
        .bind(o)
        .execute(&mut *tx)
        .await
        .unwrap_err();
    assert_eq!(code(&e), "23514");
    drop(tx);
    // checkout tidak dapat diaktifkan tanpa QRIS
    let e = sqlx::query(
        "INSERT INTO commerce_settings (tenant_id, checkout_enabled) VALUES ($1, true)",
    )
    .bind(t.as_uuid())
    .execute(&pool)
    .await
    .unwrap_err();
    assert_eq!(code(&e), "23514");
    let n: i64 = sqlx::query("SELECT count(*) AS n FROM order_items WHERE order_id = $1")
        .bind(o)
        .fetch_one(&pool)
        .await
        .unwrap()
        .get("n");
    assert_eq!(n, 1);
}
