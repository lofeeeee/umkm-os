//! P10: penyimpanan integrasi GoPay — segel/buka sesi (envelope), charge nominal unik, anti
//! double-claim. Dijalankan dengan MIGRATOR (menerapkan migrasi 0036).

use base64::{Engine, engine::general_purpose::STANDARD};
use sqlx::PgPool;
use umkm_database::{MIGRATOR, begin_tenant_tx};
use umkm_domain::catalog::Money;
use umkm_infrastructure::payment_repo;
use umkm_infrastructure::{
    crypto::Keyring,
    gopay::GopaySession,
    gopay_repo::{self, GopayRepoError},
};
use umkm_shared::ids::TenantId;
use uuid::Uuid;

fn keyring() -> Keyring {
    Keyring::parse(&format!("v1:{}", STANDARD.encode([7u8; 32]))).unwrap()
}

/// QRIS statis valid (CRC dihitung oleh implementasi domain).
fn static_qris() -> String {
    use umkm_domain::commerce::qris::crc16_ccitt;
    let fields = [
        ("00", "01"),
        ("01", "11"),
        ("26", "0014COM.EXAMPLE0208ABCDEF12"),
        ("53", "360"),
        ("58", "ID"),
        ("59", "WARUNG KOPI"),
        ("60", "JAKARTA"),
    ];
    let mut body = String::new();
    for (id, v) in fields {
        body.push_str(id);
        body.push_str(&format!("{:02}", v.len()));
        body.push_str(v);
    }
    body.push_str("6304");
    let crc = crc16_ccitt(body.as_bytes());
    body.push_str(&format!("{crc:04X}"));
    body
}

fn session() -> GopaySession {
    GopaySession {
        phone_e164: "+6281234567890".into(),
        merchant_id: Some("M1".into()),
        outlet_name: Some("Warung Kopi".into()),
        access_token: "AT-rahasia".into(),
        refresh_token: "RT-rahasia".into(),
        expires_at_ms: 1_800_000_000_000,
    }
}

async fn tenant(pool: &PgPool) -> TenantId {
    let id: Uuid =
        sqlx::query_scalar("INSERT INTO tenants (name, slug) VALUES ('T', 'toko-a') RETURNING id")
            .fetch_one(pool)
            .await
            .unwrap();
    TenantId::from_uuid(id)
}

async fn order(pool: &PgPool, t: TenantId, key: &str) -> Uuid {
    let mut tx = begin_tenant_tx(pool, t).await.unwrap();
    let c: Uuid = sqlx::query_scalar(
        "INSERT INTO customers (tenant_id, name, email) VALUES ($1, 'S', 's@x.id') RETURNING id",
    )
    .bind(t.as_uuid())
    .fetch_one(&mut *tx)
    .await
    .unwrap();
    let id: Uuid = sqlx::query_scalar(
        "INSERT INTO orders (tenant_id, code, customer_id, status, fulfillment, customer_name,
             subtotal, delivery_fee, total, access_token_hash, idempotency_key, request_hash,
             privacy_version, terms_version, consented_at, expires_at)
         VALUES ($1, $2, $3, 'AWAITING_PAYMENT', 'pickup', 'S', 100, 0, 100,
             sha256(gen_random_uuid()::text::bytea), $4, sha256(gen_random_uuid()::text::bytea),
             'v0', 'v0', now(), now() + interval '1 hour') RETURNING id",
    )
    .bind(t.as_uuid())
    .bind(format!("ORD-{}", key.to_uppercase()))
    .bind(c)
    .bind(key)
    .fetch_one(&mut *tx)
    .await
    .unwrap();
    tx.commit().await.unwrap();
    id
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn session_seal_open_roundtrip(pool: PgPool) {
    let k = keyring();
    let t = tenant(&pool).await;

    assert!(
        gopay_repo::get_session(&pool, t, &k)
            .await
            .unwrap()
            .is_none()
    );

    gopay_repo::put_session(&pool, t, &session(), &k)
        .await
        .unwrap();
    let stored = gopay_repo::get_session(&pool, t, &k)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(stored.session.access_token, "AT-rahasia");
    assert_eq!(stored.session.merchant_id.as_deref(), Some("M1"));
    assert_eq!(stored.status, "active");
    assert!(stored.static_qris.is_none());

    // QRIS statis diatur terpisah, dipertahankan saat sesi di-refresh (put ulang).
    gopay_repo::set_static_qris(&pool, t, &static_qris())
        .await
        .unwrap();
    gopay_repo::put_session(&pool, t, &session(), &k)
        .await
        .unwrap();
    let stored = gopay_repo::get_session(&pool, t, &k)
        .await
        .unwrap()
        .unwrap();
    assert!(stored.static_qris.is_some());

    gopay_repo::mark_needs_relogin(&pool, t).await.unwrap();
    let stored = gopay_repo::get_session(&pool, t, &k)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(stored.status, "needs_relogin");
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn wrong_key_fails_to_open(pool: PgPool) {
    let t = tenant(&pool).await;
    gopay_repo::put_session(&pool, t, &session(), &keyring())
        .await
        .unwrap();

    let other = Keyring::parse(&format!("v1:{}", STANDARD.encode([9u8; 32]))).unwrap();
    assert!(matches!(
        gopay_repo::get_session(&pool, t, &other).await,
        Err(GopayRepoError::Crypto)
    ));
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn charges_have_unique_amounts_and_dedup_txn(pool: PgPool) {
    let t = tenant(&pool).await;
    let q = static_qris();
    let o1 = order(&pool, t, "key-aaaa-01").await;
    let o2 = order(&pool, t, "key-bbbb-02").await;

    let c1 = gopay_repo::create_charge(&pool, t, o1, 30_000, &q, 1800)
        .await
        .unwrap();
    let c2 = gopay_repo::create_charge(&pool, t, o2, 30_000, &q, 1800)
        .await
        .unwrap();
    // Nominal dasar sama → total harus berbeda (kode unik berbeda).
    assert_ne!(c1.total_rupiah, c2.total_rupiah);
    assert_eq!(c1.total_rupiah, 30_000 + c1.unique_code as i64);
    // QRIS dinamis valid.
    umkm_domain::commerce::qris::verify(&c1.qris_payload).unwrap();

    // get_charge mengembalikan yang sama.
    let got = gopay_repo::get_charge(&pool, t, o1).await.unwrap().unwrap();
    assert_eq!(got.total_rupiah, c1.total_rupiah);
    assert_eq!(
        gopay_repo::pending_charges(&pool, t).await.unwrap().len(),
        2
    );

    // Satu transaksi hanya boleh melunasi satu charge.
    assert!(
        gopay_repo::mark_matched(&pool, t, o1, "TXN-1")
            .await
            .unwrap()
    );
    assert!(
        !gopay_repo::mark_matched(&pool, t, o2, "TXN-1")
            .await
            .unwrap()
    );
    // Charge yang sama tak bisa dicocokkan dua kali.
    assert!(
        !gopay_repo::mark_matched(&pool, t, o1, "TXN-2")
            .await
            .unwrap()
    );

    // Setelah satu cocok, tinggal satu pending.
    assert_eq!(
        gopay_repo::pending_charges(&pool, t).await.unwrap().len(),
        1
    );
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn auto_confirm_moves_order_to_processing(pool: PgPool) {
    let t = tenant(&pool).await;
    let o = order(&pool, t, "key-auto-01").await;

    // Payment pending (seperti dibuat saat checkout QRIS).
    {
        let mut tx = begin_tenant_tx(&pool, t).await.unwrap();
        sqlx::query(
            "INSERT INTO payments (tenant_id, order_id, amount, status) VALUES ($1, $2, 100, 'pending')",
        )
        .bind(t.as_uuid())
        .bind(o)
        .execute(&mut *tx)
        .await
        .unwrap();
        tx.commit().await.unwrap();
    }

    // Pembeli membayar total (base 100 + kode unik 50).
    let next = payment_repo::confirm_auto(
        &pool,
        t,
        o,
        Money::parse("150").unwrap(),
        "TXN-9",
        (None, None),
    )
    .await
    .unwrap();
    assert_eq!(next, umkm_domain::commerce::state::OrderStatus::Processing);

    let status: String = sqlx::query_scalar("SELECT status FROM orders WHERE id = $1")
        .bind(o)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(status, "PROCESSING");

    // Audit sistem tercatat tanpa aktor pengguna.
    let (action, actor): (String, Option<Uuid>) = sqlx::query_as(
        "SELECT action, actor_user_id FROM audit_logs WHERE resource_id = $1 AND action = 'payment.confirmed_auto'",
    )
    .bind(o.to_string())
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(action, "payment.confirmed_auto");
    assert!(actor.is_none());

    // Idempoten: order sudah bukan AWAITING_PAYMENT → tolak konfirmasi kedua.
    assert!(
        payment_repo::confirm_auto(
            &pool,
            t,
            o,
            Money::parse("150").unwrap(),
            "TXN-9",
            (None, None)
        )
        .await
        .is_err()
    );
}

// ---- Penjaga sesi (keep-alive, migrasi 0040) ----

use gopay_repo::LockResult;

#[sqlx::test(migrator = "MIGRATOR")]
async fn refresh_lock_save_and_fail_tracking(pool: PgPool) {
    let t = tenant(&pool).await;
    let k = keyring();
    assert!(matches!(
        gopay_repo::lock_for_refresh(&pool, t, &k).await.unwrap(),
        LockResult::NotConnected
    ));
    gopay_repo::put_session(&pool, t, &session(), &k)
        .await
        .unwrap();
    let stored = gopay_repo::get_session(&pool, t, &k)
        .await
        .unwrap()
        .unwrap();
    assert!(
        stored.refreshed_at_ms.is_some(),
        "terhubung = baru diperbarui"
    );
    assert_eq!(stored.refresh_failures, 0);

    // Gagal sementara: tetap aktif, penghitung & kode tercatat.
    let LockResult::Locked(lock) = gopay_repo::lock_for_refresh(&pool, t, &k).await.unwrap() else {
        panic!("harus terkunci");
    };
    lock.fail("status_503", false).await.unwrap();
    let stored = gopay_repo::get_session(&pool, t, &k)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(stored.status, "active");
    assert_eq!(stored.refresh_failures, 1);
    assert_eq!(stored.last_error.as_deref(), Some("status_503"));

    // Berhasil: token baru tersimpan, penghitung & error direset.
    let LockResult::Locked(lock) = gopay_repo::lock_for_refresh(&pool, t, &k).await.unwrap() else {
        panic!("harus terkunci");
    };
    let fresh = GopaySession {
        access_token: "AT-baru".into(),
        refresh_token: "RT-baru".into(),
        ..session()
    };
    lock.save(&fresh, &k).await.unwrap();
    let stored = gopay_repo::get_session(&pool, t, &k)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(stored.session.refresh_token, "RT-baru");
    assert_eq!(stored.refresh_failures, 0);
    assert!(stored.last_error.is_none());

    // Ditolak definitif: perlu login ulang.
    let LockResult::Locked(lock) = gopay_repo::lock_for_refresh(&pool, t, &k).await.unwrap() else {
        panic!("harus terkunci");
    };
    lock.fail("unauthorized", true).await.unwrap();
    let stored = gopay_repo::get_session(&pool, t, &k)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(stored.status, "needs_relogin");
    assert_eq!(stored.last_error.as_deref(), Some("unauthorized"));

    // "Cek sekarang": access token ternyata masih diterima tetapi perpanjangan ditolak → aktif lagi,
    // kode refresh_… tercatat, refreshed_at digeser (worker tidak mencoba tiap siklus).
    let LockResult::Locked(lock) = gopay_repo::lock_for_refresh(&pool, t, &k).await.unwrap() else {
        panic!("harus terkunci");
    };
    lock.keep_active(Some("refresh_status_400")).await.unwrap();
    let stored = gopay_repo::get_session(&pool, t, &k)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(stored.status, "active");
    assert_eq!(stored.last_error.as_deref(), Some("refresh_status_400"));
    assert!(stored.refreshed_at_ms.is_some());

    // Token terbukti berfungsi tanpa perlu refresh → galat lama dibersihkan.
    let LockResult::Locked(lock) = gopay_repo::lock_for_refresh(&pool, t, &k).await.unwrap() else {
        panic!("harus terkunci");
    };
    lock.keep_active(None).await.unwrap();
    let stored = gopay_repo::get_session(&pool, t, &k)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(stored.status, "active");
    assert!(stored.last_error.is_none());

    // Hubungkan ulang memulihkan status dan mereset pelacakan.
    gopay_repo::put_session(&pool, t, &session(), &k)
        .await
        .unwrap();
    let stored = gopay_repo::get_session(&pool, t, &k)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(stored.status, "active");
    assert!(stored.last_error.is_none());
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn concurrent_refresh_is_skipped_while_locked(pool: PgPool) {
    let t = tenant(&pool).await;
    let k = keyring();
    gopay_repo::put_session(&pool, t, &session(), &k)
        .await
        .unwrap();
    let LockResult::Locked(held) = gopay_repo::lock_for_refresh(&pool, t, &k).await.unwrap() else {
        panic!("harus terkunci");
    };
    // Proses kedua tidak menunggu dan tidak ikut me-refresh (mencegah rotasi token ganda).
    assert!(matches!(
        gopay_repo::lock_for_refresh(&pool, t, &k).await.unwrap(),
        LockResult::Busy
    ));
    held.release().await.unwrap();
    assert!(matches!(
        gopay_repo::lock_for_refresh(&pool, t, &k).await.unwrap(),
        LockResult::Locked(_)
    ));
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn refresh_due_lists_expiring_and_stale_sessions_only(pool: PgPool) {
    let k = keyring();
    let mk = |slug: &'static str| {
        let pool = pool.clone();
        async move {
            let id: Uuid = sqlx::query_scalar(
                "INSERT INTO tenants (name, slug) VALUES ('T', $1) RETURNING id",
            )
            .bind(slug)
            .fetch_one(&pool)
            .await
            .unwrap();
            TenantId::from_uuid(id)
        }
    };
    let (fresh, expiring, stale, dead) = (
        mk("toko-segar").await,
        mk("toko-habis").await,
        mk("toko-lama").await,
        mk("toko-mati").await,
    );
    let far = GopaySession {
        expires_at_ms: 4_000_000_000_000,
        ..session()
    };
    for t in [fresh, stale, dead] {
        gopay_repo::put_session(&pool, t, &far, &k).await.unwrap();
    }
    // Token tinggal 10 menit.
    let soon_ms = (std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as i64)
        + 10 * 60_000;
    gopay_repo::put_session(
        &pool,
        expiring,
        &GopaySession {
            expires_at_ms: soon_ms,
            ..session()
        },
        &k,
    )
    .await
    .unwrap();
    // Terakhir diperbarui 7 jam lalu (refresh penjaga 6 jam).
    sqlx::query(
        "UPDATE gopay_sessions SET refreshed_at = now() - interval '7 hours' WHERE tenant_id = $1",
    )
    .bind(stale.as_uuid())
    .execute(&pool)
    .await
    .unwrap();
    // Perlu login ulang: tidak di-refresh otomatis.
    sqlx::query("UPDATE gopay_sessions SET status = 'needs_relogin', refreshed_at = now() - interval '7 hours' WHERE tenant_id = $1")
        .bind(dead.as_uuid())
        .execute(&pool)
        .await
        .unwrap();

    let due = gopay_repo::refresh_due_tenants(&pool, 100).await.unwrap();
    assert!(due.contains(&expiring));
    assert!(due.contains(&stale));
    assert!(!due.contains(&fresh));
    assert!(!due.contains(&dead));
}
