//! Memuat `Actor` dan data tenant untuk dashboard (P1-5).
//! Query tenant-owned dijalankan di transaksi dengan tenant context (siap RLS P1-7),
//! dan tetap menyertakan `tenant_id` di WHERE (defense-in-depth, `PRD.md §5.7`).

use sqlx::{PgPool, Row};
use umkm_domain::rbac::Actor;
use umkm_shared::ids::TenantId;
use uuid::Uuid;

/// Actor hanya bila membership `active` DAN tenant `active` (BR-011). Selain itu `None`.
pub async fn load_actor(
    pool: &PgPool,
    user_id: Uuid,
    tenant_id: TenantId,
) -> Result<Option<Actor>, sqlx::Error> {
    let mut tx = umkm_database::begin_tenant_tx(pool, tenant_id)
        .await
        .map_err(db_err)?;
    let rows = sqlx::query(
        "SELECT r.name AS role_name, p.permission_key
         FROM tenant_memberships m
         JOIN tenants t ON t.id = m.tenant_id
         JOIN roles r ON r.id = m.role_id
         LEFT JOIN role_permissions rp ON rp.role_id = r.id
         LEFT JOIN permissions p ON p.id = rp.permission_id
         WHERE m.tenant_id = $1 AND m.user_id = $2 AND m.status = 'active' AND t.status = 'active'",
    )
    .bind(tenant_id.as_uuid())
    .bind(user_id)
    .fetch_all(&mut *tx)
    .await?;
    tx.commit().await?;
    let Some(first) = rows.first() else {
        return Ok(None);
    };
    let role_name: String = first.get("role_name");
    let perms = rows
        .iter()
        .filter_map(|r| r.get::<Option<String>, _>("permission_key"));
    Ok(Some(Actor::new(user_id, tenant_id, role_name, perms)))
}

fn db_err(e: umkm_database::DbError) -> sqlx::Error {
    match e {
        umkm_database::DbError::Sqlx(e) => e,
        umkm_database::DbError::InvalidUrl => sqlx::Error::Configuration("invalid url".into()),
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MembershipSummary {
    pub tenant_id: Uuid,
    pub name: String,
    pub slug: String,
    pub role_name: String,
}

/// Tenant tempat user menjadi anggota aktif (AUTH-008). Lintas tenant, sehingga lewat
/// fungsi SECURITY DEFINER `app.user_memberships` (RLS P1-7) yang dibatasi `user_id` sesi.
pub async fn list_memberships(
    pool: &PgPool,
    user_id: Uuid,
) -> Result<Vec<MembershipSummary>, sqlx::Error> {
    let rows = sqlx::query("SELECT tenant_id, name, slug, role_name FROM app.user_memberships($1)")
        .bind(user_id)
        .fetch_all(pool)
        .await?;
    Ok(rows
        .into_iter()
        .map(|r| MembershipSummary {
            tenant_id: r.get("tenant_id"),
            name: r.get("name"),
            slug: r.get("slug"),
            role_name: r.get("role_name"),
        })
        .collect())
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TenantDetail {
    pub id: Uuid,
    pub name: String,
    pub slug: String,
    pub status: String,
    pub primary_hostname: Option<String>,
}

pub async fn get_tenant(pool: &PgPool, actor: &Actor) -> Result<Option<TenantDetail>, sqlx::Error> {
    let mut tx = umkm_database::begin_tenant_tx(pool, actor.tenant_id)
        .await
        .map_err(db_err)?;
    let row = sqlx::query(
        "SELECT t.id, t.name, t.slug, t.status,
                (SELECT d.hostname FROM tenant_domains d
                 WHERE d.tenant_id = t.id AND d.type = 'subdomain' ORDER BY d.created_at LIMIT 1) AS host
         FROM tenants t WHERE t.id = $1",
    )
    .bind(actor.tenant_id.as_uuid())
    .fetch_optional(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(row.map(|r| TenantDetail {
        id: r.get("id"),
        name: r.get("name"),
        slug: r.get("slug"),
        status: r.get("status"),
        primary_hostname: r.get("host"),
    }))
}

/// Ubah nama tenant (butuh `tenant.update`; dicek pemanggil) + audit before/after di
/// transaksi yang sama (P1-8). Nama business profile dikelola terpisah (P2-5).
pub async fn rename_tenant(
    pool: &PgPool,
    actor: &Actor,
    name: &str,
    request_id: Option<&str>,
    ip: Option<&str>,
) -> Result<bool, sqlx::Error> {
    let mut tx = umkm_database::begin_tenant_tx(pool, actor.tenant_id)
        .await
        .map_err(db_err)?;
    let before: Option<String> =
        sqlx::query_scalar("SELECT name FROM tenants WHERE id = $1 FOR UPDATE")
            .bind(actor.tenant_id.as_uuid())
            .fetch_optional(&mut *tx)
            .await?;
    let Some(before) = before else {
        return Ok(false);
    };
    sqlx::query("UPDATE tenants SET name = $2 WHERE id = $1")
        .bind(actor.tenant_id.as_uuid())
        .bind(name)
        .execute(&mut *tx)
        .await?;
    crate::audit_repo::record(
        &mut *tx,
        &umkm_domain::audit::AuditEvent::new(umkm_domain::audit::actions::TENANT_UPDATED, "tenant")
            .by_user(actor.user_id)
            .in_tenant(actor.tenant_id)
            .resource(actor.tenant_id)
            .request(request_id, ip)
            .with_summary(
                serde_json::json!({ "before": { "name": before }, "after": { "name": name } }),
            ),
    )
    .await?;
    tx.commit().await?;
    Ok(true)
}
