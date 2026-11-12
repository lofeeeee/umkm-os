//! P3-3: customer & CRM (AC-CRM-001/002/004/005/006), akun customer per toko (ADR-0005).

mod common;

use std::time::Duration;

use axum::http::{Method, header};
use common::*;
use serde_json::{Value, json};
use sqlx::PgPool;
use umkm_application::notify::OutboxEvent;
use umkm_database::MIGRATOR;
use umkm_domain::{auth::AuthPolicy, crm::Contact};
use umkm_infrastructure::{
    crm_repo,
    metrics::Metrics,
    outbox::{CapturingEmailSender, WorkerConfig, process_once},
    retention::{RetentionPolicy, run_purge},
};
use umkm_shared::ids::TenantId;
use uuid::Uuid;

const HOST_A: &str = "toko-a.ryasai.my.id";
const HOST_B: &str = "toko-b.ryasai.my.id";

fn customers(t: Uuid) -> String {
    format!("/api/v1/tenants/{t}/customers")
}

async fn staff(a: &TestApp, m: Method, url: String, who: &Who, body: Option<Value>) -> Res {
    json(&a.router, m, &url, Some(who), body).await
}

/// Request publik ke host toko (cookie customer + Origin opsional).
async fn shop(
    a: &TestApp,
    m: Method,
    host: &str,
    path: &str,
    cookie: Option<&(String, String)>,
    origin: Option<&str>,
    body: Option<Value>,
) -> Res {
    let mut b = axum::http::Request::builder()
        .method(m)
        .uri(path)
        .header(header::HOST, host)
        .header(header::CONTENT_TYPE, "application/json");
    if let Some((c, csrf)) = cookie {
        b = b.header(header::COOKIE, c).header("x-csrf-token", csrf);
    }
    if let Some(o) = origin {
        b = b.header(header::ORIGIN, o);
    }
    let body = body.map(|v| v.to_string()).unwrap_or_default();
    use tower::ServiceExt as _;
    let res = a
        .router
        .clone()
        .oneshot(b.body(axum::body::Body::from(body)).unwrap())
        .await
        .unwrap();
    let status = res.status().as_u16();
    let headers = res.headers().clone();
    use http_body_util::BodyExt as _;
    let bytes = res.into_body().collect().await.unwrap().to_bytes().to_vec();
    Res {
        status,
        headers,
        bytes,
    }
}

async fn guest(a: &TestApp, t: Uuid, name: &str, email: &str) -> Uuid {
    let db = a.state.db.clone().unwrap();
    let tid = TenantId::from_uuid(t);
    let mut tx = umkm_database::begin_tenant_tx(&db, tid).await.unwrap();
    let id = crm_repo::create_in(
        &mut tx,
        tid,
        &Contact {
            name: name.into(),
            email: Some(email.into()),
            phone: None,
        },
        None,
    )
    .await
    .unwrap();
    tx.commit().await.unwrap();
    id
}

async fn register(a: &TestApp, host: &str, email: &str) -> Res {
    shop(
        a,
        Method::POST,
        host,
        "/api/v1/public/customer/register",
        None,
        Some(&format!("https://{host}")),
        Some(json!({"email": email, "password": "rahasia-panjang-1", "accept_privacy": true, "accept_terms": true})),
    )
    .await
}

async fn login(a: &TestApp, host: &str, email: &str) -> Option<(String, String)> {
    let r = shop(
        a,
        Method::POST,
        host,
        "/api/v1/public/customer/login",
        None,
        Some(&format!("https://{host}")),
        Some(json!({"email": email, "password": "rahasia-panjang-1"})),
    )
    .await;
    if r.status != 200 {
        return None;
    }
    let cookie = r.headers[header::SET_COOKIE]
        .to_str()
        .unwrap()
        .split(';')
        .next()
        .unwrap()
        .to_owned();
    assert!(cookie.starts_with("__Host-umkm_cust="), "{cookie}");
    Some((
        cookie,
        r.json()["data"]["csrf_token"].as_str().unwrap().to_owned(),
    ))
}

/// Jalankan worker outbox (role aplikasi) dan kembalikan token dari email verifikasi terakhir.
async fn deliver_verification(a: &TestApp) -> String {
    let sender = CapturingEmailSender::default();
    let cfg = WorkerConfig {
        worker_id: "w".into(),
        batch: 10,
        lease: Duration::from_secs(30),
        max_attempts: 3,
        base_backoff: Duration::from_secs(1),
        verify_url_base: "https://app.ryasai.my.id/verify-email".into(),
        policy: AuthPolicy::default(),
    };
    process_once(&a.state.db.clone().unwrap(), &sender, &cfg, &Metrics::new())
        .await
        .unwrap();
    let sent = sender.sent.lock().unwrap();
    let m = sent
        .iter()
        .rev()
        .find(|m| m.template == "customer_email_verification")
        .expect("email verifikasi");
    let body = &m.text_body;
    let i = body.find("#token=").unwrap() + 7;
    body[i..].split_whitespace().next().unwrap().to_owned()
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn staff_crm_is_tenant_isolated_and_role_gated(pool: PgPool) {
    // AC-CRM-002, AC-CRM-004 (staf)
    let a = app(&pool).await;
    let (ta, tb) = (tenant(&pool, "toko-a").await, tenant(&pool, "toko-b").await);
    let (oa, ob) = (
        member(&pool, ta, "Owner").await,
        member(&pool, tb, "Owner").await,
    );
    let support = member(&pool, ta, "Support Agent").await;
    let catalog = member(&pool, ta, "Catalog Manager").await;
    let r = staff(
        &a,
        Method::POST,
        customers(ta),
        &oa,
        Some(json!({
            "name": "Sari", "email": "Sari@Mail.id", "phone": "0812 3456 7890",
            "tags": ["VIP", "vip", "Langganan"], "internal_note": "Suka telat bayar"
        })),
    )
    .await;
    assert_eq!(r.status, 201, "{}", r.json());
    let c = r.json()["data"].clone();
    assert_eq!(c["email"], "sari@mail.id");
    assert_eq!(c["phone"], "+6281234567890");
    assert_eq!(c["tags"], json!(["langganan", "vip"]));
    let id = c["id"].as_str().unwrap().to_owned();

    assert_eq!(
        staff(
            &a,
            Method::GET,
            format!("{}?q=sari@mail", customers(ta)),
            &oa,
            None
        )
        .await
        .json()["data"]["total"],
        1
    );
    assert_eq!(
        staff(
            &a,
            Method::GET,
            format!("{}?q=6281234", customers(ta)),
            &oa,
            None
        )
        .await
        .json()["data"]["total"],
        1
    );
    assert_eq!(
        staff(
            &a,
            Method::GET,
            format!("{}?q=sari@mail", customers(tb)),
            &ob,
            None
        )
        .await
        .json()["data"]["total"],
        0,
        "tenant B tidak menemukan"
    );
    assert_eq!(
        staff(
            &a,
            Method::GET,
            format!("{}/{id}", customers(tb)),
            &ob,
            None
        )
        .await
        .status,
        404
    );
    assert_eq!(
        staff(
            &a,
            Method::GET,
            format!("{}?tag=VIP", customers(ta)),
            &oa,
            None
        )
        .await
        .json()["data"]["total"],
        1
    );

    // Support Agent baca saja; Catalog Manager tidak punya akses CRM
    assert_eq!(
        staff(
            &a,
            Method::GET,
            format!("{}/{id}", customers(ta)),
            &support,
            None
        )
        .await
        .status,
        200
    );
    let body = json!({"name": "X", "email": "x@x.id"});
    assert_eq!(
        staff(
            &a,
            Method::PUT,
            format!("{}/{id}", customers(ta)),
            &support,
            Some(body)
        )
        .await
        .status,
        403
    );
    assert_eq!(
        staff(&a, Method::GET, customers(ta), &catalog, None)
            .await
            .status,
        403
    );

    // validasi
    let r = staff(
        &a,
        Method::POST,
        customers(ta),
        &oa,
        Some(json!({"name": "Tanpa kontak"})),
    )
    .await;
    assert_eq!(r.status, 422);
    let r = staff(
        &a,
        Method::PUT,
        format!("{}/{id}/addresses", customers(ta)),
        &oa,
        Some(json!([{"address": "Jl. Mawar 1", "label": "Rumah"}])),
    )
    .await;
    assert_eq!(r.json()["data"]["addresses"][0]["address"], "Jl. Mawar 1");
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn guests_with_same_email_are_never_merged_and_accounts_are_per_shop(pool: PgPool) {
    // AC-CRM-001
    let a = app(&pool).await;
    let (ta, _tb) = (tenant(&pool, "toko-a").await, tenant(&pool, "toko-b").await);
    let g1 = guest(&a, ta, "Sari", "sari@x.id").await;
    let g2 = guest(&a, ta, "Penipu", "sari@x.id").await;
    assert_ne!(g1, g2, "email sama = record terpisah");

    assert_eq!(register(&a, HOST_A, "sari@x.id").await.status, 201);
    assert_eq!(
        register(&a, HOST_A, "SARI@x.id").await.json()["error"]["details"]["reason"],
        "email_taken"
    );
    // akun toko A tidak berlaku di toko B
    assert!(login(&a, HOST_B, "sari@x.id").await.is_none());
    let s = login(&a, HOST_A, "sari@x.id").await.unwrap();
    // belum terverifikasi → tidak ada customer guest yang terlihat
    let me = shop(
        &a,
        Method::GET,
        HOST_A,
        "/api/v1/public/customer/me",
        Some(&s),
        None,
        None,
    )
    .await;
    assert_eq!(me.json()["data"]["email_verified"], false);
    assert_eq!(me.json()["data"]["customer_ids"], json!([]));
    // cookie toko A dikirim ke host toko B → tidak dikenali
    assert_eq!(
        shop(
            &a,
            Method::GET,
            HOST_B,
            "/api/v1/public/customer/me",
            Some(&s),
            None,
            None
        )
        .await
        .status,
        401
    );
    // tidak ada data CRM internal di endpoint customer
    let raw = String::from_utf8(me.bytes.clone()).unwrap();
    assert!(
        !raw.contains("internal_note") && !raw.contains("tags"),
        "{raw}"
    );

    // registrasi wajib persetujuan privacy/terms (D-12)
    let r = shop(&a, Method::POST, HOST_A, "/api/v1/public/customer/register", None, None,
        Some(json!({"email": "b@x.id", "password": "rahasia-panjang-1", "accept_privacy": true, "accept_terms": false}))).await;
    assert_eq!(r.status, 422);
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn linking_only_after_verification_and_only_same_shop(pool: PgPool) {
    // AC-CRM-005
    let a = app(&pool).await;
    let (ta, tb) = (tenant(&pool, "toko-a").await, tenant(&pool, "toko-b").await);
    let oa = member(&pool, ta, "Owner").await;
    let ga = guest(&a, ta, "Sari", "sari@x.id").await;
    let gb = guest(&a, tb, "Sari", "sari@x.id").await;
    let gone = guest(&a, ta, "Sari lama", "sari@x.id").await;
    staff(
        &a,
        Method::POST,
        format!("{}/{gone}/deletion-request", customers(ta)),
        &oa,
        None,
    )
    .await;
    let other = guest(&a, ta, "Budi", "budi@x.id").await;

    register(&a, HOST_A, "sari@x.id").await;
    let token = deliver_verification(&a).await;
    // token toko A dipakai di host toko B → ditolak (tenant context)
    let verify = |host: &'static str, tok: String| {
        let a = &a;
        async move {
            shop(
                a,
                Method::POST,
                host,
                "/api/v1/public/customer/verify-email",
                None,
                None,
                Some(json!({ "token": tok })),
            )
            .await
        }
    };
    assert_eq!(verify(HOST_B, token.clone()).await.status, 400);
    let r = verify(HOST_A, token.clone()).await;
    assert_eq!(r.status, 200, "{}", r.json());
    assert_eq!(r.json()["data"]["linked_customers"], 1);
    assert_eq!(
        verify(HOST_A, token).await.status,
        400,
        "token sekali pakai"
    );

    let s = login(&a, HOST_A, "sari@x.id").await.unwrap();
    let me = shop(
        &a,
        Method::GET,
        HOST_A,
        "/api/v1/public/customer/me",
        Some(&s),
        None,
        None,
    )
    .await;
    assert_eq!(me.json()["data"]["email_verified"], true);
    assert_eq!(me.json()["data"]["customer_ids"], json!([ga.to_string()]));
    let linked = |id: Uuid| {
        let o = a.owner.clone();
        async move {
            sqlx::query_scalar::<_, bool>(
                "SELECT account_id IS NOT NULL FROM customers WHERE id = $1",
            )
            .bind(id)
            .fetch_one(&o)
            .await
            .unwrap()
        }
    };
    assert!(!linked(gb).await, "customer toko lain tidak ikut");
    assert!(
        !linked(gone).await,
        "customer dalam proses hapus tidak ditautkan"
    );
    assert!(!linked(other).await);
    let audit: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM audit_logs WHERE tenant_id = $1 AND action = 'customer.linked'",
    )
    .bind(ta)
    .fetch_one(&a.owner)
    .await
    .unwrap();
    assert_eq!(audit, 1);

    // guest baru setelah terverifikasi → tertaut saat login berikutnya
    let later = guest(&a, ta, "Sari", "sari@x.id").await;
    login(&a, HOST_A, "sari@x.id").await.unwrap();
    assert!(linked(later).await);
    let staff_view = staff(
        &a,
        Method::GET,
        format!("{}/{ga}", customers(ta)),
        &oa,
        None,
    )
    .await;
    assert_eq!(staff_view.json()["data"]["has_account"], true);
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn customer_session_requires_same_shop_origin_and_csrf(pool: PgPool) {
    let a = app(&pool).await;
    tenant(&pool, "toko-a").await;
    register(&a, HOST_A, "sari@x.id").await;
    let r = shop(
        &a,
        Method::POST,
        HOST_A,
        "/api/v1/public/customer/login",
        None,
        Some("https://toko-b.ryasai.my.id"),
        Some(json!({"email": "sari@x.id", "password": "rahasia-panjang-1"})),
    )
    .await;
    assert_eq!(r.status, 403, "Origin toko lain");
    let r = shop(&a, Method::POST, HOST_A, "/api/v1/public/customer/register", None, Some("https://app.ryasai.my.id"),
        Some(json!({"email": "c@x.id", "password": "rahasia-panjang-1", "accept_privacy": true, "accept_terms": true}))).await;
    assert_eq!(r.status, 403, "Origin dashboard");
    let s = login(&a, HOST_A, "sari@x.id").await.unwrap();
    let bad = (s.0.clone(), "salah".to_owned());
    let origin = Some("https://toko-a.ryasai.my.id");
    assert_eq!(
        shop(
            &a,
            Method::POST,
            HOST_A,
            "/api/v1/public/customer/logout",
            Some(&bad),
            origin,
            None
        )
        .await
        .status,
        403
    );
    assert_eq!(
        shop(
            &a,
            Method::POST,
            HOST_A,
            "/api/v1/public/customer/logout",
            Some(&s),
            Some("https://evil.example"),
            None
        )
        .await
        .status,
        403
    );
    assert_eq!(
        shop(
            &a,
            Method::POST,
            HOST_A,
            "/api/v1/public/customer/logout",
            Some(&s),
            origin,
            None
        )
        .await
        .status,
        204
    );
    assert_eq!(
        shop(
            &a,
            Method::GET,
            HOST_A,
            "/api/v1/public/customer/me",
            Some(&s),
            None,
            None
        )
        .await
        .status,
        401,
        "sesi dicabut"
    );
    let r = shop(
        &a,
        Method::POST,
        HOST_A,
        "/api/v1/public/customer/login",
        None,
        None,
        Some(json!({"email": "sari@x.id", "password": "salah-sekali-123"})),
    )
    .await;
    let r2 = shop(
        &a,
        Method::POST,
        HOST_A,
        "/api/v1/public/customer/login",
        None,
        None,
        Some(json!({"email": "tidakada@x.id", "password": "salah-sekali-123"})),
    )
    .await;
    assert_eq!((r.status, r2.status), (401, 401));
    assert_eq!(
        r.json()["error"]["message"],
        r2.json()["error"]["message"],
        "tidak membocorkan akun ada/tidak"
    );
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn deletion_request_anonymizes_after_grace_and_export_is_audited(pool: PgPool) {
    // AC-CRM-006
    let a = app(&pool).await;
    let t = tenant(&pool, "toko-a").await;
    let o = member(&pool, t, "Owner").await;
    let viewer = member(&pool, t, "Viewer").await;
    let id = guest(&a, t, "Sari", "sari@x.id").await;
    let recent = guest(&a, t, "Budi", "budi@x.id").await;
    staff(
        &a,
        Method::PUT,
        format!("{}/{id}/addresses", customers(t)),
        &o,
        Some(json!([{"address": "Jl. Mawar 1"}])),
    )
    .await;
    register(&a, HOST_A, "sari@x.id").await;
    let token = deliver_verification(&a).await;
    shop(
        &a,
        Method::POST,
        HOST_A,
        "/api/v1/public/customer/verify-email",
        None,
        None,
        Some(json!({ "token": token })),
    )
    .await;
    let s = login(&a, HOST_A, "sari@x.id").await.unwrap();

    let r = staff(
        &a,
        Method::GET,
        format!("{}/{id}/export", customers(t)),
        &o,
        None,
    )
    .await;
    assert_eq!(r.json()["data"]["addresses"][0]["address"], "Jl. Mawar 1");
    assert_eq!(
        staff(
            &a,
            Method::GET,
            format!("{}/{id}/export", customers(t)),
            &viewer,
            None
        )
        .await
        .status,
        403
    );

    assert_eq!(
        staff(
            &a,
            Method::POST,
            format!("{}/{id}/deletion-request", customers(t)),
            &o,
            None
        )
        .await
        .status,
        202
    );
    assert_eq!(
        staff(
            &a,
            Method::POST,
            format!("{}/{recent}/deletion-request", customers(t)),
            &o,
            None
        )
        .await
        .status,
        202
    );
    assert_eq!(
        shop(
            &a,
            Method::GET,
            HOST_A,
            "/api/v1/public/customer/me",
            Some(&s),
            None,
            None
        )
        .await
        .status,
        401,
        "sesi akun dicabut"
    );
    let r = staff(
        &a,
        Method::PUT,
        format!("{}/{id}", customers(t)),
        &o,
        Some(json!({"name": "Sari", "email": "s@x.id"})),
    )
    .await;
    assert_eq!(r.json()["error"]["details"]["reason"], "customer_deleted");

    // grace belum lewat: tidak ada perubahan
    let app_db = app_pool(&pool).await;
    let policy = RetentionPolicy {
        audit: Duration::from_secs(365 * 86_400),
        closed_tenant_grace: Duration::from_secs(30 * 86_400),
        auth_artifacts: Duration::from_secs(30 * 86_400),
        outbox: Duration::from_secs(30 * 86_400),
    };
    assert_eq!(run_purge(&app_db, &policy).await.unwrap().customers, 0);
    sqlx::query(
        "UPDATE customers SET deletion_requested_at = now() - interval '31 days' WHERE id = $1",
    )
    .bind(id)
    .execute(&a.owner)
    .await
    .unwrap();
    assert_eq!(run_purge(&app_db, &policy).await.unwrap().customers, 1);
    assert_eq!(
        run_purge(&app_db, &policy).await.unwrap().customers,
        0,
        "idempoten"
    );
    let row: (String, Option<String>, Option<Uuid>, i64, i64) = sqlx::query_as(
        "SELECT name, email, account_id,
                (SELECT count(*) FROM customer_addresses WHERE customer_id = $1),
                (SELECT count(*) FROM customer_accounts WHERE tenant_id = $2)
         FROM customers WHERE id = $1",
    )
    .bind(id)
    .bind(t)
    .fetch_one(&a.owner)
    .await
    .unwrap();
    assert_eq!(row, ("Pelanggan dihapus".into(), None, None, 0, 0));
    assert_eq!(
        staff(&a, Method::GET, format!("{}/{id}", customers(t)), &o, None)
            .await
            .status,
        404
    );
    // grace < 30 hari ditolak fungsi
    let e = crm_repo::purge(&app_db, 86_400).await.unwrap_err();
    assert_eq!(
        e.as_database_error().unwrap().code().as_deref(),
        Some("22023")
    );
    let actions: Vec<String> = sqlx::query_scalar(
        "SELECT action FROM audit_logs WHERE tenant_id = $1 AND resource_id = $2 ORDER BY created_at, id",
    )
    .bind(t)
    .bind(id.to_string())
    .fetch_all(&a.owner)
    .await
    .unwrap();
    assert!(
        actions.contains(&"customer.exported".to_owned())
            && actions.contains(&"customer.deletion_requested".to_owned()),
        "{actions:?}"
    );
    let _ = OutboxEvent::customer_email_verification_requested(t, id);
}
