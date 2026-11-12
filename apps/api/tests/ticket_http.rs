//! P4-3: ticketing (AC-TKT-001..005): sumber, lifecycle + paritas DB, assign, catatan
//! internal tersembunyi dari customer, riwayat, reopen, SLA, cakupan agen.

mod common;

use axum::http::{Method, header};
use common::*;
use serde_json::{Value, json};
use sqlx::PgPool;
use umkm_database::MIGRATOR;
use umkm_domain::tickets::{ALL_STATUSES, Priority, Source};
use umkm_infrastructure::ticket_repo::{self as repo, NewTicket};
use umkm_shared::ids::TenantId;
use uuid::Uuid;

const HOST: &str = "toko-a.ryasai.my.id";

fn base(t: Uuid) -> String {
    format!("/api/v1/tenants/{t}/tickets")
}

async fn call(a: &TestApp, m: Method, url: String, who: &Who, body: Option<Value>) -> Res {
    json(&a.router, m, &url, Some(who), body).await
}

async fn public(a: &TestApp, m: Method, path: &str, token: &str, body: Option<Value>) -> Res {
    use http_body_util::BodyExt as _;
    use tower::ServiceExt as _;
    let b = axum::http::Request::builder()
        .method(m)
        .uri(path)
        .header(header::HOST, HOST)
        .header(header::CONTENT_TYPE, "application/json")
        .header("x-order-token", token);
    let res = a
        .router
        .clone()
        .oneshot(
            b.body(axum::body::Body::from(
                body.map(|v| v.to_string()).unwrap_or_default(),
            ))
            .unwrap(),
        )
        .await
        .unwrap();
    let status = res.status().as_u16();
    let headers = res.headers().clone();
    let bytes = res.into_body().collect().await.unwrap().to_bytes().to_vec();
    Res {
        status,
        headers,
        bytes,
    }
}

/// Toko + satu pesanan guest; kembalikan (order_id, token).
async fn order(a: &TestApp, t: Uuid, o: &Who) -> (Uuid, String) {
    sqlx::query("UPDATE business_profiles SET pickup_available = true WHERE tenant_id = $1")
        .bind(t)
        .execute(&a.owner)
        .await
        .unwrap();
    let p = call(
        a,
        Method::POST,
        format!("/api/v1/tenants/{t}/products"),
        o,
        Some(json!({"name": "Kopi", "variants": [{"sku": "K-1", "price": "15000"}]})),
    )
    .await
    .json()["data"]
        .clone();
    let (pid, vid) = (
        p["id"].as_str().unwrap().to_owned(),
        p["variants"][0]["id"].as_str().unwrap().to_owned(),
    );
    call(
        a,
        Method::POST,
        format!("/api/v1/tenants/{t}/products/{pid}/status"),
        o,
        Some(json!({"status": "active"})),
    )
    .await;
    call(
        a,
        Method::POST,
        format!("/api/v1/tenants/{t}/inventory/{vid}/movements"),
        o,
        Some(json!({"kind": "stock_in", "quantity": 5, "reason": "awal"})),
    )
    .await;
    let q = upload(&a.router, t, o, png()).await.json()["data"]["id"]
        .as_str()
        .unwrap()
        .to_owned();
    call(a, Method::PUT, format!("/api/v1/tenants/{t}/commerce/settings"), o, Some(json!({"qris": format!("asset:{q}"), "qris_receiver_name": "T", "checkout_enabled": true}))).await;
    let body = json!({"items": [{"variant_id": vid, "quantity": 1}], "fulfillment": "pickup", "name": "Sari", "email": "s@x.id", "accept_privacy": true, "accept_terms": true});
    use http_body_util::BodyExt as _;
    use tower::ServiceExt as _;
    let req = axum::http::Request::builder()
        .method(Method::POST)
        .uri("/api/v1/public/checkout")
        .header(header::HOST, HOST)
        .header(header::CONTENT_TYPE, "application/json")
        .header("idempotency-key", "key-tkt-0001")
        .body(axum::body::Body::from(body.to_string()))
        .unwrap();
    let res = a.router.clone().oneshot(req).await.unwrap();
    let v: Value =
        serde_json::from_slice(&res.into_body().collect().await.unwrap().to_bytes()).unwrap();
    let token = v["data"]["access_token"].as_str().unwrap().to_owned();
    let id: Uuid = sqlx::query_scalar("SELECT id FROM orders WHERE tenant_id = $1")
        .bind(t)
        .fetch_one(&a.owner)
        .await
        .unwrap();
    (id, token)
}

async fn st(a: &TestApp, url: &str, who: &Who, s: &str) -> Res {
    call(
        a,
        Method::POST,
        url.to_owned(),
        who,
        Some(json!({"status": s})),
    )
    .await
}

fn reason(r: &Res) -> Value {
    r.json()["error"]["details"]["reason"].clone()
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn tickets_from_every_source_stay_in_tenant(pool: PgPool) {
    // AC-TKT-001
    let a = app(&pool).await;
    let (t, tb) = (tenant(&pool, "toko-a").await, tenant(&pool, "toko-b").await);
    let o = member(&pool, t, "Owner").await;
    let ob = member(&pool, tb, "Owner").await;
    let (oid, token) = order(&a, t, &o).await;
    // order (customer)
    let r = public(
        &a,
        Method::POST,
        "/api/v1/public/orders/current/tickets",
        &token,
        Some(json!({"subject": "Kopi tumpah", "description": "Sampai berantakan"})),
    )
    .await;
    assert_eq!(r.status, 201, "{}", r.json());
    assert!(
        r.json()["data"]["code"]
            .as_str()
            .unwrap()
            .starts_with("TKT-")
    );
    // manual (staf) dengan rujukan order tenant ini
    let r = call(
        &a,
        Method::POST,
        base(t),
        &o,
        Some(json!({"subject": "Telepon pelanggan", "order_id": oid, "priority": "high"})),
    )
    .await;
    assert_eq!(r.status, 201, "{}", r.json());
    assert_eq!(
        (
            r.json()["data"]["source"].clone(),
            r.json()["data"]["priority"].clone()
        ),
        (json!("manual"), json!("high"))
    );
    // rujukan order tenant lain → ditolak
    let r = call(
        &a,
        Method::POST,
        base(tb),
        &ob,
        Some(json!({"subject": "x", "order_id": oid})),
    )
    .await;
    assert_eq!(
        r.json()["error"]["details"]["errors"][0]["code"],
        "invalid_reference"
    );
    // system & chat lewat repo
    let db = a.state.db.clone().unwrap();
    let tid = TenantId::from_uuid(t);
    let mk = |source, conv| NewTicket {
        source,
        customer_id: None,
        order_id: None,
        conversation_id: conv,
        subject: "Otomatis".into(),
        description: None,
        priority: Priority::Low,
        tags: vec![],
        assignee: None,
    };
    repo::create(&db, tid, &mk(Source::System, None), None, (None, None))
        .await
        .unwrap();
    let conv = Uuid::new_v4();
    repo::create(&db, tid, &mk(Source::Chat, Some(conv)), None, (None, None))
        .await
        .unwrap();
    repo::create(&db, tid, &mk(Source::Chat, Some(conv)), None, (None, None))
        .await
        .unwrap();
    assert!(
        repo::create(&db, tid, &mk(Source::Chat, None), None, (None, None))
            .await
            .is_err(),
        "chat tanpa conversation"
    );
    let sources: Vec<String> =
        sqlx::query_scalar("SELECT source FROM tickets WHERE tenant_id = $1 ORDER BY source")
            .bind(t)
            .fetch_all(&a.owner)
            .await
            .unwrap();
    assert_eq!(
        sources,
        ["chat", "chat", "manual", "order", "system"],
        "satu conversation → ≥1 ticket"
    );
    assert_eq!(
        call(&a, Method::GET, base(tb), &ob, None).await.json()["data"]["total"],
        0
    );
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn lifecycle_assignment_history_and_sla(pool: PgPool) {
    // AC-TKT-002, AC-TKT-003, AC-TKT-005
    let a = app(&pool).await;
    let (t, tb) = (tenant(&pool, "toko-a").await, tenant(&pool, "toko-b").await);
    let o = member(&pool, t, "Owner").await;
    let agent = member(&pool, t, "Support Agent").await;
    let agent2 = member(&pool, t, "Support Agent").await;
    let outsider = member(&pool, tb, "Owner").await;
    let id = call(
        &a,
        Method::POST,
        base(t),
        &o,
        Some(json!({"subject": "Pesanan belum sampai", "priority": "urgent", "tags": ["Kirim"]})),
    )
    .await
    .json()["data"]["id"]
        .as_str()
        .unwrap()
        .to_owned();
    let url = |tail: &str| format!("{}/{id}{tail}", base(t));
    let status_url = url("/status");

    // agen tidak melihat ticket yang belum ditugaskan kepadanya; tidak bisa assign
    assert_eq!(
        call(&a, Method::GET, url(""), &agent, None).await.status,
        404
    );
    assert_eq!(
        call(&a, Method::GET, base(t), &agent, None).await.json()["data"]["total"],
        0
    );
    assert_eq!(
        call(
            &a,
            Method::POST,
            url("/assign"),
            &agent,
            Some(json!({"assignee_user_id": agent.user}))
        )
        .await
        .status,
        403
    );
    // assignee wajib anggota aktif tenant
    let r = call(
        &a,
        Method::POST,
        url("/assign"),
        &o,
        Some(json!({"assignee_user_id": outsider.user})),
    )
    .await;
    assert_eq!(
        r.json()["error"]["details"]["errors"][0]["code"],
        "not_a_member"
    );
    let r = call(
        &a,
        Method::POST,
        url("/assign"),
        &o,
        Some(json!({"assignee_user_id": agent.user})),
    )
    .await;
    assert_eq!(
        (
            r.json()["data"]["status"].clone(),
            r.json()["data"]["assignee_user_id"].clone()
        ),
        (json!("ASSIGNED"), json!(agent.user.to_string()))
    );
    assert_eq!(
        call(&a, Method::GET, url(""), &agent, None).await.status,
        200
    );
    assert_eq!(
        call(&a, Method::GET, url(""), &agent2, None).await.status,
        404,
        "agen lain tidak melihat"
    );

    // transisi tidak terdaftar → 409
    assert_eq!(st(&a, &status_url, &agent, "IN_PROGRESS").await.status, 200);
    assert_eq!(
        reason(&st(&a, &status_url, &agent, "CLOSED").await),
        json!("invalid_ticket_status"),
        "IN_PROGRESS → CLOSED tidak terdaftar"
    );
    // SLA: balasan publik pertama mengisi first_responded_at; catatan internal tidak
    call(
        &a,
        Method::POST,
        url("/messages"),
        &agent,
        Some(json!({"body": "Catatan: cek kurir", "internal": true})),
    )
    .await;
    let v = call(&a, Method::GET, url(""), &agent, None).await.json()["data"].clone();
    assert_eq!(v["first_responded_at"], Value::Null);
    call(
        &a,
        Method::POST,
        url("/messages"),
        &agent,
        Some(json!({"body": "Sedang kami cek"})),
    )
    .await;
    let v = call(&a, Method::GET, url(""), &agent, None).await.json()["data"].clone();
    assert!(v["first_responded_at"].is_string() && v["sla_first_response_breached"] == false);
    let r = st(&a, &status_url, &agent, "RESOLVED").await;
    assert!(
        r.json()["data"]["resolved_at"].is_string(),
        "resolved_at terisi saat RESOLVED"
    );
    let r = st(&a, &status_url, &agent, "CLOSED").await;
    assert!(r.json()["data"]["closed_at"].is_string());
    // reopen dalam 7 hari boleh; lewat jendela → 409
    let r = st(&a, &status_url, &agent, "IN_PROGRESS").await;
    assert_eq!(
        (
            r.json()["data"]["status"].clone(),
            r.json()["data"]["reopen_count"].clone(),
            r.json()["data"]["resolved_at"].clone()
        ),
        (json!("IN_PROGRESS"), json!(1), Value::Null)
    );
    st(&a, &status_url, &agent, "RESOLVED").await;
    st(&a, &status_url, &agent, "CLOSED").await;
    sqlx::query("UPDATE tickets SET closed_at = now() - interval '8 days' WHERE id = $1::uuid")
        .bind(&id)
        .execute(&a.owner)
        .await
        .unwrap();
    assert_eq!(
        reason(&st(&a, &status_url, &agent, "IN_PROGRESS").await),
        json!("reopen_expired")
    );
    // SLA terlewati terdeteksi
    sqlx::query("UPDATE tickets SET resolution_due_at = resolved_at - interval '1 hour', first_response_due_at = created_at WHERE id = $1::uuid")
        .bind(&id).execute(&a.owner).await.unwrap();
    let v = call(&a, Method::GET, url(""), &o, None).await.json()["data"].clone();
    assert_eq!(v["sla_resolution_breached"], true);
    // riwayat lengkap & audit
    let kinds: Vec<String> = v["events"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| e["kind"].as_str().unwrap().to_owned())
        .collect();
    assert_eq!(
        kinds,
        [
            "created",
            "assigned",
            "status_changed",
            "internal_note",
            "public_reply",
            "status_changed",
            "status_changed",
            "reopened",
            "status_changed",
            "status_changed"
        ]
    );
    let audits: Vec<String> = sqlx::query_scalar("SELECT action FROM audit_logs WHERE tenant_id = $1 AND resource_type = 'ticket' ORDER BY created_at, id")
        .bind(t).fetch_all(&a.owner).await.unwrap();
    assert!(
        audits.contains(&"ticket.assigned".to_owned())
            && audits.contains(&"ticket.closed".to_owned()),
        "{audits:?}"
    );
    // pencarian & filter
    let code = v["code"].as_str().unwrap().to_owned();
    assert_eq!(
        call(
            &a,
            Method::GET,
            format!("{}?q={}", base(t), &code[4..]),
            &o,
            None
        )
        .await
        .json()["data"]["total"],
        1
    );
    assert_eq!(
        call(
            &a,
            Method::GET,
            format!("{}?tag=kirim&status=CLOSED", base(t)),
            &o,
            None
        )
        .await
        .json()["data"]["total"],
        1
    );
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn internal_notes_never_reach_customer(pool: PgPool) {
    // AC-TKT-004
    let a = app(&pool).await;
    let t = tenant(&pool, "toko-a").await;
    let o = member(&pool, t, "Owner").await;
    let (_, token) = order(&a, t, &o).await;
    let r = public(
        &a,
        Method::POST,
        "/api/v1/public/orders/current/tickets",
        &token,
        Some(json!({"subject": "Rusak"})),
    )
    .await;
    let id = r.json()["data"]["id"].as_str().unwrap().to_owned();
    let url = |tail: &str| format!("{}/{id}{tail}", base(t));
    call(
        &a,
        Method::POST,
        url("/messages"),
        &o,
        Some(json!({"body": "RAHASIA: pelanggan ini sering komplain", "internal": true})),
    )
    .await;
    call(
        &a,
        Method::POST,
        url("/messages"),
        &o,
        Some(json!({"body": "Mohon kirim foto"})),
    )
    .await;
    send(
        &a.router,
        Method::POST,
        &format!("{}?internal=true", url("/attachments")),
        Some(&o),
        None,
        Some("image/png"),
        png(),
    )
    .await;
    send(
        &a.router,
        Method::POST,
        &url("/attachments"),
        Some(&o),
        None,
        Some("image/png"),
        png(),
    )
    .await;
    call(
        &a,
        Method::POST,
        url("/status"),
        &o,
        Some(json!({"status": "IN_PROGRESS"})),
    )
    .await;
    call(
        &a,
        Method::POST,
        url("/status"),
        &o,
        Some(json!({"status": "WAITING_CUSTOMER"})),
    )
    .await;

    let r = public(
        &a,
        Method::GET,
        "/api/v1/public/orders/current/tickets",
        &token,
        None,
    )
    .await;
    let raw = String::from_utf8(r.bytes.clone()).unwrap();
    assert!(
        !raw.contains("RAHASIA")
            && !raw.contains("\"internal\"")
            && !raw.contains("events")
            && !raw.contains("assignee"),
        "{raw}"
    );
    assert!(raw.contains("Mohon kirim foto"));
    assert_eq!(
        r.json()["data"][0]["attachments"].as_array().unwrap().len(),
        1
    );
    // lampiran internal tidak dapat diunduh customer walau id diketahui
    let internal_id: Uuid =
        sqlx::query_scalar("SELECT id FROM ticket_attachments WHERE visibility = 'internal'")
            .fetch_one(&a.owner)
            .await
            .unwrap();
    assert_eq!(
        public(
            &a,
            Method::GET,
            &format!("/api/v1/public/orders/current/tickets/{id}/attachments/{internal_id}"),
            &token,
            None
        )
        .await
        .status,
        404
    );
    // balasan customer: WAITING_CUSTOMER → IN_PROGRESS
    let r = public(
        &a,
        Method::POST,
        &format!("/api/v1/public/orders/current/tickets/{id}/messages"),
        &token,
        Some(json!({"body": "Ini fotonya"})),
    )
    .await;
    assert_eq!(r.json()["data"]["status"], "IN_PROGRESS");
    let n: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM notification_outbox WHERE event_type = 'ticket.customer_reply'",
    )
    .fetch_one(&a.owner)
    .await
    .unwrap();
    assert_eq!(n, 1);
    // token salah / pesanan lain → tidak terlihat
    assert_eq!(
        public(
            &a,
            Method::GET,
            "/api/v1/public/orders/current/tickets",
            "salah",
            None
        )
        .await
        .status,
        404
    );
    // DB: customer tidak bisa menulis catatan internal
    let e = sqlx::query("INSERT INTO ticket_messages (tenant_id, ticket_id, visibility, author_type, body) VALUES ($1, $2::uuid, 'internal', 'customer', 'x')")
        .bind(t).bind(&id).execute(&a.owner).await.unwrap_err();
    assert_eq!(
        e.as_database_error().unwrap().code().as_deref(),
        Some("23514")
    );
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn db_ticket_transitions_match_domain(pool: PgPool) {
    let mut n = 0;
    for a in ALL_STATUSES {
        for b in ALL_STATUSES {
            let db: bool = sqlx::query_scalar("SELECT app.ticket_transition_allowed($1, $2)")
                .bind(a.as_str())
                .bind(b.as_str())
                .fetch_one(&pool)
                .await
                .unwrap();
            assert_eq!(db, a.can_transition(b), "{a:?} -> {b:?}");
            n += 1;
        }
    }
    assert_eq!(n, 49);
    // history append-only bagi role aplikasi
    let a = app(&pool).await;
    let t = tenant(&pool, "toko-a").await;
    let o = member(&pool, t, "Owner").await;
    call(&a, Method::POST, base(t), &o, Some(json!({"subject": "x"}))).await;
    let app_db = app_pool(&pool).await;
    for sql in [
        "UPDATE ticket_events SET kind = 'x'",
        "DELETE FROM ticket_events",
        "DELETE FROM tickets",
        "UPDATE ticket_messages SET body = 'x'",
    ] {
        let mut tx = umkm_database::begin_tenant_tx(&app_db, TenantId::from_uuid(t))
            .await
            .unwrap();
        let e = sqlx::query(sqlx::AssertSqlSafe(sql))
            .execute(&mut *tx)
            .await
            .unwrap_err();
        assert_eq!(
            e.as_database_error().unwrap().code().as_deref(),
            Some("42501"),
            "{sql}"
        );
    }
}
