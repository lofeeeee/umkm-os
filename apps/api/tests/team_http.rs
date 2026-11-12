//! P4-1: tim, role kustom, undangan (PRD.md §3, AUTH-009/010). Anti-eskalasi, Owner terakhir,
//! isolasi tenant, audit.

mod common;

use std::time::Duration;

use axum::http::{Method, header};
use common::*;
use serde_json::{Value, json};
use sqlx::PgPool;
use umkm_database::MIGRATOR;
use umkm_domain::auth::AuthPolicy;
use umkm_infrastructure::{
    metrics::Metrics,
    outbox::{CapturingEmailSender, WorkerConfig, process_once},
};
use uuid::Uuid;

fn base(t: Uuid) -> String {
    format!("/api/v1/tenants/{t}")
}

async fn call(a: &TestApp, m: Method, url: String, who: &Who, body: Option<Value>) -> Res {
    json(&a.router, m, &url, Some(who), body).await
}

async fn role_id(a: &TestApp, t: Uuid, who: &Who, name: &str) -> String {
    let r = call(a, Method::GET, format!("{}/roles", base(t)), who, None).await;
    r.json()["data"]
        .as_array()
        .unwrap()
        .iter()
        .find(|x| x["name"] == name)
        .unwrap()["id"]
        .as_str()
        .unwrap()
        .to_owned()
}

async fn membership(a: &TestApp, t: Uuid, who: &Who, user: Uuid) -> String {
    let r = call(a, Method::GET, format!("{}/members", base(t)), who, None).await;
    r.json()["data"]
        .as_array()
        .unwrap()
        .iter()
        .find(|x| x["user_id"] == user.to_string())
        .unwrap()["membership_id"]
        .as_str()
        .unwrap()
        .to_owned()
}

async fn custom_role(a: &TestApp, t: Uuid, owner: &Who, name: &str, perms: &[&str]) -> String {
    let r = call(
        a,
        Method::POST,
        format!("{}/roles", base(t)),
        owner,
        Some(json!({"name": name, "permissions": perms})),
    )
    .await;
    assert_eq!(r.status, 201, "{}", r.json());
    role_id(a, t, owner, name).await
}

async fn patch_member(a: &TestApp, t: Uuid, who: &Who, m: &str, body: Value) -> Res {
    call(
        a,
        Method::PATCH,
        format!("{}/members/{m}", base(t)),
        who,
        Some(body),
    )
    .await
}

fn reason(r: &Res) -> Value {
    r.json()["error"]["details"]["reason"].clone()
}

/// Jalankan worker outbox; token dari email undangan terakhir.
async fn deliver(a: &TestApp) -> String {
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
        .find(|m| m.template == "team_invitation")
        .expect("email undangan");
    assert!(
        m.text_body
            .contains("https://app.ryasai.my.id/invite#token="),
        "{}",
        m.text_body
    );
    let i = m.text_body.find("#token=").unwrap() + 7;
    m.text_body[i..]
        .split_whitespace()
        .next()
        .unwrap()
        .to_owned()
}

async fn accept(a: &TestApp, token: &str, who: Option<&Who>, password: Option<&str>) -> Res {
    let body = json!({ "token": token, "password": password });
    send(
        &a.router,
        Method::POST,
        "/api/v1/invitations/accept",
        who,
        None,
        Some("application/json"),
        body.to_string().into_bytes(),
    )
    .await
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn permissions_cannot_be_escalated(pool: PgPool) {
    let a = app(&pool).await;
    let t = tenant(&pool, "toko-a").await;
    let o = member(&pool, t, "Owner").await;
    let viewer = member(&pool, t, "Viewer").await;
    let mgr = member(&pool, t, "Manager").await;
    let x = member(&pool, t, "Viewer").await;
    let lead = custom_role(
        &a,
        t,
        &o,
        "Ketua tim",
        &[
            "members.read",
            "members.manage",
            "orders.read",
            "tenant.read",
        ],
    )
    .await;
    let kasir = custom_role(&a, t, &o, "Kasir", &["orders.read", "tenant.read"]).await;
    let xm = membership(&a, t, &o, x.user).await;
    assert_eq!(
        patch_member(&a, t, &o, &xm, json!({"role_id": lead}))
            .await
            .status,
        200
    );

    // Viewer/Manager tanpa members.manage
    let vm = membership(&a, t, &o, viewer.user).await;
    assert_eq!(
        patch_member(&a, t, &viewer, &vm, json!({"status": "suspended"}))
            .await
            .status,
        403
    );
    assert_eq!(
        patch_member(&a, t, &mgr, &vm, json!({"status": "suspended"}))
            .await
            .status,
        403
    );

    // X (Ketua tim) tidak dapat memberi/menyentuh hak di luar miliknya
    let r = call(
        &a,
        Method::POST,
        format!("{}/roles", base(t)),
        &x,
        Some(json!({"name": "Auditor", "permissions": ["audit.read"]})),
    )
    .await;
    assert_eq!(
        (r.status, reason(&r)),
        (403, json!("permission_escalation"))
    );
    let om = membership(&a, t, &o, o.user).await;
    let r = patch_member(&a, t, &x, &om, json!({"status": "suspended"})).await;
    assert_eq!(
        reason(&r),
        json!("permission_escalation"),
        "tidak boleh menyentuh Owner"
    );
    let mm = membership(&a, t, &o, mgr.user).await;
    let owner_role = role_id(&a, t, &o, "Owner").await;
    assert_eq!(
        reason(&patch_member(&a, t, &x, &vm, json!({"role_id": owner_role})).await),
        json!("permission_escalation")
    );
    assert_eq!(
        reason(&patch_member(&a, t, &x, &mm, json!({"role_id": kasir})).await),
        json!("permission_escalation"),
        "Manager lebih kuat dari X"
    );
    let r = call(&a, Method::PUT, format!("{}/roles/{lead}", base(t)), &x, Some(json!({"name": "Ketua tim", "permissions": ["members.manage", "members.read", "orders.read", "tenant.read", "orders.approve"]}))).await;
    assert_eq!(
        reason(&r),
        json!("permission_escalation"),
        "menaikkan hak role sendiri"
    );
    // ...tetapi boleh dalam batas haknya: Viewer → Kasir? Viewer punya hak di luar X → ditolak; Kasir → Kasir boleh
    let k = member(&pool, t, "Viewer").await;
    let km = membership(&a, t, &o, k.user).await;
    patch_member(&a, t, &o, &km, json!({"role_id": kasir})).await;
    assert_eq!(
        patch_member(&a, t, &x, &km, json!({"status": "suspended"}))
            .await
            .status,
        200
    );
    // diri sendiri tidak bisa diubah
    assert_eq!(
        reason(&patch_member(&a, t, &x, &xm, json!({"status": "active"})).await),
        json!("self_change")
    );
    // anggota nonaktif kehilangan akses
    assert_eq!(
        call(&a, Method::GET, format!("{}/orders", base(t)), &k, None)
            .await
            .status,
        404
    );

    // audit before/after
    let s: Vec<Value> = sqlx::query_scalar("SELECT summary FROM audit_logs WHERE tenant_id = $1 AND action = 'member.updated' ORDER BY created_at, id")
        .bind(t)
        .fetch_all(&a.owner)
        .await
        .unwrap();
    assert_eq!(s.last().unwrap()["before"]["status"], "active");
    assert_eq!(s.last().unwrap()["after"]["status"], "suspended");
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn last_active_owner_is_protected(pool: PgPool) {
    let a = app(&pool).await;
    let t = tenant(&pool, "toko-a").await;
    let o = member(&pool, t, "Owner").await;
    let o2 = member(&pool, t, "Owner").await;
    let all: Vec<String> = call(&a, Method::GET, "/api/v1/permissions".into(), &o, None)
        .await
        .json()["data"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap().to_owned())
        .collect();
    assert_eq!(all.len(), 37);
    let admin = custom_role(
        &a,
        t,
        &o,
        "Wakil",
        &all.iter().map(String::as_str).collect::<Vec<_>>(),
    )
    .await;
    let x = member(&pool, t, "Viewer").await;
    let xm = membership(&a, t, &o, x.user).await;
    patch_member(&a, t, &o, &xm, json!({"role_id": admin})).await;
    let m2 = membership(&a, t, &o, o2.user).await;
    assert_eq!(
        patch_member(&a, t, &o, &m2, json!({"status": "suspended"}))
            .await
            .status,
        200,
        "masih ada Owner lain"
    );
    let m1 = membership(&a, t, &o, o.user).await;
    let r = patch_member(&a, t, &x, &m1, json!({"status": "removed"})).await;
    assert_eq!(reason(&r), json!("last_owner"));
    let r = patch_member(&a, t, &x, &m1, json!({"role_id": admin})).await;
    assert_eq!(reason(&r), json!("last_owner"), "menurunkan Owner terakhir");
    // Paralel: dua Owner saling menonaktifkan → paling banyak satu berhasil
    patch_member(&a, t, &o, &m2, json!({"status": "active"})).await;
    let (r1, r2) = tokio::join!(
        patch_member(&a, t, &o, &m2, json!({"status": "suspended"})),
        patch_member(&a, t, &o2, &m1, json!({"status": "suspended"})),
    );
    assert!(
        [r1.status, r2.status].iter().filter(|s| **s == 200).count() <= 1,
        "{} {}",
        r1.status,
        r2.status
    );
    let owners: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM tenant_memberships m JOIN roles r ON r.id = m.role_id WHERE m.tenant_id = $1 AND m.status = 'active' AND r.name = 'Owner'",
    )
    .bind(t)
    .fetch_one(&a.owner)
    .await
    .unwrap();
    assert!(owners >= 1);
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn custom_roles_are_tenant_owned(pool: PgPool) {
    let a = app(&pool).await;
    let (ta, tb) = (tenant(&pool, "toko-a").await, tenant(&pool, "toko-b").await);
    let (oa, ob) = (
        member(&pool, ta, "Owner").await,
        member(&pool, tb, "Owner").await,
    );
    let kasir = custom_role(&a, ta, &oa, "Kasir", &["orders.read"]).await;
    let r = call(
        &a,
        Method::POST,
        format!("{}/roles", base(ta)),
        &oa,
        Some(json!({"name": "kasir", "permissions": []})),
    )
    .await;
    assert_eq!(reason(&r), json!("role_name_taken"));
    let r = call(
        &a,
        Method::POST,
        format!("{}/roles", base(ta)),
        &oa,
        Some(json!({"name": "X", "permissions": ["root.all"]})),
    )
    .await;
    assert_eq!(r.status, 422);
    let viewer_role = role_id(&a, ta, &oa, "Viewer").await;
    let r = call(
        &a,
        Method::PUT,
        format!("{}/roles/{viewer_role}", base(ta)),
        &oa,
        Some(json!({"name": "Viewer", "permissions": []})),
    )
    .await;
    assert_eq!(reason(&r), json!("system_role"));
    // role tenant A tidak terlihat/terpakai di tenant B
    let vb = member(&pool, tb, "Viewer").await;
    let vbm = membership(&a, tb, &ob, vb.user).await;
    assert_eq!(
        patch_member(&a, tb, &ob, &vbm, json!({"role_id": kasir}))
            .await
            .status,
        404
    );
    assert_eq!(
        call(
            &a,
            Method::DELETE,
            format!("{}/roles/{kasir}", base(tb)),
            &ob,
            None
        )
        .await
        .status,
        404
    );
    assert!(
        !call(&a, Method::GET, format!("{}/roles", base(tb)), &ob, None)
            .await
            .json()["data"]
            .to_string()
            .contains("Kasir")
    );
    // dipakai → tidak dapat dihapus; dipakai anggota → izin berlaku
    let v = member(&pool, ta, "Viewer").await;
    let vm = membership(&a, ta, &oa, v.user).await;
    patch_member(&a, ta, &oa, &vm, json!({"role_id": kasir})).await;
    assert_eq!(
        reason(
            &call(
                &a,
                Method::DELETE,
                format!("{}/roles/{kasir}", base(ta)),
                &oa,
                None
            )
            .await
        ),
        json!("role_in_use")
    );
    assert_eq!(
        call(&a, Method::GET, format!("{}/orders", base(ta)), &v, None)
            .await
            .status,
        200
    );
    assert_eq!(
        call(&a, Method::GET, format!("{}/products", base(ta)), &v, None)
            .await
            .status,
        403,
        "Kasir tanpa products.read"
    );
    let audit: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM audit_logs WHERE tenant_id = $1 AND action = 'role.changed'",
    )
    .bind(ta)
    .fetch_one(&a.owner)
    .await
    .unwrap();
    assert_eq!(audit, 1);
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn invitations_are_single_use_and_bound_to_email(pool: PgPool) {
    // AUTH-009/010 (email lewat adapter uji; email nyata menunggu D-08)
    let a = app(&pool).await;
    let t = tenant(&pool, "toko-a").await;
    let o = member(&pool, t, "Owner").await;
    let support = role_id(&a, t, &o, "Support Agent").await;
    let r = call(
        &a,
        Method::POST,
        format!("{}/invitations", base(t)),
        &o,
        Some(json!({"email": "Budi@X.id", "role_id": support})),
    )
    .await;
    assert_eq!(r.status, 201, "{}", r.json());
    assert_eq!(r.json()["data"][0]["email"], "budi@x.id");
    let token = deliver(&a).await;
    let audit: Value =
        sqlx::query_scalar("SELECT summary FROM audit_logs WHERE action = 'member.invited'")
            .fetch_one(&a.owner)
            .await
            .unwrap();
    assert!(
        !audit.to_string().contains("budi"),
        "audit tanpa email mentah: {audit}"
    );

    assert_eq!(accept(&a, &token, None, Some("pendek")).await.status, 422);
    assert_eq!(
        accept(&a, &token, None, Some("budi@x.id")).await.status,
        422,
        "password = email ditolak"
    );
    let r = accept(&a, &token, None, Some("rahasia-budi-123")).await;
    assert_eq!(r.status, 200, "{}", r.json());
    assert!(
        r.headers[header::SET_COOKIE]
            .to_str()
            .unwrap()
            .starts_with("__Host-umkm_session=")
    );
    assert_eq!(r.json()["data"]["tenant_id"], t.to_string());
    assert_eq!(
        accept(&a, &token, None, Some("rahasia-budi-123"))
            .await
            .status,
        404,
        "sekali pakai"
    );
    let m = call(&a, Method::GET, format!("{}/members", base(t)), &o, None).await;
    let budi = m.json()["data"]
        .as_array()
        .unwrap()
        .iter()
        .find(|x| x["email"] == "budi@x.id")
        .cloned()
        .unwrap();
    assert_eq!(budi["role_name"], "Support Agent");
    let verified: bool = sqlx::query_scalar(
        "SELECT email_verified_at IS NOT NULL FROM users WHERE email = 'budi@x.id'",
    )
    .fetch_one(&a.owner)
    .await
    .unwrap();
    assert!(verified, "token membuktikan kepemilikan inbox");

    // sudah anggota → tidak bisa diundang lagi
    let r = call(
        &a,
        Method::POST,
        format!("{}/invitations", base(t)),
        &o,
        Some(json!({"email": "budi@x.id", "role_id": support})),
    )
    .await;
    assert_eq!(reason(&r), json!("already_member"));

    // pengguna lain yang login dengan email berbeda tidak bisa memakai token
    let other = member(&pool, tenant(&pool, "toko-b").await, "Owner").await;
    call(
        &a,
        Method::POST,
        format!("{}/invitations", base(t)),
        &o,
        Some(json!({"email": "sari@x.id", "role_id": support})),
    )
    .await;
    let tok2 = deliver(&a).await;
    assert_eq!(
        reason(&accept(&a, &tok2, Some(&other), None).await),
        json!("email_mismatch")
    );

    // dicabut → token mati; undangan ulang membatalkan undangan lama
    let list = call(
        &a,
        Method::GET,
        format!("{}/invitations", base(t)),
        &o,
        None,
    )
    .await;
    let iid = list.json()["data"][0]["id"].as_str().unwrap().to_owned();
    assert_eq!(
        call(
            &a,
            Method::DELETE,
            format!("{}/invitations/{iid}", base(t)),
            &o,
            None
        )
        .await
        .status,
        204
    );
    assert_eq!(
        accept(&a, &tok2, None, Some("rahasia-sari-123"))
            .await
            .status,
        404
    );

    // kedaluwarsa
    call(
        &a,
        Method::POST,
        format!("{}/invitations", base(t)),
        &o,
        Some(json!({"email": "dewi@x.id", "role_id": support})),
    )
    .await;
    let tok3 = deliver(&a).await;
    sqlx::query("UPDATE invitations SET created_at = now() - interval '9 days', expires_at = now() - interval '1 day' WHERE email = 'dewi@x.id'")
        .execute(&a.owner).await.unwrap();
    assert_eq!(
        accept(&a, &tok3, None, Some("rahasia-dewi-123"))
            .await
            .status,
        404
    );

    // akun sudah ada tanpa sesi → harus login dulu
    let existing_email: String = sqlx::query_scalar("SELECT email FROM users WHERE id = $1")
        .bind(other.user)
        .fetch_one(&a.owner)
        .await
        .unwrap();
    call(
        &a,
        Method::POST,
        format!("{}/invitations", base(t)),
        &o,
        Some(json!({"email": existing_email, "role_id": support})),
    )
    .await;
    let tok4 = deliver(&a).await;
    assert_eq!(
        reason(&accept(&a, &tok4, None, Some("rahasia-lain-123")).await),
        json!("account_exists")
    );
    assert_eq!(
        accept(&a, &tok4, Some(&other), None).await.status,
        200,
        "login dengan email yang sama → bergabung"
    );

    // anti-eskalasi pada undangan
    let lead = custom_role(
        &a,
        t,
        &o,
        "Perekrut",
        &["members.read", "members.invite", "tenant.read"],
    )
    .await;
    let rec = member(&pool, t, "Viewer").await;
    let recm = membership(&a, t, &o, rec.user).await;
    patch_member(&a, t, &o, &recm, json!({"role_id": lead})).await;
    let owner_role = role_id(&a, t, &o, "Owner").await;
    let r = call(
        &a,
        Method::POST,
        format!("{}/invitations", base(t)),
        &rec,
        Some(json!({"email": "z@x.id", "role_id": owner_role})),
    )
    .await;
    assert_eq!(reason(&r), json!("permission_escalation"));
    let joined: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM audit_logs WHERE tenant_id = $1 AND action = 'member.joined'",
    )
    .bind(t)
    .fetch_one(&a.owner)
    .await
    .unwrap();
    assert_eq!(joined, 2);
}
