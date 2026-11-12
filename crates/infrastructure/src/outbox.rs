//! Transactional outbox + pemroses worker (P1-6, `PRD.md §4.13`, BR-017).
//!
//! - `enqueue` dipanggil di transaksi bisnis; kunci idempotensi ganda diabaikan.
//! - `claim` memakai `FOR UPDATE SKIP LOCKED` + lease (`locked_until`); lease kedaluwarsa
//!   (worker crash) dapat diklaim ulang.
//! - Gagal retryable -> `pending` dengan backoff eksponensial; melewati batas -> `dead`.
//! - `last_error` hanya kelas error (tanpa email/token).

use std::{sync::Mutex, time::Duration};

use sqlx::{PgExecutor, PgPool, Row};
use umkm_application::notify::{
    EmailMessage, EmailSender, OutboxEvent, SendError, events, verification_email,
};
use umkm_domain::auth::AuthPolicy;
use uuid::Uuid;

use crate::{
    auth::new_token,
    auth_repo::{self, TokenPurpose},
    metrics::{JobLabels, Metrics, QueueLabels},
};

/// Tulis event di transaksi pemanggil. `true` bila baris baru dibuat.
pub async fn enqueue<'e>(db: impl PgExecutor<'e>, ev: &OutboxEvent) -> Result<bool, sqlx::Error> {
    let n = sqlx::query(
        "INSERT INTO notification_outbox (tenant_id, event_type, payload, idempotency_key)
         VALUES ($1, $2, $3, $4) ON CONFLICT (idempotency_key) DO NOTHING",
    )
    .bind(ev.tenant_id)
    .bind(ev.event_type)
    .bind(&ev.payload)
    .bind(&ev.idempotency_key)
    .execute(db)
    .await?
    .rows_affected();
    Ok(n == 1)
}

#[derive(Debug, Clone)]
pub struct Claimed {
    pub id: Uuid,
    pub tenant_id: Option<Uuid>,
    pub event_type: String,
    pub payload: serde_json::Value,
    pub attempts: i32,
}

/// Klaim hingga `limit` event siap kerja (pending & jatuh tempo, atau lease kedaluwarsa).
pub async fn claim(
    pool: &PgPool,
    worker: &str,
    limit: i64,
    lease: Duration,
) -> Result<Vec<Claimed>, sqlx::Error> {
    let rows = sqlx::query(
        "UPDATE notification_outbox o
         SET status = 'processing', locked_until = now() + make_interval(secs => $3),
             locked_by = $2, attempts = o.attempts + 1
         WHERE o.id IN (
             SELECT id FROM notification_outbox
             WHERE (status = 'pending' AND available_at <= now())
                OR (status = 'processing' AND locked_until < now())
             ORDER BY available_at
             LIMIT $1
             FOR UPDATE SKIP LOCKED)
         RETURNING o.id, o.tenant_id, o.event_type, o.payload, o.attempts",
    )
    .bind(limit)
    .bind(worker)
    .bind(lease.as_secs_f64())
    .fetch_all(pool)
    .await?;
    Ok(rows
        .into_iter()
        .map(|r| Claimed {
            id: r.get("id"),
            tenant_id: r.get("tenant_id"),
            event_type: r.get("event_type"),
            payload: r.get("payload"),
            attempts: r.get("attempts"),
        })
        .collect())
}

/// Hanya pemegang lease yang boleh menyelesaikan event (cegah worker lama menimpa).
pub async fn mark_sent(pool: &PgPool, id: Uuid, worker: &str) -> Result<bool, sqlx::Error> {
    let n = sqlx::query(
        "UPDATE notification_outbox SET status = 'sent', processed_at = now(), locked_until = NULL, locked_by = NULL, last_error = NULL
         WHERE id = $1 AND status = 'processing' AND locked_by = $2",
    )
    .bind(id)
    .bind(worker)
    .execute(pool)
    .await?
    .rows_affected();
    Ok(n == 1)
}

pub async fn mark_failed(
    pool: &PgPool,
    id: Uuid,
    worker: &str,
    class: &str,
    retry_in: Option<Duration>,
) -> Result<(), sqlx::Error> {
    let class: String = class.chars().take(200).collect();
    match retry_in {
        Some(d) => sqlx::query(
            "UPDATE notification_outbox SET status = 'pending', available_at = now() + make_interval(secs => $3),
                 locked_until = NULL, locked_by = NULL, last_error = $4
             WHERE id = $1 AND status = 'processing' AND locked_by = $2",
        )
        .bind(id)
        .bind(worker)
        .bind(d.as_secs_f64())
        .bind(&class),
        None => sqlx::query(
            "UPDATE notification_outbox SET status = 'dead', processed_at = now(), locked_until = NULL, locked_by = NULL, last_error = $3
             WHERE id = $1 AND status = 'processing' AND locked_by = $2",
        )
        .bind(id)
        .bind(worker)
        .bind(&class),
    }
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn queue_depth(pool: &PgPool) -> Result<i64, sqlx::Error> {
    sqlx::query_scalar(
        "SELECT count(*) FROM notification_outbox WHERE status IN ('pending', 'processing')",
    )
    .fetch_one(pool)
    .await
}

#[derive(Debug, Clone)]
pub struct WorkerConfig {
    pub worker_id: String,
    pub batch: i64,
    pub lease: Duration,
    pub max_attempts: i32,
    pub base_backoff: Duration,
    pub verify_url_base: String,
    pub policy: AuthPolicy,
}

impl WorkerConfig {
    /// Backoff eksponensial: base * 2^(attempt-1), maks 1 jam.
    pub fn backoff(&self, attempt: i32) -> Duration {
        let exp = u32::try_from(attempt.saturating_sub(1))
            .unwrap_or(0)
            .min(16);
        self.base_backoff
            .saturating_mul(1 << exp)
            .min(Duration::from_secs(3600))
    }
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct Tick {
    pub claimed: usize,
    pub sent: usize,
    pub retried: usize,
    pub dead: usize,
}

enum Outcome {
    Done,
    Fail(&'static str, bool),
}

/// Satu putaran worker: klaim, proses, tandai. Dipanggil berulang oleh `apps/worker`.
pub async fn process_once(
    pool: &PgPool,
    sender: &impl EmailSender,
    cfg: &WorkerConfig,
    metrics: &Metrics,
) -> Result<Tick, sqlx::Error> {
    let batch = claim(pool, &cfg.worker_id, cfg.batch, cfg.lease).await?;
    let mut tick = Tick {
        claimed: batch.len(),
        ..Tick::default()
    };
    for ev in batch {
        let job = JobLabels {
            job: ev.event_type.clone(),
        };
        metrics.worker_jobs.get_or_create(&job).inc();
        let outcome = match ev.event_type.as_str() {
            events::EMAIL_VERIFICATION_REQUESTED => {
                handle_email_verification(pool, sender, cfg, &ev).await
            }
            events::TENANT_GUARD_PROBE => handle_tenant_guard_probe(pool, &ev).await,
            crate::team_repo::INVITATION_REQUESTED => {
                handle_invitation(pool, sender, cfg, &ev).await
            }
            events::CUSTOMER_EMAIL_VERIFICATION_REQUESTED => {
                handle_customer_verification(pool, sender, cfg, &ev).await
            }
            // Kanal notifikasi staf (in-app/email) menyusul (NTF, D-08); sampai itu dicatat.
            events::INVENTORY_LOW_STOCK
            | events::ORDER_CREATED
            | events::ORDER_STATUS_CHANGED
            | events::PAYMENT_CLAIMED => {
                tracing::info!(event_id = %ev.id, tenant_id = ?ev.tenant_id, "low-stock event (log-only)");
                Outcome::Done
            }
            _ => Outcome::Fail("unknown_event_type", false),
        };
        match outcome {
            Outcome::Done => {
                mark_sent(pool, ev.id, &cfg.worker_id).await?;
                tick.sent += 1;
            }
            Outcome::Fail(class, retryable) => {
                metrics.worker_job_failures.get_or_create(&job).inc();
                if retryable && ev.attempts < cfg.max_attempts {
                    mark_failed(
                        pool,
                        ev.id,
                        &cfg.worker_id,
                        class,
                        Some(cfg.backoff(ev.attempts)),
                    )
                    .await?;
                    tick.retried += 1;
                } else {
                    tracing::error!(event_id = %ev.id, event_type = %ev.event_type, class, attempts = ev.attempts, "outbox event dead");
                    mark_failed(pool, ev.id, &cfg.worker_id, class, None).await?;
                    tick.dead += 1;
                }
            }
        }
    }
    // Hitung dulu: guard `get_or_create` tidak boleh ditahan melewati `.await` (bukan Send).
    let depth = queue_depth(pool).await?;
    metrics
        .worker_queue_depth
        .get_or_create(&QueueLabels {
            queue: "notification_outbox".into(),
        })
        .set(depth);
    Ok(tick)
}

async fn handle_email_verification(
    pool: &PgPool,
    sender: &impl EmailSender,
    cfg: &WorkerConfig,
    ev: &Claimed,
) -> Outcome {
    let Some(user_id) = ev
        .payload
        .get("user_id")
        .and_then(|v| v.as_str())
        .and_then(|s| s.parse::<Uuid>().ok())
    else {
        return Outcome::Fail("invalid_payload", false);
    };
    let row = match sqlx::query(
        "SELECT email, email_verified_at IS NOT NULL AS verified, status FROM users WHERE id = $1",
    )
    .bind(user_id)
    .fetch_optional(pool)
    .await
    {
        Ok(r) => r,
        Err(_) => return Outcome::Fail("db_error", true),
    };
    let Some(row) = row else { return Outcome::Done }; // user dihapus: tidak ada yang dikirim
    if row.get::<bool, _>("verified") || row.get::<String, _>("status") != "active" {
        return Outcome::Done;
    }
    let email: String = row.get("email");

    // Token dibuat SAAT pengiriman; hanya hash yang disimpan. Token lama dibatalkan.
    let Ok((token, hash)) = new_token() else {
        return Outcome::Fail("rng_error", true);
    };
    let stored = async {
        let mut tx = pool.begin().await?;
        auth_repo::invalidate_user_tokens(&mut *tx, user_id, TokenPurpose::EmailVerification)
            .await?;
        auth_repo::create_user_token(
            &mut *tx,
            user_id,
            TokenPurpose::EmailVerification,
            hash,
            cfg.policy.email_verification_ttl,
        )
        .await?;
        tx.commit().await.map_err(auth_repo::RepoError::from)
    }
    .await;
    if stored.is_err() {
        return Outcome::Fail("db_error", true);
    }
    match sender
        .send(&verification_email(&email, &cfg.verify_url_base, &token))
        .await
    {
        Ok(()) => Outcome::Done,
        Err(SendError { retryable, class }) => Outcome::Fail(class, retryable),
    }
}

/// Undangan tim (AUTH-009): token dibuat saat kirim; lama tidak berlaku lagi.
async fn handle_invitation(
    pool: &PgPool,
    sender: &impl EmailSender,
    cfg: &WorkerConfig,
    ev: &Claimed,
) -> Outcome {
    let (Some(tenant), Some(id)) = (
        ev.tenant_id.map(umkm_shared::ids::TenantId::from_uuid),
        ev.payload
            .get("invitation_id")
            .and_then(|v| v.as_str())
            .and_then(|s| s.parse::<Uuid>().ok()),
    ) else {
        return Outcome::Fail("invalid_payload", false);
    };
    let Ok((token, hash)) = new_token() else {
        return Outcome::Fail("rng_error", true);
    };
    let target = match crate::team_repo::issue_invitation_token(pool, tenant, id, hash).await {
        Ok(t) => t,
        Err(_) => return Outcome::Fail("db_error", true),
    };
    let Some((email, shop)) = target else {
        return Outcome::Done; // dicabut / kedaluwarsa / diterima
    };
    // verify_url_base = https://app.<root>/verify-email → origin dashboard.
    let app_base = cfg
        .verify_url_base
        .rsplit_once('/')
        .map(|(b, _)| b)
        .unwrap_or(&cfg.verify_url_base);
    match sender
        .send(&umkm_application::notify::invitation_email(
            &email, app_base, &shop, &token,
        ))
        .await
    {
        Ok(()) => Outcome::Done,
        Err(SendError { retryable, class }) => Outcome::Fail(class, retryable),
    }
}

/// Verifikasi akun customer (ADR-0005): token dibuat di tenant context event; tautan ke host toko.
async fn handle_customer_verification(
    pool: &PgPool,
    sender: &impl EmailSender,
    cfg: &WorkerConfig,
    ev: &Claimed,
) -> Outcome {
    let (Some(tenant), Some(account)) = (
        ev.tenant_id.map(umkm_shared::ids::TenantId::from_uuid),
        ev.payload
            .get("account_id")
            .and_then(|v| v.as_str())
            .and_then(|s| s.parse::<Uuid>().ok()),
    ) else {
        return Outcome::Fail("invalid_payload", false);
    };
    let Ok((token, hash)) = new_token() else {
        return Outcome::Fail("rng_error", true);
    };
    let target = match crate::customer_auth_repo::issue_verification_token(
        pool,
        tenant,
        account,
        hash,
        cfg.policy.email_verification_ttl,
    )
    .await
    {
        Ok(t) => t,
        Err(_) => return Outcome::Fail("db_error", true),
    };
    let Some((email, host)) = target else {
        return Outcome::Done; // sudah terverifikasi / akun dihapus
    };
    match sender
        .send(&umkm_application::notify::customer_verification_email(
            &email, &host, &token,
        ))
        .await
    {
        Ok(()) => Outcome::Done,
        Err(SendError { retryable, class }) => Outcome::Fail(class, retryable),
    }
}

/// Job tenant-scoped berjalan HANYA di tenant context event (`PRD.md §13.2` #5).
/// Membaca resource milik `resource_tenant_id`; bila berbeda dari tenant event, RLS +
/// filter eksplisit membuatnya tidak terlihat dan job gagal permanen (tidak diproses).
async fn handle_tenant_guard_probe(pool: &PgPool, ev: &Claimed) -> Outcome {
    let Some(tenant) = ev.tenant_id.map(umkm_shared::ids::TenantId::from_uuid) else {
        return Outcome::Fail("missing_tenant", false);
    };
    let Some(resource_tenant) = ev
        .payload
        .get("resource_tenant_id")
        .and_then(|v| v.as_str())
        .and_then(|s| s.parse::<Uuid>().ok())
    else {
        return Outcome::Fail("invalid_payload", false);
    };
    let run = async {
        let mut tx = umkm_database::begin_tenant_tx(pool, tenant)
            .await
            .map_err(|_| "db_error")?;
        let name: Option<String> = sqlx::query_scalar(
            "SELECT name FROM business_profiles WHERE tenant_id = $1 AND tenant_id = app.current_tenant_id()",
        )
        .bind(resource_tenant)
        .fetch_optional(&mut *tx)
        .await
        .map_err(|_| "db_error")?;
        let Some(name) = name else {
            return Err("resource_not_in_tenant");
        };
        crate::audit_repo::record(
            &mut *tx,
            &umkm_domain::audit::AuditEvent::new(
                umkm_domain::audit::actions::TENANT_GUARD_PROBE_PROCESSED,
                "outbox_event",
            )
            .in_tenant(tenant)
            .resource(ev.id)
            .with_summary(serde_json::json!({ "profile_name": name })),
        )
        .await
        .map_err(|_| "db_error")?;
        tx.commit().await.map_err(|_| "db_error")
    };
    match run.await {
        Ok(()) => Outcome::Done,
        Err("db_error") => Outcome::Fail("db_error", true),
        Err(class) => Outcome::Fail(class, false),
    }
}

/// Konsumsi token verifikasi (AUTH-003). `Some(user_id)` bila token valid dan email kini terverifikasi.
pub async fn verify_email(
    pool: &PgPool,
    token: umkm_domain::auth::TokenHash,
) -> Result<Option<Uuid>, sqlx::Error> {
    let mut tx = pool.begin().await?;
    let user = auth_repo::consume_user_token(&mut *tx, TokenPurpose::EmailVerification, token)
        .await
        .map_err(|e| match e {
            auth_repo::RepoError::Db(e) => e,
            auth_repo::RepoError::EmailTaken => sqlx::Error::Protocol("unexpected".into()),
        })?;
    if let Some(u) = user {
        sqlx::query(
            "UPDATE users SET email_verified_at = COALESCE(email_verified_at, now()) WHERE id = $1",
        )
        .bind(u)
        .execute(&mut *tx)
        .await?;
    }
    tx.commit().await?;
    Ok(user)
}

/// Adapter produksi sementara (D-08 ditunda): TIDAK mengirim, hanya mencatat template
/// dan domain penerima. Tidak pernah mencatat token, isi, atau alamat lengkap.
#[derive(Debug, Default, Clone, Copy)]
pub struct LogEmailSender;

impl EmailSender for LogEmailSender {
    async fn send(&self, msg: &EmailMessage) -> Result<(), SendError> {
        let domain = msg.to.rsplit_once('@').map(|(_, d)| d).unwrap_or("?");
        tracing::warn!(
            template = msg.template,
            recipient_domain = domain,
            "email NOT delivered: provider pending D-08 (log-only adapter)"
        );
        Ok(())
    }
}

/// Adapter uji: menyimpan pesan di memori, opsional gagal N kali pertama.
#[derive(Debug, Default)]
pub struct CapturingEmailSender {
    pub sent: Mutex<Vec<EmailMessage>>,
    pub fail_next: Mutex<Vec<SendError>>,
}

impl EmailSender for CapturingEmailSender {
    async fn send(&self, msg: &EmailMessage) -> Result<(), SendError> {
        if let Some(e) = self.fail_next.lock().expect("lock").pop() {
            return Err(e);
        }
        self.sent.lock().expect("lock").push(msg.clone());
        Ok(())
    }
}
