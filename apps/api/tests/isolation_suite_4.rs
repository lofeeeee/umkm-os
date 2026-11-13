//! Isolation suite #4 (PRD.md §13.2, RTM §6): tenant A tidak dapat membaca conversation, konfigurasi AI,
//! ticket, order, atau file milik tenant B — lewat jalur HTTP nyata (path tenant sendiri dengan id milik B,
//! path tenant B, signed URL tiruan) DAN langsung di DB sebagai role aplikasi (RLS). Kontrol positif: B
//! membaca miliknya. Gate P6.

mod common;

use axum::http::Method;
use common::*;
use serde_json::json;
use sqlx::PgPool;
use umkm_database::{MIGRATOR, begin_tenant_tx};
use umkm_domain::conversation::ConversationStatus;
use umkm_infrastructure::conversation_repo::{self as repo, StartInput};
use umkm_shared::ids::TenantId;
use uuid::Uuid;

struct Resources {
    conversation: Uuid,
    ticket: Uuid,
    order: Uuid,
    object: Uuid,
}

/// Semua jenis resource untuk tenant `t` (dibuat lewat repo/HTTP/SQL seperti alur aslinya).
async fn seed(a: &TestApp, pool: &PgPool, t: Uuid, owner: &Who, secret: &str) -> Resources {
    let tid = TenantId::from_uuid(t);
    let app = app_pool(pool).await;
    let conversation = repo::start(
        &app,
        tid,
        &StartInput {
            consent_version: "v0",
            customer_id: None,
            initial: ConversationStatus::Ai,
        },
    )
    .await
    .unwrap()
    .conversation_id;
    repo::append(
        &app,
        tid,
        conversation,
        &repo::NewMessage {
            sender: umkm_domain::conversation::SenderType::Customer,
            session_id: None,
            sender_user_id: None,
            body: secret,
            ai_run_id: None,
        },
    )
    .await
    .unwrap();
    let prov: Uuid = sqlx::query_scalar(
        "INSERT INTO ai_providers (tenant_id, name, source, provider_type, model, capabilities)
         VALUES ($1, 'Platform', 'platform', 'platform', 'm1', '{\"chat\":true}') RETURNING id",
    )
    .bind(t)
    .fetch_one(pool)
    .await
    .unwrap();
    sqlx::query("INSERT INTO chatbot_configs (tenant_id, enabled, primary_provider_id, system_prompt) VALUES ($1, true, $2, $3)")
        .bind(t)
        .bind(prov)
        .bind(secret)
        .execute(pool)
        .await
        .unwrap();
    let ticket: Uuid = sqlx::query_scalar(
        "INSERT INTO tickets (tenant_id, code, source, conversation_id, subject, first_response_due_at, resolution_due_at)
         VALUES ($1, 'TKT-' || upper(substr(md5(random()::text), 1, 10)), 'chat', $2, $3, now() + interval '1 hour', now() + interval '1 day')
         RETURNING id",
    )
    .bind(t)
    .bind(conversation)
    .bind(secret)
    .fetch_one(pool)
    .await
    .unwrap();
    let mut tx = begin_tenant_tx(&app, tid).await.unwrap();
    let customer: Uuid = sqlx::query_scalar(
        "INSERT INTO customers (tenant_id, name, email) VALUES ($1, $2, 's@x.id') RETURNING id",
    )
    .bind(t)
    .bind(secret)
    .fetch_one(&mut *tx)
    .await
    .unwrap();
    let order: Uuid = sqlx::query_scalar(
        "INSERT INTO orders (tenant_id, code, customer_id, status, fulfillment, customer_name, subtotal, delivery_fee, total,
             access_token_hash, idempotency_key, request_hash, privacy_version, terms_version, consented_at, expires_at)
         VALUES ($1, 'ORD-' || upper(substr(md5(random()::text), 1, 6)), $2, 'AWAITING_PAYMENT', 'pickup', $3, 100, 0, 100,
             sha256(gen_random_uuid()::text::bytea), gen_random_uuid()::text, sha256(gen_random_uuid()::text::bytea),
             'v0', 'v0', now(), now() + interval '1 hour') RETURNING id",
    )
    .bind(t)
    .bind(customer)
    .bind(secret)
    .fetch_one(&mut *tx)
    .await
    .unwrap();
    tx.commit().await.unwrap();
    let up = upload(&a.router, t, owner, png()).await;
    let object = Uuid::parse_str(up.json()["data"]["id"].as_str().unwrap()).unwrap();
    Resources {
        conversation,
        ticket,
        order,
        object,
    }
}

async fn status(a: &TestApp, w: &Who, uri: &str) -> u16 {
    json(&a.router, Method::GET, uri, Some(w), None)
        .await
        .status
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn tenant_a_cannot_read_conversation_ai_ticket_order_file_of_b(pool: PgPool) {
    let a = app(&pool).await;
    let ta = tenant(&pool, "toko-a").await;
    let tb = tenant(&pool, "toko-b").await;
    let oa = member(&pool, ta, "Owner").await;
    let ob = member(&pool, tb, "Owner").await;
    let ra = seed(&a, &pool, ta, &oa, "rahasia-toko-a").await;
    let rb = seed(&a, &pool, tb, &ob, "rahasia-toko-b").await;

    let paths = |t: Uuid, r: &Resources| {
        [
            format!("/api/v1/tenants/{t}/conversations/{}", r.conversation),
            format!("/api/v1/tenants/{t}/tickets/{}", r.ticket),
            format!("/api/v1/tenants/{t}/orders/{}", r.order),
            format!("/api/v1/tenants/{t}/ai/config"),
        ]
    };
    // Kontrol positif: pemilik membaca miliknya.
    for p in paths(tb, &rb) {
        assert_eq!(status(&a, &ob, &p).await, 200, "{p}");
    }
    // 1) Path tenant B → bukan anggota → 404.
    for p in paths(tb, &rb) {
        assert_eq!(status(&a, &oa, &p).await, 404, "A lewat path B: {p}");
    }
    // 2) Path tenant A dengan id milik B → tidak ditemukan.
    for p in &paths(ta, &rb)[..3] {
        assert_eq!(status(&a, &oa, p).await, 404, "A dengan id B: {p}");
    }
    // 3) Daftar & konfigurasi AI tenant A tidak memuat data B.
    for p in [
        format!("/api/v1/tenants/{ta}/conversations"),
        format!("/api/v1/tenants/{ta}/tickets"),
        format!("/api/v1/tenants/{ta}/orders"),
        format!("/api/v1/tenants/{ta}/ai/config"),
        format!("/api/v1/tenants/{ta}/uploads"),
        format!("/api/v1/tenants/{ta}/conversations/{}", ra.conversation),
    ] {
        let r = json(&a.router, Method::GET, &p, Some(&oa), None).await;
        assert_eq!(r.status, 200, "{p}");
        let body = String::from_utf8_lossy(&r.bytes);
        assert!(!body.contains("rahasia-toko-b"), "{p} bocor isi B");
        for id in [rb.conversation, rb.ticket, rb.order, rb.object] {
            assert!(!body.contains(&id.to_string()), "{p} memuat id milik B");
        }
    }
    // 4) File: signed URL bertanda tangan sah untuk tenant A tetapi objek milik B → 404; milik A → 200.
    let now = umkm_api::upload::now_unix();
    let get = |u: String| {
        let r = a.router.clone();
        async move {
            send(&r, Method::GET, &u, None, None, None, vec![])
                .await
                .status
        }
    };
    assert_eq!(
        get(a.signer.sign(TenantId::from_uuid(ta), rb.object, now)).await,
        404
    );
    assert_eq!(
        get(a.signer.sign(TenantId::from_uuid(ta), ra.object, now)).await,
        200
    );

    // 5) Langsung di DB sebagai role aplikasi dalam konteks tenant A: 0 baris milik B.
    let app_db = app_pool(&pool).await;
    let mut tx = begin_tenant_tx(&app_db, TenantId::from_uuid(ta))
        .await
        .unwrap();
    for (table, id) in [
        ("conversations", rb.conversation),
        ("tickets", rb.ticket),
        ("orders", rb.order),
        ("stored_objects", rb.object),
    ] {
        let n: i64 = sqlx::query_scalar(sqlx::AssertSqlSafe(format!(
            "SELECT count(*) FROM {table} WHERE id = $1"
        )))
        .bind(id)
        .fetch_one(&mut *tx)
        .await
        .unwrap();
        assert_eq!(n, 0, "{table}: baris B terlihat dari A");
    }
    for table in [
        "conversation_messages",
        "chatbot_configs",
        "ai_providers",
        "conversation_daily_stats",
    ] {
        let n: i64 = sqlx::query_scalar(sqlx::AssertSqlSafe(format!(
            "SELECT count(*) FROM {table} WHERE tenant_id = $1"
        )))
        .bind(tb)
        .fetch_one(&mut *tx)
        .await
        .unwrap();
        assert_eq!(n, 0, "{table}: baris B terlihat dari A");
    }
    tx.rollback().await.unwrap();
    let _ = json!({});
}
