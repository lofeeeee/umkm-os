//! P6-6: analitik percakapan (FR-CNV-004, AC-CNV-004). Agregat diisi trigger, bertahan setelah purge
//! percakapan (ADR-0012), hanya data tenant sendiri. Role aplikasi + RLS.

mod common;

use std::time::Duration;

use axum::http::Method;
use common::*;
use serde_json::{Value, json};
use sqlx::PgPool;
use umkm_database::{MIGRATOR, begin_tenant_tx};
use umkm_domain::conversation::ConversationStatus;
use umkm_infrastructure::conversation_repo::{self as repo, Actor};
use umkm_shared::ids::TenantId;
use uuid::Uuid;

/// Percakapan dibuat `age_secs` lalu; pesan (pengirim, detik lalu, run).
async fn conversation(
    app: &PgPool,
    t: Uuid,
    age_secs: i64,
    msgs: &[(&str, i64, Option<Uuid>)],
) -> Uuid {
    let mut tx = begin_tenant_tx(app, TenantId::from_uuid(t)).await.unwrap();
    let c: Uuid = sqlx::query_scalar(
        "INSERT INTO conversations (tenant_id, consent_version, consented_at, created_at)
         VALUES ($1, 'v0', now(), now() - make_interval(secs => $2)) RETURNING id",
    )
    .bind(t)
    .bind(age_secs as f64)
    .fetch_one(&mut *tx)
    .await
    .unwrap();
    for (sender, ago, run) in msgs {
        sqlx::query(
            "INSERT INTO conversation_messages (tenant_id, conversation_id, sender_type, sender_user_id, body, ai_run_id, created_at)
             VALUES ($1, $2, $3, CASE WHEN $3 = 'ADMIN' THEN (SELECT user_id FROM tenant_memberships WHERE tenant_id = $1 LIMIT 1) END,
                     'x', $4, now() - make_interval(secs => $5))",
        )
        .bind(t)
        .bind(c)
        .bind(*sender)
        .bind(*run)
        .bind(*ago as f64)
        .execute(&mut *tx)
        .await
        .unwrap();
    }
    tx.commit().await.unwrap();
    c
}

#[allow(clippy::too_many_arguments)]
async fn run(
    app: &PgPool,
    t: Uuid,
    conv: Option<Uuid>,
    model: &str,
    status: &str,
    decision: &str,
    pt: i32,
    ct: i32,
) -> Uuid {
    let mut tx = begin_tenant_tx(app, TenantId::from_uuid(t)).await.unwrap();
    let id = sqlx::query_scalar(
        "INSERT INTO ai_runs (tenant_id, conversation_id, source, model, purpose, status, decision, prompt_tokens, completion_tokens)
         VALUES ($1, $2, 'platform', $3, 'chat', $4, $5, $6, $7) RETURNING id",
    )
    .bind(t)
    .bind(conv)
    .bind(model)
    .bind(status)
    .bind(decision)
    .bind(pt)
    .bind(ct)
    .fetch_one(&mut *tx)
    .await
    .unwrap();
    tx.commit().await.unwrap();
    id
}

async fn get(a: &TestApp, who: &Who, t: Uuid, q: &str) -> Res {
    json(
        &a.router,
        Method::GET,
        &format!("/api/v1/tenants/{t}/analytics/conversations{q}"),
        Some(who),
        None,
    )
    .await
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn eight_metrics_tenant_scoped_and_survive_purge(pool: PgPool) {
    let a = app(&pool).await;
    let app_db = app_pool(&pool).await;
    let ta = tenant(&pool, "toko-a").await;
    let tb = tenant(&pool, "toko-b").await;
    let owner = member(&pool, ta, "Owner").await;
    let staff = member(&pool, ta, "Support Agent").await;
    let viewer = member(&pool, ta, "Viewer").await;
    let owner_b = member(&pool, tb, "Owner").await;
    let tid = TenantId::from_uuid(ta);

    // c1: AI menjawab 3 dtk setelah pesan pelanggan; ditutup → AI-resolved (resolusi ±1 jam).
    let r1 = run(&app_db, ta, None, "m1", "ok", "send_auto", 100, 20).await;
    let c1 = conversation(
        &app_db,
        ta,
        3600,
        &[
            ("CUSTOMER", 10, None),
            ("AI", 7, Some(r1)),
            ("CUSTOMER", 5, None),
        ],
    )
    .await;
    repo::transition(
        &app_db,
        tid,
        c1,
        ConversationStatus::Closed,
        Actor::Customer,
    )
    .await
    .unwrap();
    // c2: handover AI → staf, staf membalas 5 dtk setelah pelanggan; ditutup staf → human-resolved.
    let c2 = conversation(&app_db, ta, 600, &[("CUSTOMER", 20, None)]).await;
    repo::transition(
        &app_db,
        tid,
        c2,
        ConversationStatus::WaitingHuman,
        Actor::Ai,
    )
    .await
    .unwrap();
    repo::transition(
        &app_db,
        tid,
        c2,
        ConversationStatus::Human,
        Actor::Staff(staff.user),
    )
    .await
    .unwrap();
    {
        let mut tx = begin_tenant_tx(&app_db, tid).await.unwrap();
        sqlx::query("INSERT INTO conversation_messages (tenant_id, conversation_id, sender_type, sender_user_id, body, created_at) VALUES ($1, $2, 'ADMIN', $3, 'halo', (SELECT first_customer_at FROM conversations WHERE id = $2) + interval '5 seconds')")
            .bind(ta).bind(c2).bind(staff.user).execute(&mut *tx).await.unwrap();
        tx.commit().await.unwrap();
    }
    // Handover kedua pada percakapan yang sama tidak dihitung ulang.
    repo::transition(
        &app_db,
        tid,
        c2,
        ConversationStatus::Ai,
        Actor::Staff(staff.user),
    )
    .await
    .unwrap();
    repo::transition(
        &app_db,
        tid,
        c2,
        ConversationStatus::WaitingHuman,
        Actor::Ai,
    )
    .await
    .unwrap();
    repo::transition(
        &app_db,
        tid,
        c2,
        ConversationStatus::Closed,
        Actor::Staff(staff.user),
    )
    .await
    .unwrap();
    // c3: terbuka; provider gagal → fallback; giliran Human Only tanpa provider.
    let c3 = conversation(&app_db, ta, 60, &[("CUSTOMER", 30, None)]).await;
    run(&app_db, ta, Some(c3), "m1", "timeout", "handover", 0, 0).await;
    run(&app_db, ta, Some(c3), "-", "ok", "human_queue", 0, 0).await;
    // Uji coba admin (purpose test) tidak dihitung.
    sqlx::query("INSERT INTO ai_runs (tenant_id, source, model, purpose, status, prompt_tokens) VALUES ($1, 'platform', 'm1', 'test', 'error', 999)")
        .bind(ta).execute(&pool).await.unwrap();
    // Tenant B: banyak data yang tidak boleh terhitung di A.
    for _ in 0..4 {
        let cb = conversation(&app_db, tb, 100, &[("CUSTOMER", 50, None)]).await;
        run(&app_db, tb, Some(cb), "m1", "error", "handover", 5000, 5000).await;
    }

    let r = get(&a, &owner, ta, "").await;
    assert_eq!(r.status, 200, "{}", String::from_utf8_lossy(&r.bytes));
    let d = r.json()["data"].clone();
    assert_eq!(d["total_conversations"], 3);
    assert_eq!(
        (d["ai_resolved"].clone(), d["human_resolved"].clone()),
        (json!(1), json!(1))
    );
    // Respons pertama: c1 3 dtk, c2 5 dtk → rata-rata 4 dtk (toleransi pembulatan).
    let fr = d["avg_first_response_ms"].as_i64().unwrap();
    assert!((3900..=4100).contains(&fr), "avg_first_response_ms {fr}");
    // Resolusi: (≈3600 dtk + ≈600 dtk)/2 ≈ 2100 dtk.
    let res = d["avg_resolution_ms"].as_i64().unwrap();
    assert!(
        (2_095_000..=2_110_000).contains(&res),
        "avg_resolution_ms {res}"
    );
    assert_eq!(d["handover_rate"], 0.3333);
    assert_eq!(
        (d["ai_turns"].clone(), d["fallback_rate"].clone()),
        (json!(3), json!(0.3333))
    );
    assert_eq!(
        (
            d["provider_calls"].clone(),
            d["provider_error_rate"].clone()
        ),
        (json!(2), json!(0.5))
    );
    assert_eq!(
        d["tokens"],
        json!({"prompt": 100, "completion": 20, "total": 120})
    );
    let days = d["daily"].as_array().unwrap();
    assert_eq!(days.len(), 30, "default 30 hari");
    // Jumlah seri harian = total (tidak bergantung jam: c1 bisa jatuh kemarin dekat tengah malam WIB).
    let sum = |k: &str| days.iter().map(|x| x[k].as_i64().unwrap()).sum::<i64>();
    assert_eq!(
        (
            sum("started"),
            sum("handed_over"),
            sum("ai_turns"),
            sum("tokens")
        ),
        (3, 1, 3, 120)
    );

    // AC-CNV-004: tenant B hanya datanya sendiri.
    let b = get(&a, &owner_b, tb, "").await.json()["data"].clone();
    assert_eq!(
        (
            b["total_conversations"].clone(),
            b["ai_turns"].clone(),
            b["provider_error_rate"].clone()
        ),
        (json!(4), json!(4), json!(1.0))
    );
    assert_eq!(get(&a, &owner_b, ta, "").await.status, 404);

    // ADR-0012: purge isi percakapan tidak mengubah analitik.
    sqlx::query("UPDATE conversations SET last_message_at = now() - interval '40 days' WHERE tenant_id = $1").bind(ta).execute(&pool).await.unwrap();
    assert_eq!(
        repo::purge(&app_db, Duration::from_secs(30 * 86_400))
            .await
            .unwrap(),
        3
    );
    let after = get(&a, &owner, ta, "").await.json()["data"].clone();
    for k in [
        "total_conversations",
        "ai_resolved",
        "human_resolved",
        "avg_first_response_ms",
        "avg_resolution_ms",
        "handover_rate",
        "fallback_rate",
        "provider_error_rate",
        "tokens",
    ] {
        assert_eq!(after[k], d[k], "{k} berubah setelah purge");
    }

    // Izin: Viewer (analytics.read) boleh; Support Agent tidak.
    assert_eq!(get(&a, &viewer, ta, "").await.status, 200);
    assert_eq!(get(&a, &staff, ta, "").await.status, 403);
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn range_validation(pool: PgPool) {
    let a = app(&pool).await;
    let t = tenant(&pool, "toko-a").await;
    let owner = member(&pool, t, "Owner").await;
    let ok = get(&a, &owner, t, "?from=2026-01-01&to=2026-01-31").await;
    assert_eq!(ok.status, 200);
    let d: Value = ok.json()["data"].clone();
    assert_eq!(
        (d["from"].clone(), d["to"].clone()),
        (json!("2026-01-01"), json!("2026-01-31"))
    );
    assert_eq!(d["daily"].as_array().unwrap().len(), 31);
    assert!(
        d["avg_first_response_ms"].is_null() && d["handover_rate"].is_null(),
        "tanpa data → null, bukan 0/NaN"
    );
    for q in [
        "?from=2026-02-30",
        "?from=13-01-2026",
        "?from=2026-03-01&to=2026-02-01",
        "?from=2025-01-01&to=2026-06-01",
        "?to=2026-1-1",
    ] {
        let r = get(&a, &owner, t, q).await;
        assert_eq!(r.status, 422, "{q}: {}", String::from_utf8_lossy(&r.bytes));
    }
}
