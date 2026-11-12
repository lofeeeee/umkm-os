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

#[derive(Debug, Clone, PartialEq)]
pub struct AuditRow {
    pub id: uuid::Uuid,
    pub actor_type: String,
    pub actor_user_id: Option<uuid::Uuid>,
    pub action: String,
    pub resource_type: String,
    pub resource_id: Option<String>,
    pub request_id: Option<String>,
    pub ip: Option<String>,
    pub summary: serde_json::Value,
    pub created_at: String,
}

#[derive(Debug, Clone, Default)]
pub struct AuditFilter {
    /// Awalan aksi, mis. `order.` atau `auth.login`
    pub action_prefix: Option<String>,
    pub actor_user_id: Option<uuid::Uuid>,
    pub resource_type: Option<String>,
    pub resource_id: Option<String>,
    /// RFC3339 (di-cast PostgreSQL ke timestamptz)
    pub from: Option<String>,
    pub to: Option<String>,
    pub limit: i64,
    pub offset: i64,
}

/// Audit log tenant (P4-5, `audit.read`). Hanya baris tenant ini (RLS + filter).
pub async fn list(
    pool: &sqlx::PgPool,
    t: umkm_shared::ids::TenantId,
    f: &AuditFilter,
) -> Result<(Vec<AuditRow>, i64), sqlx::Error> {
    use sqlx::Row as _;
    let mut tx = crate::pgx::tenant_tx(pool, t).await?;
    let prefix = f.action_prefix.as_deref().map(|p| {
        format!(
            "{}%",
            p.replace('\\', "\\\\")
                .replace('%', "\\%")
                .replace('_', "\\_")
        )
    });
    let w = "FROM audit_logs WHERE tenant_id = $1 AND ($2::text IS NULL OR action LIKE $2)
        AND ($3::uuid IS NULL OR actor_user_id = $3) AND ($4::text IS NULL OR resource_type = $4)
        AND ($5::text IS NULL OR resource_id = $5)
        AND ($6::text IS NULL OR created_at >= $6::timestamptz) AND ($7::text IS NULL OR created_at < $7::timestamptz)";
    let total: i64 = sqlx::query(sqlx::AssertSqlSafe(format!("SELECT count(*) AS n {w}")))
        .bind(t.as_uuid())
        .bind(&prefix)
        .bind(f.actor_user_id)
        .bind(&f.resource_type)
        .bind(&f.resource_id)
        .bind(&f.from)
        .bind(&f.to)
        .fetch_one(&mut *tx)
        .await?
        .get("n");
    let rows = sqlx::query(sqlx::AssertSqlSafe(format!(
        "SELECT id, actor_type, actor_user_id, action, resource_type, resource_id, request_id, ip, summary,
                to_char(created_at AT TIME ZONE 'UTC', 'YYYY-MM-DD\"T\"HH24:MI:SS.US\"Z\"') AS created_at
         {w} ORDER BY created_at DESC, id DESC LIMIT $8 OFFSET $9"
    )))
    .bind(t.as_uuid())
    .bind(&prefix)
    .bind(f.actor_user_id)
    .bind(&f.resource_type)
    .bind(&f.resource_id)
    .bind(&f.from)
    .bind(&f.to)
    .bind(f.limit)
    .bind(f.offset)
    .fetch_all(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok((
        rows.iter()
            .map(|r| AuditRow {
                id: r.get("id"),
                actor_type: r.get("actor_type"),
                actor_user_id: r.get("actor_user_id"),
                action: r.get("action"),
                resource_type: r.get("resource_type"),
                resource_id: r.get("resource_id"),
                request_id: r.get("request_id"),
                ip: r.get("ip"),
                summary: r.get("summary"),
                created_at: r.get("created_at"),
            })
            .collect(),
        total,
    ))
}
