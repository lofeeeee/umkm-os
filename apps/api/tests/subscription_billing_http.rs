//! P7-3: langganan + pembayaran manual (ADR-0016). Toko membuat tagihan & mengunggah bukti; super admin (sesi MFA)
//! memverifikasi lewat area platform; keputusan atomik + audit platform + notifikasi toko.

mod common;

use axum::http::Method;
use common::*;
use serde_json::{Value, json};
use sqlx::PgPool;
use umkm_database::{MIGRATOR, begin_tenant_tx};
use umkm_shared::ids::TenantId;
use uuid::Uuid;

fn base(t: Uuid) -> String {
    format!("/api/v1/tenants/{t}/subscription")
}

async fn post(a: &TestApp, w: &Who, uri: &str, body: Value) -> Res {
    json(&a.router, Method::POST, uri, Some(w), Some(body)).await
}

async fn get(a: &TestApp, w: &Who, uri: &str) -> Res {
    json(&a.router, Method::GET, uri, Some(w), None).await
}

async fn proof(a: &TestApp, w: &Who, t: Uuid, inv: &str, bytes: Vec<u8>) -> Res {
    send(
        &a.router,
        Method::POST,
        &format!("{}/invoices/{inv}/proof", base(t)),
        Some(w),
        None,
        Some("application/octet-stream"),
        bytes,
    )
    .await
}

fn reason(r: &Res) -> Value {
    r.json()["error"]["details"]["reason"].clone()
}

async fn days_left(pool: &PgPool, t: Uuid) -> f64 {
    sqlx::query_scalar("SELECT (extract(epoch FROM current_period_end - now()) / 86400)::float8 FROM subscriptions WHERE tenant_id = $1")
        .bind(t)
        .fetch_one(pool)
        .await
        .unwrap()
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn manual_payment_activates_and_renews_pro(pool: PgPool) {
    let a = app(&pool).await;
    let t = tenant(&pool, "toko-a").await;
    let owner = member(&pool, t, "Owner").await;
    let manager = member(&pool, t, "Manager").await;
    let admin = platform_admin(&pool, true).await;
    let admin_no_mfa = platform_admin(&pool, false).await;

    // Toko: tagihan Pro. Nominal dari katalog (klien tidak bisa menentukan), hanya tenant.update.
    assert_eq!(
        post(
            &a,
            &manager,
            &format!("{}/invoices", base(t)),
            json!({"plan_code": "pro"})
        )
        .await
        .status,
        403
    );
    assert_eq!(
        post(
            &a,
            &owner,
            &format!("{}/invoices", base(t)),
            json!({"plan_code": "pro", "amount_idr": 1})
        )
        .await
        .status,
        422
    );
    assert_eq!(
        post(
            &a,
            &owner,
            &format!("{}/invoices", base(t)),
            json!({"plan_code": "gratis"})
        )
        .await
        .status,
        422
    );
    let r = post(
        &a,
        &owner,
        &format!("{}/invoices", base(t)),
        json!({"plan_code": "pro"}),
    )
    .await;
    assert_eq!(r.status, 201, "{}", String::from_utf8_lossy(&r.bytes));
    let inv = r.json()["data"].clone();
    assert_eq!(
        (
            inv["amount_idr"].clone(),
            inv["period_days"].clone(),
            inv["status"].clone()
        ),
        (json!(100000), json!(30), json!("awaiting_payment"))
    );
    assert!(inv["code"].as_str().unwrap().starts_with("INV-"));
    let iid = inv["id"].as_str().unwrap().to_owned();
    let list = get(&a, &owner, &format!("{}/invoices", base(t)))
        .await
        .json()["data"]
        .clone();
    assert!(list["payment_instructions"].is_string());

    // Bukti: bukan gambar 422; PNG → submitted; toko melihat bukti sendiri.
    assert_eq!(
        proof(&a, &owner, t, &iid, b"bukan gambar".to_vec())
            .await
            .status,
        422
    );
    let r = proof(&a, &owner, t, &iid, png()).await;
    assert_eq!(
        (r.status, r.json()["data"]["status"].clone()),
        (200, json!("submitted"))
    );
    let img = get(&a, &owner, &format!("{}/invoices/{iid}/proof", base(t))).await;
    assert_eq!(
        (img.status, img.headers["content-type"].to_str().unwrap()),
        (200, "image/png")
    );
    assert_eq!(img.headers["x-content-type-options"], "nosniff");
    // Selama menunggu verifikasi tidak bisa membuat tagihan baru.
    let r = post(
        &a,
        &owner,
        &format!("{}/invoices", base(t)),
        json!({"plan_code": "starter"}),
    )
    .await;
    assert_eq!((r.status, reason(&r)), (409, json!("invoice_pending")));

    // Platform: hanya super admin dengan sesi MFA.
    // Pengguna biasa — bahkan dengan sesi lolos MFA — bukan admin platform.
    sqlx::query("UPDATE user_sessions SET mfa_verified_at = now() WHERE user_id = $1")
        .bind(owner.user)
        .execute(&pool)
        .await
        .unwrap();
    let r = get(&a, &owner, "/api/v1/platform/invoices").await;
    assert_eq!(r.status, 403);
    assert_ne!(
        reason(&r),
        json!("mfa_required"),
        "ditolak karena peran, bukan karena MFA"
    );
    assert_eq!(get(&a, &owner, "/api/v1/platform/me").await.status, 403);
    let r = get(&a, &admin_no_mfa, "/api/v1/platform/invoices").await;
    assert_eq!((r.status, reason(&r)), (403, json!("mfa_required")));
    assert_eq!(get(&a, &admin, "/api/v1/platform/me").await.status, 200);
    let q = get(&a, &admin, "/api/v1/platform/invoices").await.json()["data"].clone();
    assert_eq!(q.as_array().unwrap().len(), 1);
    assert_eq!(
        (q[0]["tenant_slug"].clone(), q[0]["status"].clone()),
        (json!("toko-a"), json!("submitted"))
    );
    assert_eq!(
        get(
            &a,
            &admin,
            &format!("/api/v1/platform/invoices/{iid}/proof")
        )
        .await
        .status,
        200
    );
    assert_eq!(
        post(
            &a,
            &owner,
            &format!("/api/v1/platform/invoices/{iid}/approve"),
            json!({})
        )
        .await
        .status,
        403
    );

    // Setujui → Pro aktif 30 hari, sekali saja.
    let r = post(
        &a,
        &admin,
        &format!("/api/v1/platform/invoices/{iid}/approve"),
        json!({}),
    )
    .await;
    assert_eq!(
        (r.status, r.json()["data"]["status"].clone()),
        (200, json!("paid")),
        "{}",
        String::from_utf8_lossy(&r.bytes)
    );
    assert_eq!(
        post(
            &a,
            &admin,
            &format!("/api/v1/platform/invoices/{iid}/approve"),
            json!({})
        )
        .await
        .status,
        409
    );
    let s = get(&a, &owner, &base(t)).await.json()["data"].clone();
    assert_eq!(
        (s["plan"]["code"].clone(), s["status"].clone()),
        (json!("pro"), json!("active"))
    );
    let d = days_left(&pool, t).await;
    assert!((29.99..=30.0).contains(&d), "{d}");

    // Perpanjangan paket sama → dari akhir periode berjalan (≈ 60 hari).
    let iid2 = post(
        &a,
        &owner,
        &format!("{}/invoices", base(t)),
        json!({"plan_code": "pro"}),
    )
    .await
    .json()["data"]["id"]
        .as_str()
        .unwrap()
        .to_owned();
    proof(&a, &owner, t, &iid2, png()).await;
    post(
        &a,
        &admin,
        &format!("/api/v1/platform/invoices/{iid2}/approve"),
        json!({}),
    )
    .await;
    let d = days_left(&pool, t).await;
    assert!((59.99..=60.0).contains(&d), "{d}");

    // Audit platform (aktor platform, tenant tercatat) + notifikasi status langganan ke toko.
    let audits: Vec<(String, String, Option<Uuid>)> = sqlx::query_as(
        "SELECT action, actor_type, tenant_id FROM audit_logs WHERE action LIKE 'platform.%' ORDER BY created_at",
    )
    .fetch_all(&pool)
    .await
    .unwrap();
    assert!(audits.contains(&("platform.proof_viewed".into(), "platform".into(), None)));
    assert_eq!(
        audits
            .iter()
            .filter(|x| x.0 == "platform.invoice_approved" && x.1 == "platform" && x.2 == Some(t))
            .count(),
        2
    );
    let n: i64 = sqlx::query_scalar("SELECT count(*) FROM notification_outbox WHERE tenant_id = $1 AND event_type = 'subscription.status'")
        .bind(t)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(n, 2);
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn reject_cancel_and_no_self_approval(pool: PgPool) {
    let a = app(&pool).await;
    let t = tenant(&pool, "toko-a").await;
    let tb = tenant(&pool, "toko-b").await;
    let owner = member(&pool, t, "Owner").await;
    let owner_b = member(&pool, tb, "Owner").await;
    let admin = platform_admin(&pool, true).await;
    let iid = post(
        &a,
        &owner,
        &format!("{}/invoices", base(t)),
        json!({"plan_code": "starter"}),
    )
    .await
    .json()["data"]["id"]
        .as_str()
        .unwrap()
        .to_owned();
    let inv = Uuid::parse_str(&iid).unwrap();
    proof(&a, &owner, t, &iid, png()).await;

    // Toko tidak dapat menandai lunas / mengubah nominal sendiri, meski langsung ke DB sebagai role aplikasi.
    let app_db = app_pool(&pool).await;
    for sql in [
        "UPDATE subscription_invoices SET status = 'paid', decided_at = now(), period_start = now(), period_end = now() + interval '1 year'",
        "UPDATE subscription_invoices SET amount_idr = 1",
        "INSERT INTO subscription_invoices (tenant_id, code, plan_code, amount_idr, period_days, status) VALUES (app.current_tenant_id(), 'INV-AAAAAAAAAA', 'pro', 1, 365, 'paid')",
    ] {
        let mut tx = begin_tenant_tx(&app_db, TenantId::from_uuid(t))
            .await
            .unwrap();
        let e = sqlx::query(sql).execute(&mut *tx).await.unwrap_err();
        assert_eq!(
            e.as_database_error().and_then(|d| d.code()).as_deref(),
            Some("42501"),
            "{sql}: {e}"
        );
        tx.rollback().await.unwrap();
    }
    // Toko lain tidak melihat tagihan/bukti.
    assert_eq!(
        get(&a, &owner_b, &format!("{}/invoices/{iid}/proof", base(tb)))
            .await
            .status,
        404
    );
    assert!(
        get(&a, &owner_b, &format!("{}/invoices", base(tb)))
            .await
            .json()["data"]["items"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    assert_eq!(proof(&a, &owner_b, tb, &iid, png()).await.status, 404);

    // Tolak: alasan wajib; alasan terlihat toko; langganan tetap uji coba.
    assert_eq!(
        post(
            &a,
            &admin,
            &format!("/api/v1/platform/invoices/{iid}/reject"),
            json!({"reason": "  "})
        )
        .await
        .status,
        422
    );
    let r = post(
        &a,
        &admin,
        &format!("/api/v1/platform/invoices/{iid}/reject"),
        json!({"reason": "Nominal tidak sesuai"}),
    )
    .await;
    assert_eq!(r.json()["data"]["status"], "rejected");
    let it = get(&a, &owner, &format!("{}/invoices", base(t)))
        .await
        .json()["data"]["items"][0]
        .clone();
    assert_eq!(
        (it["status"].clone(), it["rejection_reason"].clone()),
        (json!("rejected"), json!("Nominal tidak sesuai"))
    );
    assert_eq!(
        get(&a, &owner, &base(t)).await.json()["data"]["status"],
        "trialing"
    );
    // Bukti tidak bisa diganti setelah diputus.
    assert_eq!(proof(&a, &owner, t, &iid, png()).await.status, 409);
    assert_eq!(
        post(
            &a,
            &admin,
            &format!("/api/v1/platform/invoices/{inv}/approve"),
            json!({})
        )
        .await
        .status,
        409
    );

    // Tagihan baru menggantikan tagihan belum dibayar; batal → 204, ulang → 409.
    let i1 = post(
        &a,
        &owner,
        &format!("{}/invoices", base(t)),
        json!({"plan_code": "starter"}),
    )
    .await
    .json()["data"]["id"]
        .as_str()
        .unwrap()
        .to_owned();
    let i2 = post(
        &a,
        &owner,
        &format!("{}/invoices", base(t)),
        json!({"plan_code": "pro"}),
    )
    .await
    .json()["data"]["id"]
        .as_str()
        .unwrap()
        .to_owned();
    let items = get(&a, &owner, &format!("{}/invoices", base(t)))
        .await
        .json()["data"]["items"]
        .clone();
    let status_of = |id: &str| {
        items
            .as_array()
            .unwrap()
            .iter()
            .find(|x| x["id"] == id)
            .unwrap()["status"]
            .clone()
    };
    assert_eq!(
        (status_of(&i1), status_of(&i2)),
        (json!("cancelled"), json!("awaiting_payment"))
    );
    assert_eq!(
        json(
            &a.router,
            Method::POST,
            &format!("{}/invoices/{i2}/cancel", base(t)),
            Some(&owner),
            None
        )
        .await
        .status,
        204
    );
    assert_eq!(
        json(
            &a.router,
            Method::POST,
            &format!("{}/invoices/{i2}/cancel", base(t)),
            Some(&owner),
            None
        )
        .await
        .status,
        409
    );
    // Hanya tagihan 'submitted' yang bisa disetujui.
    assert_eq!(
        post(
            &a,
            &admin,
            &format!("/api/v1/platform/invoices/{i2}/approve"),
            json!({})
        )
        .await
        .status,
        409
    );
    assert_eq!(
        post(
            &a,
            &admin,
            &format!("/api/v1/platform/invoices/{}/approve", Uuid::new_v4()),
            json!({})
        )
        .await
        .status,
        404
    );
}
