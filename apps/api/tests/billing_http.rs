//! P7-1: konfigurasi paket & langganan (FR-SUB-001, AC-SUB-001, ADR-0016). Role aplikasi + RLS.

mod common;

use axum::http::Method;
use common::*;
use serde_json::{Value, json};
use sqlx::PgPool;
use umkm_database::{MIGRATOR, begin_tenant_tx};
use umkm_shared::ids::TenantId;

async fn sub(a: &TestApp, w: &Who, t: uuid::Uuid) -> Res {
    json(
        &a.router,
        Method::GET,
        &format!("/api/v1/tenants/{t}/subscription"),
        Some(w),
        None,
    )
    .await
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn plans_are_data_and_new_tenants_start_trial(pool: PgPool) {
    let a = app(&pool).await;
    // Katalog publik: Starter Rp50.000 (uji coba 14 hari, tanpa token AI platform, BYOK boleh), Pro Rp100.000.
    let r = send(
        &a.router,
        Method::GET,
        "/api/v1/plans",
        None,
        None,
        None,
        vec![],
    )
    .await;
    assert_eq!(r.status, 200);
    let plans = r.json()["data"].clone();
    let codes: Vec<&str> = plans
        .as_array()
        .unwrap()
        .iter()
        .map(|p| p["code"].as_str().unwrap())
        .collect();
    assert_eq!(codes, ["starter", "pro"], "tidak ada paket gratis");
    assert_eq!(
        (
            plans[0]["price_idr"].clone(),
            plans[0]["trial_days"].clone()
        ),
        (json!(50000), json!(14))
    );
    assert_eq!(plans[0]["limits"]["platform_ai_tokens_monthly"], 0);
    assert_eq!(plans[0]["limits"]["byok_allowed"], true);
    assert_eq!(
        (
            plans[1]["price_idr"].clone(),
            plans[1]["trial_days"].clone()
        ),
        (json!(100000), Value::Null)
    );
    assert_eq!(
        plans[1]["limits"]["conversations_monthly"],
        Value::Null,
        "Pro: percakapan tak terbatas"
    );
    for p in plans.as_array().unwrap() {
        assert_eq!(p["limits"]["custom_domain"], false, "ADR-0008");
    }

    // Toko baru → uji coba Starter 14 hari (trigger, semua jalur pembuatan tenant).
    let t = tenant(&pool, "toko-a").await;
    let owner = member(&pool, t, "Owner").await;
    let r = sub(&a, &owner, t).await;
    assert_eq!(r.status, 200, "{}", String::from_utf8_lossy(&r.bytes));
    let s = r.json()["data"].clone();
    assert_eq!(
        (s["plan"]["code"].clone(), s["status"].clone()),
        (json!("starter"), json!("trialing"))
    );
    let days: f64 = sqlx::query_scalar("SELECT (extract(epoch FROM trial_ends_at - now()) / 86400)::float8 FROM subscriptions WHERE tenant_id = $1")
        .bind(t)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert!((13.99..=14.0).contains(&days), "{days}");

    // AC-SUB-001: ubah limits_json di data → langsung terlihat, tanpa deploy/restart.
    sqlx::query("UPDATE plans SET limits_json = jsonb_set(limits_json, '{max_products}', '75') WHERE code = 'starter'")
        .execute(&pool)
        .await
        .unwrap();
    assert_eq!(
        sub(&a, &owner, t).await.json()["data"]["plan"]["limits"]["max_products"],
        75
    );

    // Uji coba lewat → status efektif expired walau baris belum diperbarui job.
    sqlx::query(
        "UPDATE subscriptions SET trial_ends_at = now() - interval '1 minute' WHERE tenant_id = $1",
    )
    .bind(t)
    .execute(&pool)
    .await
    .unwrap();
    assert_eq!(sub(&a, &owner, t).await.json()["data"]["status"], "expired");

    // Paket rusak (kunci hilang) → fail-closed (error), bukan dianggap tak terbatas.
    sqlx::query(
        "UPDATE plans SET limits_json = limits_json - 'max_members' WHERE code = 'starter'",
    )
    .execute(&pool)
    .await
    .unwrap();
    assert_eq!(sub(&a, &owner, t).await.status, 500);
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn tenants_cannot_change_plans_or_subscriptions(pool: PgPool) {
    let a = app(&pool).await;
    let ta = tenant(&pool, "toko-a").await;
    let tb = tenant(&pool, "toko-b").await;
    let ob = member(&pool, tb, "Owner").await;
    assert_eq!(sub(&a, &ob, ta).await.status, 404, "toko lain");
    assert_eq!(sub(&a, &ob, tb).await.status, 200);

    let app_db = app_pool(&pool).await;
    for sql in [
        "UPDATE plans SET price_idr = 0",
        "INSERT INTO plans (code, name, price_idr, limits_json) VALUES ('gratis', 'Gratis', 0, '{}')",
        "DELETE FROM plans",
        "UPDATE subscriptions SET status = 'active', current_period_end = now() + interval '10 years'",
        "INSERT INTO subscriptions (tenant_id, plan_code, status, current_period_end) VALUES (gen_random_uuid(), 'pro', 'active', now())",
        "DELETE FROM subscriptions",
    ] {
        let mut tx = begin_tenant_tx(&app_db, TenantId::from_uuid(ta))
            .await
            .unwrap();
        let e = sqlx::query(sql).execute(&mut *tx).await.unwrap_err();
        assert_eq!(
            e.as_database_error().and_then(|d| d.code()).as_deref(),
            Some("42501"),
            "{sql}"
        );
        tx.rollback().await.unwrap();
    }
    // Langganan tenant lain tidak terlihat dari konteks A.
    let mut tx = begin_tenant_tx(&app_db, TenantId::from_uuid(ta))
        .await
        .unwrap();
    let n: i64 = sqlx::query_scalar("SELECT count(*) FROM subscriptions")
        .fetch_one(&mut *tx)
        .await
        .unwrap();
    assert_eq!(n, 1);
    tx.rollback().await.unwrap();
}
