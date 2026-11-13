//! P7-5: analitik toko (FR-ANL-001, AC-ANL-001) & platform (FR-ANL-002, AC-ANL-002). Role aplikasi + RLS.

mod common;

use axum::http::Method;
use common::*;
use serde_json::{Value, json};
use sqlx::PgPool;
use umkm_database::{MIGRATOR, begin_tenant_tx};
use umkm_domain::conversation::ConversationStatus;
use umkm_infrastructure::conversation_repo::{self as conv, Actor, StartInput};
use umkm_shared::ids::TenantId;
use uuid::Uuid;

/// Data toko: pesanan (terbayar/batal), item, stok menipis, pelanggan, tiket, percakapan, AI.
/// `scale` membedakan nilai antar tenant.
async fn seed(pool: &PgPool, t: Uuid, scale: i64) {
    let app = app_pool(pool).await;
    let tid = TenantId::from_uuid(t);
    let mut tx = begin_tenant_tx(&app, tid).await.unwrap();
    let customer: Uuid = sqlx::query_scalar(
        "INSERT INTO customers (tenant_id, name, email) VALUES ($1, 'Sari', 's@x.id') RETURNING id",
    )
    .bind(t)
    .fetch_one(&mut *tx)
    .await
    .unwrap();
    sqlx::query("INSERT INTO customers (tenant_id, name, email, created_at) VALUES ($1, 'Lama', 'l@x.id', now() - interval '90 days')")
        .bind(t)
        .execute(&mut *tx)
        .await
        .unwrap();
    // 3 terbayar (COMPLETED, PROCESSING, PAID), 1 batal, 1 kedaluwarsa, 1 menunggu bayar.
    // Dibuat AWAITING_PAYMENT (aturan state machine), status akhir di-set setelah commit (fixture).
    let mut finals: Vec<(Uuid, &str)> = vec![];
    for (status, total, product, qty) in [
        ("COMPLETED", 100_000, "Kopi Gayo", 2),
        ("PROCESSING", 50_000, "Kopi Gayo", 1),
        ("PAID", 25_000, "Teh Melati", 1),
        ("CANCELLED", 999_000, "Kopi Gayo", 50),
        ("EXPIRED", 10_000, "Teh Melati", 1),
        ("AWAITING_PAYMENT", 10_000, "Teh Melati", 1),
    ] {
        let total = total * scale;
        let o: Uuid = sqlx::query_scalar(
            "INSERT INTO orders (tenant_id, code, customer_id, status, fulfillment, customer_name, subtotal, delivery_fee, total,
                 access_token_hash, idempotency_key, request_hash, privacy_version, terms_version, consented_at, expires_at)
             VALUES ($1, 'ORD-' || upper(substr(md5(random()::text), 1, 6)), $2, $3, 'pickup', 'Sari', $4, 0, $4,
                 sha256(gen_random_uuid()::text::bytea), gen_random_uuid()::text, sha256(gen_random_uuid()::text::bytea),
                 'v0', 'v0', now(), now() + interval '1 hour') RETURNING id",
        )
        .bind(t)
        .bind(customer)
        .bind("AWAITING_PAYMENT")
        .bind(total)
        .fetch_one(&mut *tx)
        .await
        .unwrap();
        finals.push((o, status));
        sqlx::query(
            "INSERT INTO order_items (tenant_id, order_id, position, product_name, variant_name, sku, unit_price, quantity, line_total)
             VALUES ($1, $2, 0, $3, 'Default', 'SKU-1', $4, $5, $6)",
        )
        .bind(t)
        .bind(o)
        .bind(product)
        .bind(total / qty)
        .bind(qty as i32)
        .bind(total)
        .execute(&mut *tx)
        .await
        .unwrap();
    }
    // Stok: 1 varian di bawah ambang, 1 aman, 1 tanpa ambang.
    let p: Uuid = sqlx::query_scalar("INSERT INTO products (tenant_id, name, slug, status) VALUES ($1, 'Kopi Gayo', 'kopi-gayo', 'active') RETURNING id")
        .bind(t)
        .fetch_one(&mut *tx)
        .await
        .unwrap();
    for (sku, name, on_hand, min) in [
        ("K-1", "250g", 2, Some(5)),
        ("K-2", "500g", 20, Some(5)),
        ("K-3", "1kg", 0, None),
    ] {
        let v: Uuid = sqlx::query_scalar(
            "INSERT INTO product_variants (tenant_id, product_id, sku, name, price, is_active, position) VALUES ($1, $2, $3, $4, 1000, true, (SELECT count(*) FROM product_variants WHERE product_id = $2)::int) RETURNING id",
        )
        .bind(t)
        .bind(p)
        .bind(sku)
        .bind(name)
        .fetch_one(&mut *tx)
        .await
        .unwrap();
        sqlx::query("UPDATE inventory_levels SET on_hand = $3, min_stock = $4 WHERE tenant_id = $1 AND variant_id = $2")
            .bind(t)
            .bind(v)
            .bind(on_hand)
            .bind(min)
            .execute(&mut *tx)
            .await
            .unwrap();
    }
    // Tiket: 2 dibuat, 1 selesai 2 jam setelah dibuat.
    for (code, resolved) in [("TKT-AAAAAAAAA1", true), ("TKT-AAAAAAAAA2", false)] {
        sqlx::query(
            "INSERT INTO tickets (tenant_id, code, source, subject, created_at, first_response_due_at, resolution_due_at)
             VALUES ($1, $2, 'manual', 'x', now() - interval '2 hours', now() + interval '1 hour', now() + interval '1 day')",
        )
        .bind(t)
        .bind(code.replace('A', if scale == 1 { "A" } else { "B" }))
        .execute(&mut *tx)
        .await
        .unwrap();
        if resolved {
            // OPEN → RESOLVED (transisi sah).
            sqlx::query("UPDATE tickets SET status = 'RESOLVED', resolved_at = now() WHERE tenant_id = $1 AND code = $2")
                .bind(t)
                .bind(code.replace('A', if scale == 1 { "A" } else { "B" }))
                .execute(&mut *tx)
                .await
                .unwrap();
        }
    }
    sqlx::query("INSERT INTO ai_runs (tenant_id, source, model, purpose, status, prompt_tokens, completion_tokens) VALUES ($1, 'platform', 'm', 'chat', 'ok', $2, 10), ($1, 'byok', 'm', 'chat', 'ok', 5, 5)")
        .bind(t)
        .bind((100 * scale) as i32)
        .execute(&mut *tx)
        .await
        .unwrap();
    tx.commit().await.unwrap();
    // Fixture: lompat ke status akhir tanpa trigger transisi (hanya untuk menyiapkan data uji).
    let mut otx = pool.begin().await.unwrap();
    sqlx::query("SET LOCAL session_replication_role = replica")
        .execute(&mut *otx)
        .await
        .unwrap();
    for (id, s) in finals {
        sqlx::query("UPDATE orders SET status = $2 WHERE id = $1")
            .bind(id)
            .bind(s)
            .execute(&mut *otx)
            .await
            .unwrap();
    }
    otx.commit().await.unwrap();
    // Percakapan: 2 baru, 1 dialihkan → ditutup staf.
    let si = || StartInput {
        consent_version: "v0",
        customer_id: None,
        initial: ConversationStatus::Ai,
    };
    let c1 = conv::start(&app, tid, &si()).await.unwrap().conversation_id;
    conv::start(&app, tid, &si()).await.unwrap();
    conv::transition(&app, tid, c1, ConversationStatus::WaitingHuman, Actor::Ai)
        .await
        .unwrap();
    conv::transition(&app, tid, c1, ConversationStatus::Closed, Actor::System)
        .await
        .unwrap();
}

async fn get(a: &TestApp, w: &Who, uri: &str) -> Res {
    json(&a.router, Method::GET, uri, Some(w), None).await
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn tenant_overview_nine_metrics_scoped(pool: PgPool) {
    let a = app(&pool).await;
    let ta = tenant(&pool, "toko-a").await;
    let tb = tenant(&pool, "toko-b").await;
    let owner = member(&pool, ta, "Owner").await;
    let viewer = member(&pool, ta, "Viewer").await;
    let agent = member(&pool, ta, "Support Agent").await;
    let owner_b = member(&pool, tb, "Owner").await;
    seed(&pool, ta, 1).await;
    seed(&pool, tb, 10).await;

    let r = get(
        &a,
        &owner,
        &format!("/api/v1/tenants/{ta}/analytics/overview"),
    )
    .await;
    assert_eq!(r.status, 200, "{}", String::from_utf8_lossy(&r.bytes));
    let d = r.json()["data"].clone();
    // 1-2. Omzet hanya pesanan terbayar; rata-rata nilai pesanan.
    assert_eq!(
        (
            d["revenue"].clone(),
            d["orders"].clone(),
            d["paid_orders"].clone()
        ),
        (json!("175000.00"), json!(6), json!(3))
    );
    assert_eq!(d["average_order_value"], "58333.33");
    // 3. Terlaris dari pesanan terbayar (pesanan batal 50 pcs tidak dihitung).
    assert_eq!(
        d["top_products"][0],
        json!({"name": "Kopi Gayo", "quantity": 3, "revenue": "150000.00"})
    );
    assert_eq!(d["top_products"][1]["name"], "Teh Melati");
    // 4. Stok menipis.
    assert_eq!(d["stock_alerts"], 1);
    assert_eq!(
        d["stock_alert_items"][0],
        json!({"product": "Kopi Gayo", "variant": "250g", "available": 2, "min_stock": 5})
    );
    // 5. Pelanggan.
    assert_eq!(
        (d["customers_total"].clone(), d["customers_new"].clone()),
        (json!(2), json!(1))
    );
    // 6. Pembatalan: (batal + kedaluwarsa) / 6.
    assert_eq!(d["cancellation_rate"], 0.3333);
    // 7. Tiket.
    assert_eq!(
        (d["tickets_created"].clone(), d["tickets_resolved"].clone()),
        (json!(2), json!(1))
    );
    let res = d["avg_ticket_resolution_ms"].as_i64().unwrap();
    assert!((7_190_000..=7_210_000).contains(&res), "{res}");
    // 8. AI resolution/handover.
    assert_eq!(
        (
            d["conversations"].clone(),
            d["human_resolved"].clone(),
            d["ai_resolved"].clone(),
            d["handover_rate"].clone()
        ),
        (json!(2), json!(0), json!(1), json!(0.5))
    );
    // 9. Pemakaian AI.
    assert_eq!(
        (
            d["ai_platform_tokens"].clone(),
            d["ai_byok_tokens"].clone(),
            d["ai_turns"].clone()
        ),
        (json!(110), json!(10), json!(2))
    );

    // AC-ANL-001: tenant B (nilai ×10) hanya datanya sendiri.
    let b = get(
        &a,
        &owner_b,
        &format!("/api/v1/tenants/{tb}/analytics/overview"),
    )
    .await
    .json()["data"]
        .clone();
    assert_eq!(
        (b["revenue"].clone(), b["ai_platform_tokens"].clone()),
        (json!("1750000.00"), json!(1010))
    );
    assert_eq!(
        get(
            &a,
            &owner_b,
            &format!("/api/v1/tenants/{ta}/analytics/overview")
        )
        .await
        .status,
        404
    );
    // Izin & rentang.
    assert_eq!(
        get(
            &a,
            &viewer,
            &format!("/api/v1/tenants/{ta}/analytics/overview")
        )
        .await
        .status,
        200
    );
    assert_eq!(
        get(
            &a,
            &agent,
            &format!("/api/v1/tenants/{ta}/analytics/overview")
        )
        .await
        .status,
        403
    );
    assert_eq!(
        get(
            &a,
            &owner,
            &format!("/api/v1/tenants/{ta}/analytics/overview?from=2026-02-30")
        )
        .await
        .status,
        422
    );
    let empty = get(
        &a,
        &owner,
        &format!("/api/v1/tenants/{ta}/analytics/overview?from=2025-01-01&to=2025-01-31"),
    )
    .await
    .json()["data"]
        .clone();
    assert_eq!(
        (
            empty["revenue"].clone(),
            empty["average_order_value"].clone(),
            empty["cancellation_rate"].clone()
        ),
        (json!("0.00"), Value::Null, Value::Null)
    );
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn platform_analytics_only_for_platform_admin(pool: PgPool) {
    let a = app(&pool).await;
    let ta = tenant(&pool, "toko-a").await;
    let tb = tenant(&pool, "toko-b").await;
    pro(&pool, tb).await;
    sqlx::query("UPDATE tenants SET status = 'suspended' WHERE id = $1")
        .bind(tb)
        .execute(&pool)
        .await
        .unwrap();
    seed(&pool, ta, 1).await;
    let mut tx = begin_tenant_tx(&app_pool(&pool).await, TenantId::from_uuid(ta))
        .await
        .unwrap();
    sqlx::query("INSERT INTO ai_runs (tenant_id, source, model, purpose, status, error_code) VALUES ($1, 'platform', 'm', 'chat', 'timeout', 'timeout'), ($1, 'platform', '-', 'chat', 'ok', NULL)")
        .bind(ta)
        .execute(&mut *tx)
        .await
        .unwrap();
    tx.commit().await.unwrap();

    // AC-ANL-002: anggota toko — termasuk Owner dengan sesi MFA — ditolak.
    let owner = member(&pool, ta, "Owner").await;
    sqlx::query("UPDATE user_sessions SET mfa_verified_at = now() WHERE user_id = $1")
        .bind(owner.user)
        .execute(&pool)
        .await
        .unwrap();
    let r = get(&a, &owner, "/api/v1/platform/analytics").await;
    assert_eq!(r.status, 403);
    assert_ne!(r.json()["error"]["details"]["reason"], "mfa_required");
    let no_mfa = platform_admin(&pool, false).await;
    assert_eq!(
        get(&a, &no_mfa, "/api/v1/platform/analytics").await.status,
        403
    );

    let admin = platform_admin(&pool, true).await;
    let r = get(&a, &admin, "/api/v1/platform/analytics").await;
    assert_eq!(r.status, 200, "{}", String::from_utf8_lossy(&r.bytes));
    let d = r.json()["data"].clone();
    assert_eq!(
        (
            d["tenants"]["active"].clone(),
            d["tenants"]["suspended"].clone(),
            d["tenants"]["new_in_range"].clone()
        ),
        (json!(1), json!(1), json!(2))
    );
    let subs = d["subscriptions"].as_array().unwrap();
    assert!(
        subs.contains(&json!({"plan": "starter", "status": "trialing", "count": 1})),
        "{subs:?}"
    );
    assert!(
        subs.contains(&json!({"plan": "pro", "status": "active", "count": 1})),
        "{subs:?}"
    );
    assert_eq!(d["ai"]["platform_tokens"], 110);
    assert_eq!(d["ai"]["byok_tokens"], 10);
    // Error provider: 1 timeout dari 3 panggilan provider (giliran '-' tidak dihitung).
    assert_eq!(
        (
            d["ai"]["provider_calls"].clone(),
            d["ai"]["provider_errors"].clone(),
            d["provider_error_rate"].clone()
        ),
        (json!(3), json!(1), json!(0.3333))
    );
    assert_eq!(d["ai"]["error_codes"]["timeout"], 1);
    assert!(d["api_usage"]["requests_total"].as_u64().is_some());
    assert!(d["health"]["checks"].is_array());
    // Tidak memuat data pribadi/nama toko — hanya angka agregat.
    let body = d.to_string();
    for s in ["Sari", "s@x.id", "toko-a", "Kopi Gayo"] {
        assert!(!body.contains(s), "agregat platform memuat {s}");
    }
}
