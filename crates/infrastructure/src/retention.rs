//! Purge retensi (P1-9, ADR-0012). Memanggil fungsi `app.purge_*` (SECURITY DEFINER).
//! Idempoten: aman dijalankan berulang; putaran kedua tanpa data kedaluwarsa = 0.

use std::time::Duration;

use sqlx::{PgPool, Row};
use umkm_shared::config::RetentionSettings;

use crate::metrics::{JobLabels, Metrics};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RetentionPolicy {
    pub audit: Duration,
    pub closed_tenant_grace: Duration,
    pub auth_artifacts: Duration,
    pub outbox: Duration,
    /// Percakapan: sejak pesan terakhir (ADR-0012, P6-1).
    pub conversations: Duration,
}

const DAY: u64 = 86_400;

impl From<&RetentionSettings> for RetentionPolicy {
    fn from(r: &RetentionSettings) -> Self {
        Self {
            audit: Duration::from_secs(u64::from(r.audit_days) * DAY),
            closed_tenant_grace: Duration::from_secs(u64::from(r.closed_tenant_grace_days) * DAY),
            auth_artifacts: Duration::from_secs(u64::from(r.auth_artifact_days) * DAY),
            outbox: Duration::from_secs(u64::from(r.outbox_days) * DAY),
            conversations: Duration::from_secs(u64::from(r.conversation_days) * DAY),
        }
    }
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct PurgeReport {
    pub audit_rows: i64,
    pub tenants: i64,
    pub sessions: i64,
    pub tokens: i64,
    pub outbox_events: i64,
    /// Customer yang diminta hapus, dianonimkan setelah grace (ADR-0012, AC-CRM-006).
    pub customers: i64,
    pub conversations: i64,
}

pub async fn run_purge(pool: &PgPool, p: &RetentionPolicy) -> Result<PurgeReport, sqlx::Error> {
    let secs = |d: Duration| d.as_secs_f64();
    let audit_rows: i64 =
        sqlx::query_scalar("SELECT app.purge_audit(now() - make_interval(secs => $1))")
            .bind(secs(p.audit))
            .fetch_one(pool)
            .await?;
    let tenants: i64 =
        sqlx::query_scalar("SELECT app.purge_closed_tenants(make_interval(secs => $1))")
            .bind(secs(p.closed_tenant_grace))
            .fetch_one(pool)
            .await?;
    let row = sqlx::query(
        "SELECT sessions, tokens FROM app.purge_auth_artifacts(now() - make_interval(secs => $1))",
    )
    .bind(secs(p.auth_artifacts))
    .fetch_one(pool)
    .await?;
    let outbox_events: i64 =
        sqlx::query_scalar("SELECT app.purge_outbox(now() - make_interval(secs => $1))")
            .bind(secs(p.outbox))
            .fetch_one(pool)
            .await?;
    // Grace sama dengan data pelanggan saat tenant tutup: +30 hari (ADR-0012).
    let customers = crate::crm_repo::purge(pool, p.closed_tenant_grace.as_secs() as i64).await?;
    let conversations = crate::conversation_repo::purge(pool, p.conversations).await?;
    Ok(PurgeReport {
        conversations,
        customers,
        audit_rows,
        tenants,
        sessions: row.get("sessions"),
        tokens: row.get("tokens"),
        outbox_events,
    })
}

/// Satu putaran job retensi dengan metrics + log ringkasan (tanpa data pribadi).
pub async fn purge_job(pool: &PgPool, p: &RetentionPolicy, metrics: &Metrics) {
    let job = JobLabels {
        job: "retention.purge".into(),
    };
    metrics.worker_jobs.get_or_create(&job).inc();
    match run_purge(pool, p).await {
        Ok(r) => tracing::info!(
            audit_rows = r.audit_rows,
            tenants = r.tenants,
            sessions = r.sessions,
            tokens = r.tokens,
            outbox_events = r.outbox_events,
            customers = r.customers,
            conversations = r.conversations,
            "retention purge"
        ),
        Err(e) => {
            metrics.worker_job_failures.get_or_create(&job).inc();
            tracing::error!(error = %e, "retention purge failed");
        }
    }
}
