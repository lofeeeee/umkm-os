//! Penulisan audit log (P1-8). Append-only: hanya INSERT.

use sqlx::PgExecutor;
use umkm_domain::audit::AuditEvent;

/// Tulis event di executor pemanggil. Untuk mutasi bisnis, panggil di transaksi yang
/// sama (dengan tenant context) agar audit dan perubahan atomik.
pub async fn record<'e>(db: impl PgExecutor<'e>, e: &AuditEvent) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO audit_logs (tenant_id, actor_type, actor_user_id, action, resource_type, resource_id, request_id, ip, summary)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)",
    )
    .bind(e.tenant_id.map(|t| *t.as_uuid()))
    .bind(e.actor_type.as_str())
    .bind(e.actor_user_id)
    .bind(e.action)
    .bind(e.resource_type)
    .bind(&e.resource_id)
    .bind(&e.request_id)
    .bind(&e.ip)
    .bind(e.summary())
    .execute(db)
    .await?;
    Ok(())
}

/// Event keamanan di luar transaksi bisnis (login/logout): gagal menulis audit
/// dicatat sebagai error, tidak menggagalkan request.
pub async fn record_best_effort(pool: &sqlx::PgPool, e: &AuditEvent) {
    if let Err(err) = record(pool, e).await {
        tracing::error!(error = %err, action = e.action, "audit write failed");
    }
}
