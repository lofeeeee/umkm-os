//! Resolusi host -> tenant (P1-4) via fungsi `app.resolve_host` (SECURITY DEFINER).

use sqlx::{PgExecutor, Row};
use umkm_shared::ids::TenantId;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HostRecord {
    pub tenant_id: TenantId,
    pub domain_status: String,
    pub tenant_status: String,
}

impl HostRecord {
    /// TEN-009 + BR-011: hanya domain `active` milik tenant `active` yang dilayani.
    pub fn servable(&self) -> bool {
        self.domain_status == "active" && self.tenant_status == "active"
    }
}

/// `hostname` harus sudah dinormalisasi (`umkm_domain::tenant::normalize_host`).
pub async fn resolve_host<'e>(
    db: impl PgExecutor<'e>,
    hostname: &str,
) -> Result<Option<HostRecord>, sqlx::Error> {
    let row =
        sqlx::query("SELECT tenant_id, domain_status, tenant_status FROM app.resolve_host($1)")
            .bind(hostname)
            .fetch_optional(db)
            .await?;
    Ok(row.map(|r| HostRecord {
        tenant_id: TenantId::from_uuid(r.get("tenant_id")),
        domain_status: r.get("domain_status"),
        tenant_status: r.get("tenant_status"),
    }))
}
