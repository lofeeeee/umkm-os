//! Pemeriksaan role database aplikasi (P1-7, `PRD.md §5.7`): role aplikasi tidak boleh
//! superuser dan tidak boleh `BYPASSRLS`, karena keduanya melewati RLS.

use sqlx::{PgPool, Row};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RoleInfo {
    pub name: String,
    pub superuser: bool,
    pub bypass_rls: bool,
}

impl RoleInfo {
    pub fn rls_enforced(&self) -> bool {
        !self.superuser && !self.bypass_rls
    }
}

/// Atribut efektif `current_user` (role sesi, mis. setelah `SET ROLE`).
pub async fn current_role(pool: &PgPool) -> Result<RoleInfo, sqlx::Error> {
    let r = sqlx::query(
        "SELECT rolname, rolsuper, rolbypassrls FROM pg_roles WHERE rolname = current_user",
    )
    .fetch_one(pool)
    .await?;
    Ok(RoleInfo {
        name: r.get("rolname"),
        superuser: r.get("rolsuper"),
        bypass_rls: r.get("rolbypassrls"),
    })
}

/// Guard startup: di luar development, tolak berjalan bila role DB melewati RLS.
/// `allow_bypass` = true hanya untuk environment development (log peringatan).
pub async fn ensure_rls_enforced(pool: &PgPool, allow_bypass: bool) -> Result<RoleInfo, String> {
    let info = current_role(pool)
        .await
        .map_err(|e| format!("tidak dapat memverifikasi role database: {e}"))?;
    if info.rls_enforced() {
        return Ok(info);
    }
    let msg = format!(
        "role database '{}' melewati RLS (superuser={}, bypassrls={}); gunakan role aplikasi (P1-7)",
        info.name, info.superuser, info.bypass_rls
    );
    if allow_bypass {
        tracing::warn!("{msg} — diizinkan hanya di development");
        Ok(info)
    } else {
        Err(msg)
    }
}
