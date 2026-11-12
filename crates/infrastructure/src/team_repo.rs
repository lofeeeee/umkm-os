//! Tim, role kustom, undangan (P4-1, `PRD.md §3`, AUTH-009/010). Tenant context (RLS).
//! Perubahan keanggotaan diserialkan per tenant (advisory lock) agar aturan "minimal satu Owner
//! aktif" tidak bisa dilanggar dua request paralel.

use std::collections::BTreeSet;

use sqlx::{PgPool, Row};
use umkm_application::notify::OutboxEvent;
use umkm_domain::{
    audit::AuditEvent,
    auth::TokenHash,
    rbac::Actor,
    team::{INVITATION_TTL_SECS, OWNER, TeamError, check_grant, check_owner_remains},
};
use umkm_shared::ids::TenantId;
use uuid::Uuid;

use crate::pgx::{Tx, tenant_tx, violation};

pub type Ctx<'a> = (Option<&'a str>, Option<&'a str>);

pub const INVITATION_REQUESTED: &str = "team.invitation_requested";

#[derive(Debug, thiserror::Error)]
pub enum TeamRepoError {
    #[error("tidak ditemukan")]
    NotFound,
    #[error(transparent)]
    Rule(#[from] TeamError),
    #[error("role sistem tidak dapat diubah")]
    SystemRole,
    #[error("role masih dipakai anggota/undangan")]
    RoleInUse,
    #[error("nama role sudah dipakai")]
    RoleNameTaken,
    #[error("sudah menjadi anggota")]
    AlreadyMember,
    #[error("email akun tidak sesuai undangan")]
    EmailMismatch,
    #[error("email sudah punya akun: masuk dulu lalu terima undangan")]
    AccountExists,
    #[error(transparent)]
    Db(#[from] sqlx::Error),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Member {
    pub membership_id: Uuid,
    pub user_id: Uuid,
    pub email: String,
    pub role_id: Uuid,
    pub role_name: String,
    pub status: String,
    pub created_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Role {
    pub id: Uuid,
    pub name: String,
    pub is_system: bool,
    pub permissions: Vec<String>,
    pub members: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Invitation {
    pub id: Uuid,
    pub email: String,
    pub role_id: Uuid,
    pub role_name: String,
    pub expires_at: String,
    pub created_at: String,
}

const TS: &str = "'YYYY-MM-DD\"T\"HH24:MI:SS.US\"Z\"'";

#[allow(clippy::too_many_arguments)]
async fn audit(
    tx: &mut Tx,
    actor: Option<&Actor>,
    t: TenantId,
    action: &'static str,
    kind: &'static str,
    id: Uuid,
    summary: serde_json::Value,
    (request_id, ip): Ctx<'_>,
) -> Result<(), sqlx::Error> {
    let mut e = AuditEvent::new(action, kind)
        .in_tenant(t)
        .resource(id)
        .request(request_id, ip)
        .with_summary(summary);
    if let Some(a) = actor {
        e = e.by_user(a.user_id);
    }
    crate::audit_repo::record(&mut **tx, &e).await
}

async fn lock_team(tx: &mut Tx, t: TenantId) -> Result<(), sqlx::Error> {
    sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended('team:' || $1::text, 0))")
        .bind(t.as_uuid())
        .execute(&mut **tx)
        .await?;
    Ok(())
}

/// Permission sebuah role (sistem atau kustom tenant ini). `None` bila role tidak terlihat.
async fn role_perms(
    tx: &mut Tx,
    role: Uuid,
) -> Result<Option<(String, BTreeSet<String>)>, sqlx::Error> {
    let rows = sqlx::query(
        "SELECT r.name, p.permission_key FROM roles r
         LEFT JOIN role_permissions rp ON rp.role_id = r.id
         LEFT JOIN permissions p ON p.id = rp.permission_id
         WHERE r.id = $1",
    )
    .bind(role)
    .fetch_all(&mut **tx)
    .await?;
    let Some(first) = rows.first() else {
        return Ok(None);
    };
    Ok(Some((
        first.get("name"),
        rows.iter()
            .filter_map(|r| r.get::<Option<String>, _>("permission_key"))
            .collect(),
    )))
}

fn actor_set(actor: &Actor) -> BTreeSet<&str> {
    actor.permissions().collect()
}

fn grant_ok(actor: &Actor, perms: &BTreeSet<String>) -> Result<(), TeamError> {
    check_grant(&actor_set(actor), perms.iter().map(String::as_str))
}

// ------------------------------------------------------------------ anggota

pub async fn list_members(pool: &PgPool, t: TenantId) -> Result<Vec<Member>, sqlx::Error> {
    let mut tx = tenant_tx(pool, t).await?;
    let rows = sqlx::query(sqlx::AssertSqlSafe(format!(
        "SELECT m.id, m.user_id, u.email, m.role_id, r.name AS role_name, m.status,
                to_char(m.created_at AT TIME ZONE 'UTC', {TS}) AS created_at
         FROM tenant_memberships m JOIN users u ON u.id = m.user_id JOIN roles r ON r.id = m.role_id
         WHERE m.tenant_id = $1 AND m.status <> 'removed' ORDER BY m.created_at, m.id"
    )))
    .bind(t.as_uuid())
    .fetch_all(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(rows
        .iter()
        .map(|r| Member {
            membership_id: r.get("id"),
            user_id: r.get("user_id"),
            email: r.get("email"),
            role_id: r.get("role_id"),
            role_name: r.get("role_name"),
            status: r.get("status"),
            created_at: r.get("created_at"),
        })
        .collect())
}

async fn active_owners(tx: &mut Tx, t: TenantId) -> Result<usize, sqlx::Error> {
    let n: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM tenant_memberships m JOIN roles r ON r.id = m.role_id
         WHERE m.tenant_id = $1 AND m.status = 'active' AND r.tenant_id IS NULL AND r.name = $2",
    )
    .bind(t.as_uuid())
    .bind(OWNER)
    .fetch_one(&mut **tx)
    .await?;
    Ok(usize::try_from(n).unwrap_or(0))
}

/// Ubah role dan/atau status anggota (`members.manage`). Aturan: bukan diri sendiri; pelaku
/// harus memiliki semua permission role lama DAN role baru target (anti-eskalasi); minimal satu
/// Owner aktif tersisa.
pub async fn update_member(
    pool: &PgPool,
    actor: &Actor,
    membership: Uuid,
    role: Option<Uuid>,
    status: Option<&str>,
    ctx: Ctx<'_>,
) -> Result<(), TeamRepoError> {
    let t = actor.tenant_id;
    let mut tx = tenant_tx(pool, t).await?;
    lock_team(&mut tx, t).await?;
    let r = sqlx::query(
        "SELECT m.user_id, m.role_id, m.status, (r.tenant_id IS NULL AND r.name = $3) AS is_owner
         FROM tenant_memberships m JOIN roles r ON r.id = m.role_id
         WHERE m.tenant_id = $1 AND m.id = $2 AND m.status <> 'removed' FOR UPDATE OF m",
    )
    .bind(t.as_uuid())
    .bind(membership)
    .bind(OWNER)
    .fetch_optional(&mut *tx)
    .await?
    .ok_or(TeamRepoError::NotFound)?;
    let (user, old_role, old_status, is_owner): (Uuid, Uuid, String, bool) = (
        r.get("user_id"),
        r.get("role_id"),
        r.get("status"),
        r.get("is_owner"),
    );
    if user == actor.user_id {
        return Err(TeamError::SelfChange.into());
    }
    let (old_name, old_perms) = role_perms(&mut tx, old_role)
        .await?
        .ok_or(TeamRepoError::NotFound)?;
    grant_ok(actor, &old_perms)?;
    let new_role = role.unwrap_or(old_role);
    let (new_name, new_perms) = role_perms(&mut tx, new_role)
        .await?
        .ok_or(TeamRepoError::NotFound)?;
    grant_ok(actor, &new_perms)?;
    let new_status = status.unwrap_or(&old_status);
    let stays_owner = new_status == "active" && new_role == old_role;
    check_owner_remains(
        active_owners(&mut tx, t).await?,
        is_owner && old_status == "active",
        stays_owner,
    )?;
    sqlx::query(
        "UPDATE tenant_memberships SET role_id = $3, status = $4 WHERE tenant_id = $1 AND id = $2",
    )
    .bind(t.as_uuid())
    .bind(membership)
    .bind(new_role)
    .bind(new_status)
    .execute(&mut *tx)
    .await?;
    audit(
        &mut tx,
        Some(actor),
        t,
        "member.updated",
        "membership",
        membership,
        serde_json::json!({
            "user_id": user,
            "before": { "role": old_name, "status": old_status },
            "after": { "role": new_name, "status": new_status },
        }),
        ctx,
    )
    .await?;
    tx.commit().await?;
    Ok(())
}

// ------------------------------------------------------------------ role

pub async fn list_roles(pool: &PgPool, t: TenantId) -> Result<Vec<Role>, sqlx::Error> {
    let mut tx = tenant_tx(pool, t).await?;
    let rows = sqlx::query(
        "SELECT r.id, r.name, r.is_system,
                COALESCE(array_agg(p.permission_key ORDER BY p.permission_key) FILTER (WHERE p.permission_key IS NOT NULL), '{}') AS perms,
                (SELECT count(*) FROM tenant_memberships m WHERE m.tenant_id = $1 AND m.role_id = r.id AND m.status = 'active') AS members
         FROM roles r
         LEFT JOIN role_permissions rp ON rp.role_id = r.id
         LEFT JOIN permissions p ON p.id = rp.permission_id
         WHERE r.tenant_id IS NULL OR r.tenant_id = $1
         GROUP BY r.id ORDER BY r.is_system DESC, r.name",
    )
    .bind(t.as_uuid())
    .fetch_all(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(rows
        .iter()
        .map(|r| Role {
            id: r.get("id"),
            name: r.get("name"),
            is_system: r.get("is_system"),
            permissions: r.get("perms"),
            members: r.get("members"),
        })
        .collect())
}

async fn set_role_perms(
    tx: &mut Tx,
    role: Uuid,
    perms: &BTreeSet<&str>,
) -> Result<(), sqlx::Error> {
    sqlx::query("DELETE FROM role_permissions WHERE role_id = $1")
        .bind(role)
        .execute(&mut **tx)
        .await?;
    let keys: Vec<&str> = perms.iter().copied().collect();
    sqlx::query(
        "INSERT INTO role_permissions (role_id, permission_id)
         SELECT $1, id FROM permissions WHERE permission_key = ANY($2)",
    )
    .bind(role)
    .bind(&keys)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

/// Buat (id `None`) / ubah role kustom. Permission ⊆ milik pelaku.
pub async fn save_role(
    pool: &PgPool,
    actor: &Actor,
    id: Option<Uuid>,
    name: &str,
    perms: &BTreeSet<&'static str>,
    ctx: Ctx<'_>,
) -> Result<Uuid, TeamRepoError> {
    check_grant(&actor_set(actor), perms.iter().copied())?;
    let t = actor.tenant_id;
    let mut tx = tenant_tx(pool, t).await?;
    lock_team(&mut tx, t).await?;
    let before = match id {
        Some(rid) => {
            let r = sqlx::query("SELECT is_system FROM roles WHERE id = $1 AND (tenant_id = $2 OR tenant_id IS NULL)")
                .bind(rid)
                .bind(t.as_uuid())
                .fetch_optional(&mut *tx)
                .await?
                .ok_or(TeamRepoError::NotFound)?;
            if r.get::<bool, _>("is_system") {
                return Err(TeamRepoError::SystemRole);
            }
            let (_, old) = role_perms(&mut tx, rid)
                .await?
                .ok_or(TeamRepoError::NotFound)?;
            // Mengubah role yang sudah lebih kuat dari pelaku = eskalasi atas pemegangnya.
            grant_ok(actor, &old)?;
            Some(old)
        }
        None => None,
    };
    let map = |e: sqlx::Error| match violation(&e) {
        Some((c, _)) if c == "23505" => TeamRepoError::RoleNameTaken,
        _ => TeamRepoError::Db(e),
    };
    let rid: Uuid = match id {
        None => sqlx::query_scalar(
            "INSERT INTO roles (tenant_id, name, is_system) VALUES ($1, $2, false) RETURNING id",
        )
        .bind(t.as_uuid())
        .bind(name)
        .fetch_one(&mut *tx)
        .await
        .map_err(map)?,
        Some(rid) => {
            sqlx::query("UPDATE roles SET name = $3 WHERE id = $1 AND tenant_id = $2")
                .bind(rid)
                .bind(t.as_uuid())
                .bind(name)
                .execute(&mut *tx)
                .await
                .map_err(map)?;
            rid
        }
    };
    set_role_perms(&mut tx, rid, perms).await?;
    audit(
        &mut tx,
        Some(actor),
        t,
        "role.changed",
        "role",
        rid,
        serde_json::json!({ "name": name, "before": before, "after": perms }),
        ctx,
    )
    .await?;
    tx.commit().await?;
    Ok(rid)
}

pub async fn delete_role(
    pool: &PgPool,
    actor: &Actor,
    id: Uuid,
    ctx: Ctx<'_>,
) -> Result<(), TeamRepoError> {
    let t = actor.tenant_id;
    let mut tx = tenant_tx(pool, t).await?;
    lock_team(&mut tx, t).await?;
    let sys: Option<bool> = sqlx::query_scalar(
        "SELECT is_system FROM roles WHERE id = $1 AND (tenant_id = $2 OR tenant_id IS NULL)",
    )
    .bind(id)
    .bind(t.as_uuid())
    .fetch_optional(&mut *tx)
    .await?;
    match sys {
        None => return Err(TeamRepoError::NotFound),
        Some(true) => return Err(TeamRepoError::SystemRole),
        Some(false) => {}
    }
    let (_, perms) = role_perms(&mut tx, id)
        .await?
        .ok_or(TeamRepoError::NotFound)?;
    grant_ok(actor, &perms)?;
    sqlx::query("DELETE FROM roles WHERE id = $1 AND tenant_id = $2")
        .bind(id)
        .bind(t.as_uuid())
        .execute(&mut *tx)
        .await
        .map_err(|e| match violation(&e) {
            Some((c, _)) if c == "23503" => TeamRepoError::RoleInUse,
            _ => TeamRepoError::Db(e),
        })?;
    audit(
        &mut tx,
        Some(actor),
        t,
        "role.deleted",
        "role",
        id,
        serde_json::json!({}),
        ctx,
    )
    .await?;
    tx.commit().await?;
    Ok(())
}

// ------------------------------------------------------------------ undangan

pub async fn list_invitations(pool: &PgPool, t: TenantId) -> Result<Vec<Invitation>, sqlx::Error> {
    let mut tx = tenant_tx(pool, t).await?;
    let rows = sqlx::query(sqlx::AssertSqlSafe(format!(
        "SELECT i.id, i.email, i.role_id, r.name AS role_name,
                to_char(i.expires_at AT TIME ZONE 'UTC', {TS}) AS expires_at,
                to_char(i.created_at AT TIME ZONE 'UTC', {TS}) AS created_at
         FROM invitations i JOIN roles r ON r.id = i.role_id
         WHERE i.tenant_id = $1 AND i.accepted_at IS NULL AND i.revoked_at IS NULL AND i.expires_at > now()
         ORDER BY i.created_at DESC"
    )))
    .bind(t.as_uuid())
    .fetch_all(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(rows
        .iter()
        .map(|r| Invitation {
            id: r.get("id"),
            email: r.get("email"),
            role_id: r.get("role_id"),
            role_name: r.get("role_name"),
            expires_at: r.get("expires_at"),
            created_at: r.get("created_at"),
        })
        .collect())
}

/// Undang (`members.invite`). Token TIDAK dibuat di sini: worker membuatnya saat mengirim email
/// (payload outbox tanpa rahasia). Undangan tertunda lama untuk email yang sama dicabut.
pub async fn invite(
    pool: &PgPool,
    actor: &Actor,
    email: &str,
    role: Uuid,
    ctx: Ctx<'_>,
) -> Result<Uuid, TeamRepoError> {
    let t = actor.tenant_id;
    let mut tx = tenant_tx(pool, t).await?;
    lock_team(&mut tx, t).await?;
    let (_, perms) = role_perms(&mut tx, role)
        .await?
        .ok_or(TeamRepoError::NotFound)?;
    grant_ok(actor, &perms)?;
    let member: bool = sqlx::query_scalar(
        "SELECT EXISTS (SELECT 1 FROM tenant_memberships m JOIN users u ON u.id = m.user_id
                        WHERE m.tenant_id = $1 AND lower(u.email) = lower($2) AND m.status <> 'removed')",
    )
    .bind(t.as_uuid())
    .bind(email)
    .fetch_one(&mut *tx)
    .await?;
    if member {
        return Err(TeamRepoError::AlreadyMember);
    }
    sqlx::query(
        "UPDATE invitations SET revoked_at = now()
         WHERE tenant_id = $1 AND lower(email) = lower($2) AND accepted_at IS NULL AND revoked_at IS NULL",
    )
    .bind(t.as_uuid())
    .bind(email)
    .execute(&mut *tx)
    .await?;
    // Hash placeholder acak (tidak pernah dikirim ke siapa pun); worker menggantinya.
    let id: Uuid = sqlx::query_scalar(
        "INSERT INTO invitations (tenant_id, email, role_id, token_hash, invited_by, expires_at)
         VALUES ($1, $2, $3, sha256(gen_random_uuid()::text::bytea || gen_random_uuid()::text::bytea), $4,
                 now() + make_interval(secs => $5))
         RETURNING id",
    )
    .bind(t.as_uuid())
    .bind(email)
    .bind(role)
    .bind(actor.user_id)
    .bind(INVITATION_TTL_SECS as f64)
    .fetch_one(&mut *tx)
    .await?;
    crate::outbox::enqueue(
        &mut *tx,
        &OutboxEvent {
            tenant_id: Some(*t.as_uuid()),
            event_type: INVITATION_REQUESTED,
            payload: serde_json::json!({ "invitation_id": id }),
            idempotency_key: format!("invitation:{id}"),
        },
    )
    .await?;
    audit(
        &mut tx,
        Some(actor),
        t,
        "member.invited",
        "invitation",
        id,
        serde_json::json!({ "email_sha256": crate::auth::subject_digest(&email.to_lowercase()), "role_id": role }),
        ctx,
    )
    .await?;
    tx.commit().await?;
    Ok(id)
}

pub async fn revoke_invitation(
    pool: &PgPool,
    actor: &Actor,
    id: Uuid,
    ctx: Ctx<'_>,
) -> Result<(), TeamRepoError> {
    let t = actor.tenant_id;
    let mut tx = tenant_tx(pool, t).await?;
    let n = sqlx::query(
        "UPDATE invitations SET revoked_at = now()
         WHERE tenant_id = $1 AND id = $2 AND accepted_at IS NULL AND revoked_at IS NULL",
    )
    .bind(t.as_uuid())
    .bind(id)
    .execute(&mut *tx)
    .await?
    .rows_affected();
    if n == 0 {
        return Err(TeamRepoError::NotFound);
    }
    audit(
        &mut tx,
        Some(actor),
        t,
        "member.invitation_revoked",
        "invitation",
        id,
        serde_json::json!({}),
        ctx,
    )
    .await?;
    tx.commit().await?;
    Ok(())
}

/// Worker: token baru untuk undangan masih berlaku. `(email, nama toko)` untuk isi email.
pub async fn issue_invitation_token(
    pool: &PgPool,
    t: TenantId,
    id: Uuid,
    hash: TokenHash,
) -> Result<Option<(String, String)>, sqlx::Error> {
    let mut tx = tenant_tx(pool, t).await?;
    let r = sqlx::query(
        "UPDATE invitations i SET token_hash = $3
         FROM tenants tn
         WHERE i.tenant_id = $1 AND i.id = $2 AND tn.id = i.tenant_id
           AND i.accepted_at IS NULL AND i.revoked_at IS NULL AND i.expires_at > now()
         RETURNING i.email, tn.name",
    )
    .bind(t.as_uuid())
    .bind(id)
    .bind(&hash.0[..])
    .fetch_optional(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(r.map(|r| (r.get("email"), r.get("name"))))
}

/// Penerima undangan: user yang sudah ada (sesi, email cocok) atau user baru (token = bukti
/// kepemilikan inbox → email langsung terverifikasi). Sekali pakai (AUTH-010).
pub enum Accepter<'a> {
    Existing { user_id: Uuid, email: &'a str },
    New { password_hash: &'a str },
}

/// Email undangan yang masih berlaku (untuk kebijakan password akun baru).
pub async fn peek(pool: &PgPool, token: TokenHash) -> Result<Option<String>, sqlx::Error> {
    sqlx::query_scalar("SELECT email FROM app.find_invitation($1)")
        .bind(&token.0[..])
        .fetch_optional(pool)
        .await
}

pub async fn accept(
    pool: &PgPool,
    token: TokenHash,
    who: Accepter<'_>,
) -> Result<(TenantId, Uuid), TeamRepoError> {
    let found = sqlx::query("SELECT id, tenant_id, email FROM app.find_invitation($1)")
        .bind(&token.0[..])
        .fetch_optional(pool)
        .await?
        .ok_or(TeamRepoError::NotFound)?;
    let t = TenantId::from_uuid(found.get("tenant_id"));
    let inv: Uuid = found.get("id");
    let email: String = found.get("email");
    let mut tx = tenant_tx(pool, t).await?;
    lock_team(&mut tx, t).await?;
    let r = sqlx::query(
        "SELECT role_id FROM invitations
         WHERE tenant_id = $1 AND id = $2 AND token_hash = $3
           AND accepted_at IS NULL AND revoked_at IS NULL AND expires_at > now() FOR UPDATE",
    )
    .bind(t.as_uuid())
    .bind(inv)
    .bind(&token.0[..])
    .fetch_optional(&mut *tx)
    .await?
    .ok_or(TeamRepoError::NotFound)?;
    let role: Uuid = r.get("role_id");
    let user = match who {
        Accepter::Existing { user_id, email: e } => {
            if !e.eq_ignore_ascii_case(&email) {
                return Err(TeamRepoError::EmailMismatch);
            }
            user_id
        }
        Accepter::New { password_hash } => sqlx::query_scalar(
            "INSERT INTO users (email, password_hash, email_verified_at) VALUES (lower($1), $2, now()) RETURNING id",
        )
        .bind(&email)
        .bind(password_hash)
        .fetch_one(&mut *tx)
        .await
        .map_err(|e| match violation(&e) {
            Some((c, _)) if c == "23505" => TeamRepoError::AccountExists,
            _ => TeamRepoError::Db(e),
        })?,
    };
    sqlx::query("INSERT INTO tenant_memberships (tenant_id, user_id, role_id) VALUES ($1, $2, $3)")
        .bind(t.as_uuid())
        .bind(user)
        .bind(role)
        .execute(&mut *tx)
        .await
        .map_err(|e| match violation(&e) {
            Some((c, _)) if c == "23505" => TeamRepoError::AlreadyMember,
            _ => TeamRepoError::Db(e),
        })?;
    sqlx::query("UPDATE invitations SET accepted_at = now() WHERE tenant_id = $1 AND id = $2")
        .bind(t.as_uuid())
        .bind(inv)
        .execute(&mut *tx)
        .await?;
    crate::audit_repo::record(
        &mut *tx,
        &AuditEvent::new("member.joined", "invitation")
            .by_user(user)
            .in_tenant(t)
            .resource(inv)
            .with_summary(serde_json::json!({ "role_id": role })),
    )
    .await?;
    tx.commit().await?;
    Ok((t, user))
}
