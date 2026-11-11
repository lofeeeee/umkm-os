//! P1-2 security tests (PRD.md §4.1, §6.1) di Postgres + Redis nyata.
//! Butuh `eval "$(scripts/dev-db.sh up)"` (DATABASE_URL + REDIS_URL).

use std::time::Duration;

use sqlx::PgPool;
use umkm_database::MIGRATOR;
use umkm_domain::auth::{Email, PasswordHasher, RateRule};
use umkm_infrastructure::{
    auth::{Argon2idHasher, hash_token, new_token},
    auth_repo::{self as repo, RepoError, TokenPurpose},
    rate_limit::RateLimiter,
    redis::{KeyNamespace, RedisClient},
};
use umkm_shared::config::SecretString;
use uuid::Uuid;

fn hasher() -> Argon2idHasher {
    Argon2idHasher::new(1024, 1, 1).unwrap()
}

async fn new_user(pool: &PgPool, email: &str, password: &str) -> Uuid {
    let hash = hasher().hash(password).unwrap();
    repo::create_user(pool, &Email::parse(email).unwrap(), &hash)
        .await
        .unwrap()
}

const DAY: Duration = Duration::from_secs(86_400);

#[sqlx::test(migrator = "MIGRATOR")]
async fn register_duplicate_email_case_insensitive_is_rejected(pool: PgPool) {
    new_user(&pool, "Budi@X.id", "rahasia-123").await;
    let err = repo::create_user(
        &pool,
        &Email::parse("budi@x.ID").unwrap(),
        &hasher().hash("x-12345678").unwrap(),
    )
    .await
    .unwrap_err();
    assert!(matches!(err, RepoError::EmailTaken), "{err:?}");
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn login_lookup_case_insensitive_and_password_stored_hashed(pool: PgPool) {
    let id = new_user(&pool, "Budi@X.id", "rahasia-123").await;
    let rec = repo::find_login(&pool, &Email::parse("BUDI@x.id").unwrap())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(rec.user_id, id);
    assert!(rec.active);
    assert!(rec.password_hash.starts_with("$argon2id$"));
    assert!(!rec.password_hash.contains("rahasia-123"));
    assert!(hasher().verify("rahasia-123", &rec.password_hash).unwrap());
    assert!(
        repo::find_login(&pool, &Email::parse("nobody@x.id").unwrap())
            .await
            .unwrap()
            .is_none()
    );
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn session_lifecycle_and_token_never_stored_plaintext(pool: PgPool) {
    let u = new_user(&pool, "u@x.id", "rahasia-123").await;
    let (token, hash) = new_token().unwrap();
    let sid = repo::create_session(&pool, u, hash, DAY, Some("Firefox"))
        .await
        .unwrap();

    let stored: Vec<u8> = sqlx::query_scalar("SELECT token_hash FROM user_sessions WHERE id = $1")
        .bind(sid)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(stored.len(), 32);
    assert_ne!(stored, token.as_bytes()[..32].to_vec());

    let s = repo::authenticate_session(&pool, hash_token(&token))
        .await
        .unwrap()
        .unwrap();
    assert_eq!((s.session_id, s.user_id), (sid, u));
    // Token yang salah satu karakter
    let mut wrong = token.clone();
    wrong.replace_range(0..1, if wrong.starts_with('A') { "B" } else { "A" });
    assert!(
        repo::authenticate_session(&pool, hash_token(&wrong))
            .await
            .unwrap()
            .is_none()
    );

    assert!(repo::revoke_session(&pool, u, sid).await.unwrap());
    assert!(
        repo::authenticate_session(&pool, hash)
            .await
            .unwrap()
            .is_none(),
        "sesi dicabut masih valid"
    );
    assert!(
        !repo::revoke_session(&pool, u, sid).await.unwrap(),
        "cabut dua kali"
    );
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn expired_session_and_suspended_user_are_rejected(pool: PgPool) {
    let u = new_user(&pool, "u@x.id", "rahasia-123").await;
    let (_, h1) = new_token().unwrap();
    let sid = repo::create_session(&pool, u, h1, DAY, None).await.unwrap();
    sqlx::query("UPDATE user_sessions SET created_at = now() - interval '2 days', expires_at = now() - interval '1 second' WHERE id = $1")
        .bind(sid)
        .execute(&pool)
        .await
        .unwrap();
    assert!(
        repo::authenticate_session(&pool, h1)
            .await
            .unwrap()
            .is_none(),
        "sesi kedaluwarsa"
    );

    let (_, h2) = new_token().unwrap();
    repo::create_session(&pool, u, h2, DAY, None).await.unwrap();
    assert!(
        repo::authenticate_session(&pool, h2)
            .await
            .unwrap()
            .is_some()
    );
    sqlx::query("UPDATE users SET status = 'suspended' WHERE id = $1")
        .bind(u)
        .execute(&pool)
        .await
        .unwrap();
    assert!(
        repo::authenticate_session(&pool, h2)
            .await
            .unwrap()
            .is_none(),
        "user suspended"
    );
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn revoke_all_except_current_and_object_level_scoping(pool: PgPool) {
    let (u, v) = (
        new_user(&pool, "u@x.id", "rahasia-123").await,
        new_user(&pool, "v@x.id", "rahasia-123").await,
    );
    let mut u_sessions = vec![];
    for _ in 0..3 {
        let (_, h) = new_token().unwrap();
        u_sessions.push((
            repo::create_session(&pool, u, h, DAY, None).await.unwrap(),
            h,
        ));
    }
    let (_, vh) = new_token().unwrap();
    let v_sid = repo::create_session(&pool, v, vh, DAY, None).await.unwrap();

    // User u tidak bisa mencabut sesi milik v (object-level)
    assert!(!repo::revoke_session(&pool, u, v_sid).await.unwrap());
    assert!(
        repo::authenticate_session(&pool, vh)
            .await
            .unwrap()
            .is_some()
    );

    let current = u_sessions[0].0;
    assert_eq!(
        repo::revoke_all_sessions(&pool, u, Some(current))
            .await
            .unwrap(),
        2
    );
    assert!(
        repo::authenticate_session(&pool, u_sessions[0].1)
            .await
            .unwrap()
            .is_some()
    );
    assert!(
        repo::authenticate_session(&pool, u_sessions[1].1)
            .await
            .unwrap()
            .is_none()
    );
    assert!(
        repo::authenticate_session(&pool, u_sessions[2].1)
            .await
            .unwrap()
            .is_none()
    );
    assert_eq!(repo::revoke_all_sessions(&pool, u, None).await.unwrap(), 1);
    assert!(
        repo::authenticate_session(&pool, vh)
            .await
            .unwrap()
            .is_some(),
        "sesi v tidak boleh ikut tercabut"
    );
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn reset_token_is_one_time_purpose_bound_and_expires(pool: PgPool) {
    let u = new_user(&pool, "u@x.id", "rahasia-123").await;
    let (token, hash) = new_token().unwrap();
    repo::create_user_token(
        &pool,
        u,
        TokenPurpose::PasswordReset,
        hash,
        Duration::from_secs(1800),
    )
    .await
    .unwrap();

    // Tujuan berbeda tidak dapat memakai token ini
    assert!(
        repo::consume_user_token(&pool, TokenPurpose::EmailVerification, hash_token(&token))
            .await
            .unwrap()
            .is_none()
    );
    assert_eq!(
        repo::consume_user_token(&pool, TokenPurpose::PasswordReset, hash_token(&token))
            .await
            .unwrap(),
        Some(u)
    );
    assert!(
        repo::consume_user_token(&pool, TokenPurpose::PasswordReset, hash)
            .await
            .unwrap()
            .is_none(),
        "dipakai dua kali"
    );

    let (_, expired) = new_token().unwrap();
    repo::create_user_token(
        &pool,
        u,
        TokenPurpose::PasswordReset,
        expired,
        Duration::from_secs(60),
    )
    .await
    .unwrap();
    sqlx::query("UPDATE user_tokens SET created_at = now() - interval '2 hours', expires_at = now() - interval '1 second' WHERE token_hash = $1")
        .bind(&expired.0[..])
        .execute(&pool)
        .await
        .unwrap();
    assert!(
        repo::consume_user_token(&pool, TokenPurpose::PasswordReset, expired)
            .await
            .unwrap()
            .is_none(),
        "kedaluwarsa"
    );

    let (_, pending) = new_token().unwrap();
    repo::create_user_token(
        &pool,
        u,
        TokenPurpose::PasswordReset,
        pending,
        Duration::from_secs(1800),
    )
    .await
    .unwrap();
    assert_eq!(
        repo::invalidate_user_tokens(&pool, u, TokenPurpose::PasswordReset)
            .await
            .unwrap(),
        2
    );
    assert!(
        repo::consume_user_token(&pool, TokenPurpose::PasswordReset, pending)
            .await
            .unwrap()
            .is_none()
    );
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn concurrent_consumption_succeeds_exactly_once(pool: PgPool) {
    let u = new_user(&pool, "u@x.id", "rahasia-123").await;
    let (_, hash) = new_token().unwrap();
    repo::create_user_token(
        &pool,
        u,
        TokenPurpose::PasswordReset,
        hash,
        Duration::from_secs(1800),
    )
    .await
    .unwrap();
    let mut set = tokio::task::JoinSet::new();
    for _ in 0..20 {
        let pool = pool.clone();
        set.spawn(async move {
            repo::consume_user_token(&pool, TokenPurpose::PasswordReset, hash)
                .await
                .unwrap()
        });
    }
    let winners = set.join_all().await.into_iter().flatten().count();
    assert_eq!(winners, 1, "token sekali pakai dipakai {winners} kali");
}

// ------------------------------------------------------------- rate limit (Redis)

async fn limiter(prefix_suffix: &str) -> RateLimiter {
    let url =
        std::env::var("REDIS_URL").expect("REDIS_URL wajib: eval \"$(scripts/dev-db.sh up)\"");
    let conn = RedisClient::open(&SecretString::new(url))
        .unwrap()
        .connection_manager()
        .await
        .unwrap();
    // Prefix unik per tes agar tidak saling mengganggu.
    RateLimiter::new(
        conn,
        KeyNamespace::new(&format!("umkmtest:{prefix_suffix}")).unwrap(),
    )
}

fn unique() -> String {
    Uuid::new_v4().simple().to_string()
}

#[tokio::test]
async fn rate_limit_blocks_after_limit_and_is_per_subject() {
    const RULE: RateRule = RateRule {
        name: "login_email",
        limit: 5,
        window: Duration::from_secs(60),
    };
    let rl = limiter(&unique()).await;
    for i in 1..=5 {
        let d = rl.check(&RULE, "budi@x.id").await.unwrap();
        assert!(d.allowed, "percobaan {i} harus diizinkan");
    }
    let d = rl.check(&RULE, "budi@x.id").await.unwrap();
    assert!(!d.allowed, "percobaan ke-6 harus diblokir");
    assert_eq!(d.count, 6);
    assert!(
        (1..=60).contains(&d.retry_after_secs),
        "{}",
        d.retry_after_secs
    );
    assert!(
        rl.check(&RULE, "siti@x.id").await.unwrap().allowed,
        "subjek lain tidak terpengaruh"
    );

    rl.reset(&RULE, "budi@x.id").await.unwrap();
    assert!(
        rl.check(&RULE, "budi@x.id").await.unwrap().allowed,
        "setelah reset"
    );
}

/// Semantik window diuji lewat TTL key (deterministik), bukan `sleep`: expiry Redis
/// memakai jam dinding server, yang di WSL2 bisa dikoreksi mundur di bawah beban
/// sehingga tes berbasis sleep menjadi flaky (lihat docs/evidence/P1-6.txt).
#[tokio::test]
async fn rate_limit_window_is_fixed_from_first_attempt_and_key_hides_subject() {
    const RULE: RateRule = RateRule {
        name: "reset_email",
        limit: 1,
        window: Duration::from_secs(60),
    };
    let rl = limiter(&unique()).await;
    let key = rl.key(&RULE, "budi@x.id").unwrap();
    let url = std::env::var("REDIS_URL").unwrap();
    let mut raw = redis::Client::open(url)
        .unwrap()
        .get_multiplexed_async_connection()
        .await
        .unwrap();
    let pttl = |c: &mut redis::aio::MultiplexedConnection| {
        let mut c = c.clone();
        let k = key.clone();
        async move {
            redis::cmd("PTTL")
                .arg(k)
                .query_async::<i64>(&mut c)
                .await
                .unwrap()
        }
    };

    assert_eq!(pttl(&mut raw).await, -2, "belum ada key sebelum percobaan");
    assert!(rl.check(&RULE, "budi@x.id").await.unwrap().allowed);
    let t1 = pttl(&mut raw).await;
    assert!(
        (1..=60_000).contains(&t1),
        "TTL = window setelah percobaan pertama: {t1}"
    );
    let d = rl.check(&RULE, "budi@x.id").await.unwrap();
    assert!(!d.allowed);
    assert!((1..=60).contains(&d.retry_after_secs));
    let t2 = pttl(&mut raw).await;
    assert!(
        t2 <= t1,
        "fixed window: percobaan berikut tidak memperpanjang TTL ({t1} -> {t2})"
    );

    // Key hilang (window berakhir) -> hitungan mulai dari awal
    let _: i64 = redis::cmd("DEL")
        .arg(&key)
        .query_async(&mut raw)
        .await
        .unwrap();
    assert!(rl.check(&RULE, "budi@x.id").await.unwrap().allowed);

    assert!(key.contains(":platform:rl_reset_email:"), "{key}");
    assert!(
        !key.contains("budi"),
        "subjek tidak boleh tersimpan plaintext: {key}"
    );
}

#[tokio::test]
async fn rate_limit_concurrent_attempts_never_exceed_limit() {
    const RULE: RateRule = RateRule {
        name: "login_ip",
        limit: 10,
        window: Duration::from_secs(60),
    };
    let rl = limiter(&unique()).await;
    let mut set = tokio::task::JoinSet::new();
    for _ in 0..50 {
        let rl = rl.clone();
        set.spawn(async move { rl.check(&RULE, "203.0.113.7").await.unwrap().allowed });
    }
    let allowed = set.join_all().await.into_iter().filter(|a| *a).count();
    assert_eq!(allowed, 10, "INCR atomik: tepat `limit` yang lolos");
}

#[tokio::test]
async fn rate_limit_redis_unavailable_returns_error_for_fail_closed() {
    let client = RedisClient::open(&SecretString::new("redis://127.0.0.1:1/0")).unwrap();
    assert!(
        client.connection_manager().await.is_err(),
        "Redis mati harus error, bukan diam-diam lolos"
    );
}
