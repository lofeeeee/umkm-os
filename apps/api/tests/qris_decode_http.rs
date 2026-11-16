mod common;

use common::*;
use sqlx::PgPool;
use umkm_database::MIGRATOR;

#[sqlx::test(migrator = "MIGRATOR")]
async fn qris_payload_column_exists(pool: PgPool) {
    let exists: bool = sqlx::query_scalar(
        "SELECT EXISTS (
             SELECT 1 FROM information_schema.columns
             WHERE table_schema = 'public'
               AND table_name = 'commerce_settings'
               AND column_name = 'qris_static_payload'
           )",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert!(exists, "migration 0039 belum menambah qris_static_payload");
}

use axum::http::Method;
use serde_json::json;
use sqlx::migrate::Migrator as SqlxMigrator;
use umkm_infrastructure::gopay::{GopaySession, poller::now_ms};
use umkm_infrastructure::gopay_repo;
use umkm_shared::ids::TenantId;

async fn connect_gopay(a: &TestApp, t: uuid::Uuid) {
    let session = GopaySession {
        phone_e164: "+6281200000000".into(),
        merchant_id: Some("M1".into()),
        outlet_name: Some("Toko".into()),
        access_token: "access".into(),
        refresh_token: "refresh".into(),
        expires_at_ms: now_ms() + 86_400_000,
    };
    let keyring = a.state.auth.as_ref().unwrap().keyring.as_ref().unwrap();
    gopay_repo::put_session(
        &a.state.db.clone().unwrap(),
        TenantId::from_uuid(t),
        &session,
        keyring,
    )
    .await
    .unwrap();
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn static_qris_endpoint_is_gone(pool: PgPool) {
    let a = app(&pool).await;
    let t = tenant(&pool, "qris-gone").await;
    let o = member(&pool, t, "Owner").await;
    let r = json(
        &a.router,
        Method::PUT,
        &format!("/api/v1/tenants/{t}/gopay/static-qris"),
        Some(&o),
        Some(json!({ "qris": qris_payload() })),
    )
    .await;
    assert_eq!(r.status, 404);
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn gopay_status_uses_decoded_qris_payload(pool: PgPool) {
    let a = app(&pool).await;
    let t = tenant(&pool, "qris-status").await;
    let o = member(&pool, t, "Owner").await;
    let id = upload(&a.router, t, &o, qris_png()).await.json()["data"]["id"]
        .as_str()
        .unwrap()
        .to_owned();
    let url = format!("/api/v1/tenants/{t}/commerce/settings");
    let saved = json(
        &a.router,
        Method::PUT,
        &url,
        Some(&o),
        Some(json!({
            "qris": format!("asset:{id}"), "qris_receiver_name": "Toko", "checkout_enabled": true
        })),
    )
    .await;
    assert_eq!(saved.status, 200, "{}", saved.json());
    connect_gopay(&a, t).await;
    let status = json(
        &a.router,
        Method::GET,
        &format!("/api/v1/tenants/{t}/gopay"),
        Some(&o),
        None,
    )
    .await;
    assert_eq!(status.status, 200, "{}", status.json());
    assert_eq!(status.json()["data"]["connected"], true);
    assert_eq!(status.json()["data"]["has_static_qris"], true);
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn settings_save_decodes_uploaded_qris(pool: PgPool) {
    let a = app(&pool).await;
    let t = tenant(&pool, "qris-valid").await;
    let o = member(&pool, t, "Owner").await;
    let id = upload(&a.router, t, &o, qris_png()).await.json()["data"]["id"]
        .as_str()
        .unwrap()
        .to_owned();
    let url = format!("/api/v1/tenants/{t}/commerce/settings");
    let r = json(
        &a.router,
        Method::PUT,
        &url,
        Some(&o),
        Some(json!({
            "qris": format!("asset:{id}"),
            "qris_receiver_name": "Toko A",
            "checkout_enabled": true,
            "delivery_fee": "0"
        })),
    )
    .await;
    assert_eq!(r.status, 200, "{}", r.json());
    assert_eq!(r.json()["data"]["qris_static_ready"], true);
    let payload: Option<String> = sqlx::query_scalar(
        "SELECT qris_static_payload FROM commerce_settings WHERE tenant_id = $1",
    )
    .bind(t)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(payload.as_deref(), Some(qris_payload().as_str()));
}

// ------------------------------------------------------------------ Task 7: decode gagal, penggantian, backfill

async fn save_settings(a: &TestApp, t: uuid::Uuid, o: &Who, asset_id: &str, receiver: &str) -> Res {
    let url = format!("/api/v1/tenants/{t}/commerce/settings");
    json(
        &a.router,
        Method::PUT,
        &url,
        Some(o),
        Some(json!({
            "qris": format!("asset:{asset_id}"),
            "qris_receiver_name": receiver,
            "checkout_enabled": true,
            "delivery_fee": "0"
        })),
    )
    .await
}

async fn db_payload(pool: &PgPool, t: uuid::Uuid) -> Option<String> {
    sqlx::query_scalar("SELECT qris_static_payload FROM commerce_settings WHERE tenant_id = $1")
        .bind(t)
        .fetch_one(pool)
        .await
        .unwrap()
}

/// Step 1: gambar QRIS yang tidak bisa didekode tetap tersimpan (manual checkout pakai gambar);
/// payload tidak diisi dan `qris_static_ready == false`.
#[sqlx::test(migrator = "MIGRATOR")]
async fn settings_save_accepts_undecodable_qris(pool: PgPool) {
    let a = app(&pool).await;
    let t = tenant(&pool, "qris-bad").await;
    let o = member(&pool, t, "Owner").await;
    let id = upload(&a.router, t, &o, png()).await.json()["data"]["id"]
        .as_str()
        .unwrap()
        .to_owned();
    let r = save_settings(&a, t, &o, &id, "Toko").await;
    assert_eq!(r.status, 200, "{}", r.json());
    assert_eq!(r.json()["data"]["qris_static_ready"], false);
    assert_eq!(db_payload(&pool, t).await, None);
}

/// Step 2: mengganti QRIS dengan gambar yang gagal didekode menghapus payload lama
/// (payload tidak boleh menggambarkan gambar yang tidak sama).
#[sqlx::test(migrator = "MIGRATOR")]
async fn reupload_bad_image_clears_old_payload(pool: PgPool) {
    let a = app(&pool).await;
    let t = tenant(&pool, "qris-reupload").await;
    let o = member(&pool, t, "Owner").await;
    let good = upload(&a.router, t, &o, qris_png()).await.json()["data"]["id"]
        .as_str()
        .unwrap()
        .to_owned();
    let r = save_settings(&a, t, &o, &good, "Toko").await;
    assert_eq!(r.status, 200, "{}", r.json());
    assert_eq!(r.json()["data"]["qris_static_ready"], true);

    let bad = upload(&a.router, t, &o, png()).await.json()["data"]["id"]
        .as_str()
        .unwrap()
        .to_owned();
    let r = save_settings(&a, t, &o, &bad, "Toko").await;
    assert_eq!(r.status, 200, "{}", r.json());
    assert_eq!(r.json()["data"]["qris_static_ready"], false);
    assert_eq!(db_payload(&pool, t).await, None);
}

/// Step 3: PUT ulang dengan gambar yang sama mempertahankan payload tersimpan
/// (tidak perlu decode ulang; tetap siap walau sumbernya tidak berubah).
#[sqlx::test(migrator = "MIGRATOR")]
async fn unchanged_image_preserves_payload(pool: PgPool) {
    let a = app(&pool).await;
    let t = tenant(&pool, "qris-keep").await;
    let o = member(&pool, t, "Owner").await;
    let id = upload(&a.router, t, &o, qris_png()).await.json()["data"]["id"]
        .as_str()
        .unwrap()
        .to_owned();
    let r = save_settings(&a, t, &o, &id, "Toko").await;
    assert_eq!(r.status, 200, "{}", r.json());
    // Putar-ulang objeknya: hapus payload lalu PUT body yang identik, memastikan
    // pemeliharaan payload berasal dari jalur "gambar sama" di apply_settings.
    sqlx::query("UPDATE commerce_settings SET qris_static_payload = NULL WHERE tenant_id = $1")
        .bind(t)
        .execute(&pool)
        .await
        .unwrap();
    let r = save_settings(&a, t, &o, &id, "Toko").await;
    assert_eq!(r.status, 200, "{}", r.json());
    assert_eq!(r.json()["data"]["qris_static_ready"], true);
    assert_eq!(
        db_payload(&pool, t).await.as_deref(),
        Some(qris_payload().as_str())
    );
}

/// Step 4: gambar valid yang berbeda mengganti payload (nama merchant di tag 59
/// berbeda, CRC dihitung ulang oleh fixture).
#[sqlx::test(migrator = "MIGRATOR")]
async fn changed_valid_image_replaces_payload(pool: PgPool) {
    let a = app(&pool).await;
    let t = tenant(&pool, "qris-replace").await;
    let o = member(&pool, t, "Owner").await;
    let id_a = upload(&a.router, t, &o, qris_png()).await.json()["data"]["id"]
        .as_str()
        .unwrap()
        .to_owned();
    let r = save_settings(&a, t, &o, &id_a, "Toko A").await;
    assert_eq!(r.status, 200, "{}", r.json());
    assert_eq!(
        db_payload(&pool, t).await.as_deref(),
        Some(qris_payload().as_str())
    );

    let id_b = upload(&a.router, t, &o, qris_png_named("KOPI KAKI TIGA"))
        .await
        .json()["data"]["id"]
        .as_str()
        .unwrap()
        .to_owned();
    let r = save_settings(&a, t, &o, &id_b, "Toko B").await;
    assert_eq!(r.status, 200, "{}", r.json());
    assert_eq!(r.json()["data"]["qris_static_ready"], true);
    assert_eq!(
        db_payload(&pool, t).await.as_deref(),
        Some(qris_payload_named("KOPI KAKI TIGA").as_str())
    );
}

/// Step 5: backfill malas — pengaturan lama (payload NULL) terisi saat pertama kali
/// `GET /commerce/settings` dipanggil, tanpa perlu simpan ulang.
#[sqlx::test(migrator = "MIGRATOR")]
async fn get_settings_backfills_missing_payload(pool: PgPool) {
    let a = app(&pool).await;
    let t = tenant(&pool, "qris-backfill").await;
    let o = member(&pool, t, "Owner").await;
    let id = upload(&a.router, t, &o, qris_png()).await.json()["data"]["id"]
        .as_str()
        .unwrap()
        .to_owned();
    // Instalasi lama: baris settings eksis tanpa payload (persis kondisi pra-migrasi decode).
    sqlx::query(
        "INSERT INTO commerce_settings
         (tenant_id, qris_object_id, qris_receiver_name, checkout_enabled, qris_static_payload)
         VALUES ($1, $2, 'Toko', true, NULL)",
    )
    .bind(t)
    .bind(uuid::Uuid::parse_str(&id).unwrap())
    .execute(&pool)
    .await
    .unwrap();
    assert_eq!(db_payload(&pool, t).await, None);

    let r = json(
        &a.router,
        Method::GET,
        &format!("/api/v1/tenants/{t}/commerce/settings"),
        Some(&o),
        None,
    )
    .await;
    assert_eq!(r.status, 200, "{}", r.json());
    assert_eq!(r.json()["data"]["qris_static_ready"], true);
    assert_eq!(
        db_payload(&pool, t).await.as_deref(),
        Some(qris_payload().as_str())
    );
}

/// Step 6: backfill migrasi 0039 — baris `commerce_settings` yang sudah ada sebelum migrasi
/// terisi dari `gopay_sessions.static_qris` (legacy image). Migrasi diberi stage manual:
/// database disiapkan sampai 0038, data legacy dimasukkan, lalu 0039 dijalankan.
/// SQL migrasi itu sendiri tetap di `migrations/0039_qris_payload.up.sql`.
#[sqlx::test(migrator = "MIGRATOR")]
async fn migration_0039_backfills_from_legacy_gopay_sessions(pool: PgPool) {
    // Database uji ini sudah penuh (MIGRATOR). Tiru keadaan pra-0039: buang kolom & constraint
    // yang ditambahkan 0039, kosongkan catatan migrasinya, lalu masukkan data legacy.
    sqlx::query(
        "ALTER TABLE commerce_settings DROP CONSTRAINT IF EXISTS ck_commerce_settings_qris_payload",
    )
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query("ALTER TABLE commerce_settings DROP COLUMN IF EXISTS qris_static_payload")
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("DELETE FROM _sqlx_migrations WHERE version = 39")
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("DELETE FROM commerce_settings")
        .execute(&pool)
        .await
        .unwrap();

    let a = app(&pool).await;
    let t = tenant(&pool, "qris-legacy").await;
    let o = member(&pool, t, "Owner").await;
    let id = upload(&a.router, t, &o, qris_png()).await.json()["data"]["id"]
        .as_str()
        .unwrap()
        .to_owned();
    // Toko lama: gambar QRIS sudah diatur, payload belum ada, sesi GoPay legacy
    // sudah menyimpan `static_qris`.
    sqlx::query(
        "INSERT INTO commerce_settings
         (tenant_id, qris_object_id, qris_receiver_name, checkout_enabled)
         VALUES ($1, $2, 'Toko', true)",
    )
    .bind(t)
    .bind(uuid::Uuid::parse_str(&id).unwrap())
    .execute(&pool)
    .await
    .unwrap();
    connect_gopay(&a, t).await;
    sqlx::query("UPDATE gopay_sessions SET static_qris = $2 WHERE tenant_id = $1")
        .bind(t)
        .bind(qris_payload())
        .execute(&pool)
        .await
        .unwrap();

    // Jalankan ulang migrasi 0039 asli (dari source, bukan tiruan SQL) —
    // kolom + constraint + backfill dari gopay_sessions.
    let mig = SqlxMigrator::new(std::path::Path::new(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../migrations"
    )))
    .await
    .unwrap();
    mig.run_to(39, &pool).await.unwrap();

    let payload: Option<String> = db_payload(&pool, t).await;
    assert_eq!(payload.as_deref(), Some(qris_payload().as_str()));
    let _ = o; // owner hanya untuk upload
}

/// Step 7: menghubungkan GoPay (verify-otp jalur repositori langsung) memicu backfill
/// payload NULL dari gambar yang sudah ada → status `has_static_qris == true`.
#[sqlx::test(migrator = "MIGRATOR")]
async fn gopay_connect_backfills_missing_payload(pool: PgPool) {
    let a = app(&pool).await;
    let t = tenant(&pool, "qris-gopay-fill").await;
    let o = member(&pool, t, "Owner").await;
    let id = upload(&a.router, t, &o, qris_png()).await.json()["data"]["id"]
        .as_str()
        .unwrap()
        .to_owned();
    save_settings(&a, t, &o, &id, "Toko").await;
    sqlx::query("UPDATE commerce_settings SET qris_static_payload = NULL WHERE tenant_id = $1")
        .bind(t)
        .execute(&pool)
        .await
        .unwrap();
    connect_gopay(&a, t).await;
    // put_session (jalur repositori, bukan HTTP verify-otp) tidak memicu backfill sendiri —
    // backfill terjadi pada GET /gopay berikutnya (ensure_qris_payload di handler status).
    let status = json(
        &a.router,
        Method::GET,
        &format!("/api/v1/tenants/{t}/gopay"),
        Some(&o),
        None,
    )
    .await;
    assert_eq!(status.status, 200, "{}", status.json());
    assert_eq!(status.json()["data"]["connected"], true);
    assert_eq!(status.json()["data"]["has_static_qris"], true);
    assert_eq!(
        db_payload(&pool, t).await.as_deref(),
        Some(qris_payload().as_str())
    );
}
