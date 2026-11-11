//! P1-6: transactional outbox + worker email (PRD.md §4.13, BR-017) di Postgres nyata.

use std::{sync::Arc, time::Duration};

use sqlx::PgPool;
use umkm_application::{
    notify::{OutboxEvent, SendError},
    onboarding::{LegalVersions, RegisterCommand, register},
};
use umkm_database::MIGRATOR;
use umkm_domain::auth::AuthPolicy;
use umkm_infrastructure::{
    auth::{Argon2idHasher, hash_token},
    metrics::Metrics,
    onboarding_repo::{BlockingArgon2, OsTokenIssuer, PgRegistrationStore},
    outbox::{self, CapturingEmailSender, WorkerConfig, process_once},
};
use uuid::Uuid;

fn cfg(worker: &str) -> WorkerConfig {
    WorkerConfig {
        worker_id: worker.into(),
        batch: 10,
        lease: Duration::from_secs(30),
        max_attempts: 3,
        base_backoff: Duration::from_secs(10),
        verify_url_base: "https://app.ryasai.my.id/verify-email".into(),
        policy: AuthPolicy::default(),
    }
}

async fn register_user(pool: &PgPool, email: &str) -> Uuid {
    let hasher = BlockingArgon2(Arc::new(Argon2idHasher::new(1024, 1, 1).unwrap()));
    let cmd = RegisterCommand {
        email: email.into(),
        password: "rahasia-123".into(),
        business_name: format!("Toko {email}"),
        slug: None,
        accept_terms: true,
        accept_privacy: true,
        user_agent: None,
    };
    let legal = LegalVersions {
        privacy_policy: "v0".into(),
        terms_of_service: "v0".into(),
    };
    register(
        &PgRegistrationStore::new(pool.clone()),
        &hasher,
        &OsTokenIssuer,
        cmd,
        "ryasai.my.id",
        &AuthPolicy::default(),
        &legal,
    )
    .await
    .unwrap()
    .registered
    .user_id
}

async fn statuses(pool: &PgPool) -> Vec<(String, i32)> {
    sqlx::query_as("SELECT status, attempts FROM notification_outbox ORDER BY created_at")
        .fetch_all(pool)
        .await
        .unwrap()
}

fn token_from(body: &str) -> String {
    body.split("#token=")
        .nth(1)
        .unwrap()
        .split_whitespace()
        .next()
        .unwrap()
        .to_owned()
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn registration_enqueues_event_in_same_transaction_without_secrets(pool: PgPool) {
    let u = register_user(&pool, "budi@x.id").await;
    let (ty, payload): (String, serde_json::Value) =
        sqlx::query_as("SELECT event_type, payload FROM notification_outbox")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(ty, "auth.email_verification_requested");
    assert_eq!(
        payload,
        serde_json::json!({ "user_id": u }),
        "payload hanya user_id; tanpa email/token"
    );
    // Registrasi gagal (email ganda) tidak meninggalkan event
    let hasher = BlockingArgon2(Arc::new(Argon2idHasher::new(1024, 1, 1).unwrap()));
    let dup = RegisterCommand {
        email: "BUDI@x.id".into(),
        password: "rahasia-123".into(),
        business_name: "X".into(),
        slug: None,
        accept_terms: true,
        accept_privacy: true,
        user_agent: None,
    };
    let legal = LegalVersions {
        privacy_policy: "v0".into(),
        terms_of_service: "v0".into(),
    };
    assert!(
        register(
            &PgRegistrationStore::new(pool.clone()),
            &hasher,
            &OsTokenIssuer,
            dup,
            "ryasai.my.id",
            &AuthPolicy::default(),
            &legal
        )
        .await
        .is_err()
    );
    assert_eq!(statuses(&pool).await.len(), 1);
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn worker_sends_verification_and_token_verifies_email(pool: PgPool) {
    let u = register_user(&pool, "budi@x.id").await;
    let sender = CapturingEmailSender::default();
    let tick = process_once(&pool, &sender, &cfg("w1"), &Metrics::new())
        .await
        .unwrap();
    assert_eq!((tick.claimed, tick.sent), (1, 1));
    assert_eq!(statuses(&pool).await, [("sent".into(), 1)]);

    let sent = sender.sent.lock().unwrap().clone();
    assert_eq!(sent.len(), 1);
    assert_eq!(sent[0].to, "budi@x.id");
    let token = token_from(&sent[0].text_body);
    // Hanya hash yang tersimpan
    let stored: Vec<u8> = sqlx::query_scalar(
        "SELECT token_hash FROM user_tokens WHERE user_id = $1 AND purpose = 'email_verification'",
    )
    .bind(u)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(stored, hash_token(&token).0.to_vec());
    let leaked: i64 = sqlx::query_scalar("SELECT count(*) FROM notification_outbox WHERE payload::text LIKE '%' || $1 || '%' OR coalesce(last_error, '') LIKE '%' || $1 || '%'")
        .bind(&token)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(leaked, 0);

    assert_eq!(
        outbox::verify_email(&pool, hash_token(&token))
            .await
            .unwrap(),
        Some(u)
    );
    let verified: bool =
        sqlx::query_scalar("SELECT email_verified_at IS NOT NULL FROM users WHERE id = $1")
            .bind(u)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert!(verified);
    assert_eq!(
        outbox::verify_email(&pool, hash_token(&token))
            .await
            .unwrap(),
        None,
        "token sekali pakai"
    );
    // Putaran berikut tidak mengirim ulang
    assert_eq!(
        process_once(&pool, &sender, &cfg("w1"), &Metrics::new())
            .await
            .unwrap()
            .claimed,
        0
    );
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn already_verified_user_gets_no_email(pool: PgPool) {
    let u = register_user(&pool, "budi@x.id").await;
    sqlx::query("UPDATE users SET email_verified_at = now() WHERE id = $1")
        .bind(u)
        .execute(&pool)
        .await
        .unwrap();
    let sender = CapturingEmailSender::default();
    let tick = process_once(&pool, &sender, &cfg("w1"), &Metrics::new())
        .await
        .unwrap();
    assert_eq!(tick.sent, 1, "event selesai");
    assert!(sender.sent.lock().unwrap().is_empty(), "tanpa email");
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn retryable_failure_backs_off_then_dies_after_max_attempts(pool: PgPool) {
    register_user(&pool, "budi@x.id").await;
    let sender = CapturingEmailSender::default();
    let fail = SendError {
        retryable: true,
        class: "timeout",
    };
    *sender.fail_next.lock().unwrap() = vec![fail.clone(), fail.clone(), fail];
    let c = cfg("w1");

    let t = process_once(&pool, &sender, &c, &Metrics::new())
        .await
        .unwrap();
    assert_eq!(t.retried, 1);
    let (status, attempts, err, delay): (String, i32, Option<String>, f64) = sqlx::query_as(
        "SELECT status, attempts, last_error, extract(epoch FROM available_at - now())::float8 FROM notification_outbox",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(
        (status.as_str(), attempts, err.as_deref()),
        ("pending", 1, Some("timeout"))
    );
    assert!((8.0..=10.5).contains(&delay), "backoff ~10s, dapat {delay}");
    // Belum jatuh tempo -> tidak diklaim
    assert_eq!(
        process_once(&pool, &sender, &c, &Metrics::new())
            .await
            .unwrap()
            .claimed,
        0
    );

    for expected in [("pending", 2), ("dead", 3)] {
        sqlx::query("UPDATE notification_outbox SET available_at = now()")
            .execute(&pool)
            .await
            .unwrap();
        process_once(&pool, &sender, &c, &Metrics::new())
            .await
            .unwrap();
        assert_eq!(statuses(&pool).await, [(expected.0.into(), expected.1)]);
    }
    assert!(sender.sent.lock().unwrap().is_empty());
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn non_retryable_failure_goes_dead_immediately(pool: PgPool) {
    register_user(&pool, "budi@x.id").await;
    let sender = CapturingEmailSender::default();
    *sender.fail_next.lock().unwrap() = vec![SendError {
        retryable: false,
        class: "rejected",
    }];
    let t = process_once(&pool, &sender, &cfg("w1"), &Metrics::new())
        .await
        .unwrap();
    assert_eq!(t.dead, 1);
    assert_eq!(statuses(&pool).await, [("dead".into(), 1)]);
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn crashed_worker_lease_is_reclaimed_and_stale_worker_cannot_finish(pool: PgPool) {
    register_user(&pool, "budi@x.id").await;
    // Worker A mengklaim lalu "crash" (tidak pernah menandai selesai)
    let claimed = outbox::claim(&pool, "worker-a", 10, Duration::from_secs(30))
        .await
        .unwrap();
    assert_eq!(claimed.len(), 1);
    // Selama lease berlaku, worker B tidak mendapat apa-apa
    assert!(
        outbox::claim(&pool, "worker-b", 10, Duration::from_secs(30))
            .await
            .unwrap()
            .is_empty()
    );
    // Lease kedaluwarsa -> B mengambil alih
    sqlx::query("UPDATE notification_outbox SET locked_until = now() - interval '1 second'")
        .execute(&pool)
        .await
        .unwrap();
    let sender = CapturingEmailSender::default();
    let t = process_once(&pool, &sender, &cfg("worker-b"), &Metrics::new())
        .await
        .unwrap();
    assert_eq!((t.claimed, t.sent), (1, 1));
    // Worker A yang "bangun" tidak bisa menimpa status
    assert!(
        !outbox::mark_sent(&pool, claimed[0].id, "worker-a")
            .await
            .unwrap()
    );
    assert_eq!(statuses(&pool).await, [("sent".into(), 2)]);
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn concurrent_workers_never_double_send(pool: PgPool) {
    for i in 0..20 {
        register_user(&pool, &format!("u{i}@x.id")).await;
    }
    let sender = Arc::new(CapturingEmailSender::default());
    let mut set = tokio::task::JoinSet::new();
    for w in 0..4 {
        let (pool, sender) = (pool.clone(), sender.clone());
        set.spawn(async move {
            let c = WorkerConfig {
                batch: 3,
                ..cfg(&format!("w{w}"))
            };
            let m = Metrics::new();
            let mut total = 0;
            loop {
                let t = process_once(&pool, sender.as_ref(), &c, &m).await.unwrap();
                if t.claimed == 0 {
                    break total;
                }
                total += t.sent;
            }
        });
    }
    let sent_total: usize = set.join_all().await.into_iter().sum();
    let mut recipients: Vec<String> = sender
        .sent
        .lock()
        .unwrap()
        .iter()
        .map(|m| m.to.clone())
        .collect();
    assert_eq!(sent_total, 20);
    recipients.sort();
    recipients.dedup();
    assert_eq!(recipients.len(), 20, "tiap user tepat satu email");
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn enqueue_is_idempotent_and_unknown_events_die(pool: PgPool) {
    let ev = OutboxEvent::email_verification_requested(Uuid::new_v4());
    assert!(outbox::enqueue(&pool, &ev).await.unwrap());
    assert!(
        !outbox::enqueue(&pool, &ev).await.unwrap(),
        "kunci sama diabaikan"
    );
    sqlx::query("INSERT INTO notification_outbox (event_type, payload, idempotency_key) VALUES ('mystery.event', '{}', 'k1')")
        .execute(&pool)
        .await
        .unwrap();
    let t = process_once(
        &pool,
        &CapturingEmailSender::default(),
        &cfg("w1"),
        &Metrics::new(),
    )
    .await
    .unwrap();
    // user acak tidak ada -> selesai tanpa email; event tak dikenal -> dead
    assert_eq!((t.sent, t.dead), (1, 1));
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn metrics_track_jobs_failures_and_depth(pool: PgPool) {
    register_user(&pool, "a@x.id").await;
    register_user(&pool, "b@x.id").await;
    let sender = CapturingEmailSender::default();
    *sender.fail_next.lock().unwrap() = vec![SendError {
        retryable: true,
        class: "timeout",
    }];
    let m = Metrics::new();
    process_once(&pool, &sender, &cfg("w1"), &m).await.unwrap();
    let text = m.encode();
    assert!(
        text.contains(r#"worker_jobs_total{job="auth.email_verification_requested"} 2"#),
        "{text}"
    );
    assert!(
        text.contains(r#"worker_job_failures_total{job="auth.email_verification_requested"} 1"#),
        "{text}"
    );
    assert!(
        text.contains(r#"worker_queue_depth{queue="notification_outbox"} 1"#),
        "{text}"
    );
}
