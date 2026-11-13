//! MFA TOTP lewat HTTP (AUTH-011/012, Gate F): setup → aktifkan → login dua langkah, anti-replay,
//! kode pemulihan sekali pakai, batas percobaan tantangan, rahasia terenkripsi & tidak bocor ke
//! audit, kebijakan tenant untuk role admin, dan Super Admin platform wajib MFA.

mod common;

use std::time::{SystemTime, UNIX_EPOCH};

use axum::http::Method;
use common::*;
use serde_json::{Value, json};
use sqlx::PgPool;
use umkm_api::auth::csrf_token;
use umkm_database::MIGRATOR;
use umkm_domain::{auth::PasswordHasher, mfa};
use umkm_infrastructure::{auth::Argon2idHasher, mfa::hotp};
use uuid::Uuid;

const PW: &str = "rahasia-mfa-123";

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs()
}

/// Hindari batas langkah 30 detik di tengah tes (kode dihitung di sini, diverifikasi di server).
async fn fresh_step() {
    let r = now() % mfa::STEP_SECS;
    if r >= mfa::STEP_SECS - 5 {
        tokio::time::sleep(std::time::Duration::from_secs(mfa::STEP_SECS - r + 1)).await;
    }
}

/// Kode TOTP untuk langkah `now + offset` (offset ∈ -1..=1, jendela server).
fn code(secret_b32: &str, offset: i64) -> String {
    let key = mfa::base32_decode(secret_b32).unwrap();
    let step = mfa::step_at(now()) + offset;
    format!("{:06}", hotp(&key, step as u64))
}

async fn user_with_password(pool: &PgPool, email: &str, platform_role: Option<&str>) -> Uuid {
    let hash = Argon2idHasher::new(1024, 1, 1).unwrap().hash(PW).unwrap();
    sqlx::query_scalar(
        "INSERT INTO users (email, password_hash, platform_role) VALUES ($1, $2, $3) RETURNING id",
    )
    .bind(email)
    .bind(hash)
    .bind(platform_role)
    .fetch_one(pool)
    .await
    .unwrap()
}

fn who_from(r: &Res, user: Uuid) -> Who {
    let set = r.headers[axum::http::header::SET_COOKIE].to_str().unwrap();
    let token = set
        .split(';')
        .next()
        .unwrap()
        .split_once('=')
        .unwrap()
        .1
        .to_owned();
    Who {
        user,
        cookie: format!("__Host-umkm_session={token}"),
        csrf: csrf_token(&token),
    }
}

async fn login(a: &TestApp, email: &str) -> Res {
    json(
        &a.router,
        Method::POST,
        "/api/v1/auth/login",
        None,
        Some(json!({ "email": email, "password": PW })),
    )
    .await
}

async fn login_mfa(a: &TestApp, token: &str, c: &str) -> Res {
    json(
        &a.router,
        Method::POST,
        "/api/v1/auth/login/mfa",
        None,
        Some(json!({ "mfa_token": token, "code": c })),
    )
    .await
}

async fn post(a: &TestApp, w: &Who, path: &str, body: Value) -> Res {
    json(&a.router, Method::POST, path, Some(w), Some(body)).await
}

/// Setup + aktifkan MFA dengan kode langkah sebelumnya; mengembalikan (rahasia, kode pemulihan).
async fn enroll(a: &TestApp, w: &Who) -> (String, Vec<String>) {
    let r = post(a, w, "/api/v1/auth/mfa/setup", json!({})).await;
    assert_eq!(r.status, 200, "{}", r.json());
    let secret = r.json()["data"]["secret"].as_str().unwrap().to_owned();
    let r = post(
        a,
        w,
        "/api/v1/auth/mfa/enable",
        json!({ "code": code(&secret, -1) }),
    )
    .await;
    assert_eq!(r.status, 200, "{}", r.json());
    let codes = r.json()["data"]["recovery_codes"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c.as_str().unwrap().to_owned())
        .collect();
    (secret, codes)
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn totp_enrollment_and_two_step_login(pool: PgPool) {
    fresh_step().await;
    let a = app(&pool).await;
    let email = "mfa@x.id";
    let uid = user_with_password(&pool, email, None).await;

    // Tanpa MFA: login biasa langsung memberi sesi.
    let r = login(&a, email).await;
    assert_eq!(r.status, 200, "{}", r.json());
    let w = who_from(&r, uid);
    let other = who_from(&login(&a, email).await, uid);

    let st = json(&a.router, Method::GET, "/api/v1/auth/mfa", Some(&w), None).await;
    assert_eq!(st.json()["data"]["enabled"], false);

    // Setup: rahasia 160-bit + URI otpauth. Belum aktif → login masih satu langkah.
    let r = post(&a, &w, "/api/v1/auth/mfa/setup", json!({})).await;
    assert_eq!(r.status, 200);
    let d = r.json()["data"].clone();
    let secret = d["secret"].as_str().unwrap().to_owned();
    assert_eq!(mfa::base32_decode(&secret).unwrap().len(), 20);
    assert!(
        d["otpauth_uri"]
            .as_str()
            .unwrap()
            .starts_with("otpauth://totp/UMKM%20Business%20OS:mfa%40x.id?secret=")
    );
    // Rahasia di DB terenkripsi (bukan plaintext/base32).
    let (ct, dek): (Vec<u8>, Vec<u8>) = sqlx::query_as(
        "SELECT encrypted_secret, wrapped_dek FROM mfa_credentials WHERE user_id = $1",
    )
    .bind(uid)
    .fetch_one(&pool)
    .await
    .unwrap();
    let raw = mfa::base32_decode(&secret).unwrap();
    assert!(!ct.windows(raw.len()).any(|x| x == raw.as_slice()));
    assert!(!ct.windows(secret.len()).any(|x| x == secret.as_bytes()));
    assert_eq!(dek.len(), 72);

    // Kode salah / format salah ditolak; kode benar mengaktifkan.
    let r = post(
        &a,
        &w,
        "/api/v1/auth/mfa/enable",
        json!({ "code": "000000" }),
    )
    .await;
    assert_eq!(r.status, 422);
    assert_eq!(r.json()["error"]["details"]["field"], "code");
    let r = post(&a, &w, "/api/v1/auth/mfa/enable", json!({ "code": "abc" })).await;
    assert_eq!(r.status, 422);
    let r = post(
        &a,
        &w,
        "/api/v1/auth/mfa/enable",
        json!({ "code": code(&secret, -1) }),
    )
    .await;
    assert_eq!(r.status, 200, "{}", r.json());
    let recovery: Vec<String> = r.json()["data"]["recovery_codes"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c.as_str().unwrap().to_owned())
        .collect();
    assert_eq!(recovery.len(), 10);
    // Setup ulang ditolak setelah aktif (rahasia tidak bisa ditimpa diam-diam).
    let r = post(&a, &w, "/api/v1/auth/mfa/setup", json!({})).await;
    assert_eq!(r.status, 409);
    // Sesi lain (belum MFA) dicabut; sesi ini bertanda MFA.
    let r = json(
        &a.router,
        Method::GET,
        "/api/v1/tenants",
        Some(&other),
        None,
    )
    .await;
    assert_eq!(r.status, 401);
    let st = json(&a.router, Method::GET, "/api/v1/auth/mfa", Some(&w), None).await;
    assert_eq!(
        (
            &st.json()["data"]["enabled"],
            &st.json()["data"]["session_verified"],
            &st.json()["data"]["recovery_codes_remaining"]
        ),
        (&json!(true), &json!(true), &json!(10))
    );

    // Login sekarang dua langkah: 202 tanpa cookie.
    let r = login(&a, email).await;
    assert_eq!(r.status, 202, "{}", r.json());
    assert!(r.headers.get(axum::http::header::SET_COOKIE).is_none());
    let tok = r.json()["data"]["mfa_token"].as_str().unwrap().to_owned();
    // Kode langkah -1 sudah dipakai saat enable → replay ditolak.
    let r = login_mfa(&a, &tok, &code(&secret, -1)).await;
    assert_eq!(r.status, 422);
    let r = login_mfa(&a, &tok, &code(&secret, 0)).await;
    assert_eq!(r.status, 200, "{}", r.json());
    let w2 = who_from(&r, uid);
    let s = json(
        &a.router,
        Method::GET,
        "/api/v1/auth/session",
        Some(&w2),
        None,
    )
    .await;
    assert_eq!(s.json()["data"]["mfa_verified"], true);
    // Tantangan sekali pakai.
    let r = login_mfa(&a, &tok, &code(&secret, 1)).await;
    assert_eq!(r.status, 401);

    // Kode pemulihan: sekali pakai, format bebas (huruf besar/tanpa '-').
    let tok = login(&a, email).await.json()["data"]["mfa_token"]
        .as_str()
        .unwrap()
        .to_owned();
    let r = login_mfa(&a, &tok, &recovery[0].to_uppercase().replace('-', "")).await;
    assert_eq!(r.status, 200, "{}", r.json());
    let tok = login(&a, email).await.json()["data"]["mfa_token"]
        .as_str()
        .unwrap()
        .to_owned();
    let r = login_mfa(&a, &tok, &recovery[0]).await;
    assert_eq!(r.status, 422, "kode pemulihan terpakai");

    // Batas percobaan: 5 salah → tantangan hangus walau kode berikutnya benar.
    for _ in 0..4 {
        assert_eq!(login_mfa(&a, &tok, "111111").await.status, 422);
    }
    let r = login_mfa(&a, &tok, &recovery[1]).await;
    assert_eq!(r.status, 401, "{}", r.json());

    // Matikan: password salah ditolak; password + kode pemulihan benar → mati.
    let r = post(
        &a,
        &w2,
        "/api/v1/auth/mfa/disable",
        json!({ "password": "salah-salah-salah", "code": recovery[2] }),
    )
    .await;
    assert_eq!(r.status, 422);
    assert_eq!(r.json()["error"]["details"]["field"], "password");
    let r = post(
        &a,
        &w2,
        "/api/v1/auth/mfa/disable",
        json!({ "password": PW, "code": recovery[2] }),
    )
    .await;
    assert_eq!(r.status, 200, "{}", r.json());
    assert_eq!(login(&a, email).await.status, 200);

    // Audit: event MFA tercatat; rahasia/kode tidak pernah masuk audit.
    let rows: Vec<(String, String)> = sqlx::query_as(
        "SELECT action, coalesce(summary::text, '') FROM audit_logs WHERE actor_user_id = $1 OR action LIKE 'auth.mfa.%'",
    )
    .bind(uid)
    .fetch_all(&pool)
    .await
    .unwrap();
    let actions: Vec<&str> = rows.iter().map(|r| r.0.as_str()).collect();
    for want in ["auth.mfa.enabled", "auth.mfa.failed", "auth.mfa.disabled"] {
        assert!(actions.contains(&want), "{want} di {actions:?}");
    }
    assert!(
        rows.iter()
            .any(|(a, s)| a == "auth.login.succeeded" && s.contains("recovery_code"))
    );
    let all = format!("{rows:?}");
    assert!(!all.contains(&secret));
    for c in &recovery {
        assert!(!all.contains(c.as_str()));
    }
}

async fn put(a: &TestApp, sec: &str, w: &Who, v: bool) -> Res {
    json(
        &a.router,
        Method::PUT,
        sec,
        Some(w),
        Some(json!({ "require_admin_mfa": v })),
    )
    .await
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn tenant_can_require_mfa_for_admin_roles(pool: PgPool) {
    fresh_step().await;
    let a = app(&pool).await;
    let t = tenant(&pool, "toko-mfa").await;
    let owner = member(&pool, t, "Owner").await;
    let manager = member(&pool, t, "Manager").await;
    let admin = member(&pool, t, "Admin").await;
    let viewer = member(&pool, t, "Viewer").await;
    let sec = format!("/api/v1/tenants/{t}/security");

    // Owner tanpa sesi MFA tidak boleh mewajibkan (akan mengunci dirinya sendiri).
    let r = put(&a, &sec, &owner, true).await;
    assert_eq!(r.status, 409, "{}", r.json());
    // Manager tidak punya tenant.update.
    assert_eq!(put(&a, &sec, &manager, false).await.status, 403);

    enroll(&a, &owner).await;
    let r = put(&a, &sec, &owner, true).await;
    assert_eq!(r.status, 200, "{}", r.json());
    let r = json(&a.router, Method::GET, &sec, Some(&owner), None).await;
    assert_eq!(r.json()["data"]["require_admin_mfa"], true);

    // Role admin tanpa MFA ditolak di seluruh endpoint tenant; role non-admin tidak terdampak.
    let tpath = format!("/api/v1/tenants/{t}");
    for w in [&manager, &admin] {
        let r = json(&a.router, Method::GET, &tpath, Some(w), None).await;
        assert_eq!(r.status, 403, "{}", r.json());
        assert_eq!(r.json()["error"]["details"]["reason"], "mfa_required");
    }
    let r = json(&a.router, Method::GET, &tpath, Some(&viewer), None).await;
    assert_eq!(r.status, 200);
    let r = json(&a.router, Method::GET, &tpath, Some(&owner), None).await;
    assert_eq!(r.status, 200);
    // Manager yang mendaftarkan MFA (sesi ditandai) bisa masuk lagi.
    enroll(&a, &manager).await;
    let r = json(&a.router, Method::GET, &tpath, Some(&manager), None).await;
    assert_eq!(r.status, 200, "{}", r.json());

    // Kebijakan tenant tidak berlaku di tenant lain.
    let t2 = tenant(&pool, "toko-lain").await;
    let m2 = member(&pool, t2, "Manager").await;
    let r = json(
        &a.router,
        Method::GET,
        &format!("/api/v1/tenants/{t2}"),
        Some(&m2),
        None,
    )
    .await;
    assert_eq!(r.status, 200);

    // Audit before/after.
    let s: String = sqlx::query_scalar(
        "SELECT summary::text FROM audit_logs WHERE action = 'tenant.security_updated' AND tenant_id = $1",
    )
    .bind(t)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert!(
        s.contains("\"after\": {\"require_admin_mfa\": true}"),
        "{s}"
    );

    // Mematikan kebijakan memulihkan akses.
    assert_eq!(put(&a, &sec, &owner, false).await.status, 200);
    let r = json(&a.router, Method::GET, &tpath, Some(&admin), None).await;
    assert_eq!(r.status, 200);
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn platform_super_admin_must_enroll_mfa(pool: PgPool) {
    fresh_step().await;
    let a = app(&pool).await;
    let email = "sa@x.id";
    let uid = user_with_password(&pool, email, Some("super_admin")).await;
    let r = login(&a, email).await;
    assert_eq!(r.status, 200);
    let w = who_from(&r, uid);

    let s = json(
        &a.router,
        Method::GET,
        "/api/v1/auth/session",
        Some(&w),
        None,
    )
    .await;
    assert_eq!(s.json()["data"]["mfa_enrollment_required"], true);
    // Endpoint biasa tertahan sampai MFA terdaftar.
    let r = json(&a.router, Method::GET, "/api/v1/tenants", Some(&w), None).await;
    assert_eq!(r.status, 403);
    assert_eq!(r.json()["error"]["details"]["reason"], "mfa_required");
    let st = json(&a.router, Method::GET, "/api/v1/auth/mfa", Some(&w), None).await;
    assert_eq!(st.json()["data"]["required_by_platform"], true);
    // Logout tetap bisa dari sesi yang tertahan.
    let w0 = who_from(&login(&a, email).await, uid);
    let r = post(&a, &w0, "/api/v1/auth/logout", json!({})).await;
    assert_eq!(r.status, 200, "{}", r.json());

    let (secret, _) = enroll(&a, &w).await;
    let r = json(&a.router, Method::GET, "/api/v1/tenants", Some(&w), None).await;
    assert_eq!(r.status, 200, "{}", r.json());
    // Tidak dapat dimatikan.
    let r = post(
        &a,
        &w,
        "/api/v1/auth/mfa/disable",
        json!({ "password": PW, "code": code(&secret, 0) }),
    )
    .await;
    assert_eq!(r.status, 403);
    // Login berikutnya selalu dua langkah.
    let r = login(&a, email).await;
    assert_eq!(r.status, 202);
}
