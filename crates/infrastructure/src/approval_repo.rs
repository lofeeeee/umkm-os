//! Approval engine (P4-2, `PRD.md §4.9`). Penyimpanan kebijakan, permintaan, keputusan
//! (append-only), delegasi. Aturan keputusan = `umkm_domain::approvals::check_decision`;
//! eksekusi perubahan dilakukan lapisan aplikasi setelah permintaan `approved`.

use std::collections::BTreeSet;

use sqlx::{PgPool, Row};
use umkm_application::notify::OutboxEvent;
use umkm_domain::{
    approvals::{
        ApprovalError, Decider, EntityType, RequestState, RequestStatus, Step, check_decision,
    },
    audit::AuditEvent,
    rbac::Actor,
};
use umkm_shared::ids::TenantId;
use uuid::Uuid;

use crate::pgx::{Tx, tenant_tx, violation};

pub type Ctx<'a> = (Option<&'a str>, Option<&'a str>);

pub const APPROVAL_REQUESTED: &str = "approval.requested";
pub const APPROVAL_DECIDED: &str = "approval.decided";

#[derive(Debug, thiserror::Error)]
pub enum ApprovalRepoError {
    #[error("tidak ditemukan")]
    NotFound,
    #[error("tidak ada kebijakan approval aktif")]
    NoPolicy,
    #[error("sudah ada permintaan tertunda")]
    AlreadyPending(Uuid),
    #[error(transparent)]
    Rule(#[from] ApprovalError),
    #[error("hanya pemohon yang dapat membatalkan")]
    NotRequester,
    #[error("delegasi tidak valid")]
    InvalidDelegate,
    #[error(transparent)]
    Db(#[from] sqlx::Error),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Policy {
    pub entity_type: EntityType,
    pub enabled: bool,
    pub steps: Vec<Step>,
    pub timeout_hours: i32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Decision {
    pub step: Option<i32>,
    pub decision: String,
    pub actor_user_id: Option<Uuid>,
    pub on_behalf_user_id: Option<Uuid>,
    pub reason: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Request {
    pub id: Uuid,
    pub entity_type: String,
    pub entity_id: Option<Uuid>,
    pub action: String,
    pub payload: serde_json::Value,
    pub summary: String,
    pub status: RequestStatus,
    pub current_step: usize,
    pub steps: Vec<Step>,
    pub requested_by: Uuid,
    pub expires_at: String,
    pub executed_at: Option<String>,
    pub execution_error: Option<String>,
    pub created_at: String,
    pub decisions: Vec<Decision>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Delegation {
    pub id: Uuid,
    pub from_user: Uuid,
    pub to_user: Uuid,
    pub starts_at: String,
    pub ends_at: String,
}

const TS: &str = "'YYYY-MM-DD\"T\"HH24:MI:SS.US\"Z\"'";

fn ts(c: &str) -> String {
    format!("to_char({c} AT TIME ZONE 'UTC', {TS})")
}

async fn audit(
    tx: &mut Tx,
    actor: &Actor,
    action: &'static str,
    id: Uuid,
    summary: serde_json::Value,
    (rid, ip): Ctx<'_>,
) -> Result<(), sqlx::Error> {
    crate::audit_repo::record(
        &mut **tx,
        &AuditEvent::new(action, "approval_request")
            .by_user(actor.user_id)
            .in_tenant(actor.tenant_id)
            .resource(id)
            .request(rid, ip)
            .with_summary(summary),
    )
    .await
}

async fn outbox(
    tx: &mut Tx,
    t: TenantId,
    kind: &'static str,
    id: Uuid,
    key: String,
    extra: serde_json::Value,
) -> Result<(), sqlx::Error> {
    crate::outbox::enqueue(
        &mut **tx,
        &OutboxEvent {
            tenant_id: Some(*t.as_uuid()),
            event_type: kind,
            payload: serde_json::json!({ "request_id": id, "detail": extra }),
            idempotency_key: key,
        },
    )
    .await?;
    Ok(())
}

// ------------------------------------------------------------------ kebijakan

pub async fn policy(
    pool: &PgPool,
    t: TenantId,
    e: EntityType,
) -> Result<Option<Policy>, sqlx::Error> {
    let mut tx = tenant_tx(pool, t).await?;
    let p = policy_in(&mut tx, t, e).await?;
    tx.commit().await?;
    Ok(p)
}

async fn policy_in(tx: &mut Tx, t: TenantId, e: EntityType) -> Result<Option<Policy>, sqlx::Error> {
    let r = sqlx::query("SELECT enabled, steps, timeout_hours FROM approval_policies WHERE tenant_id = $1 AND entity_type = $2")
        .bind(t.as_uuid())
        .bind(e.as_str())
        .fetch_optional(&mut **tx)
        .await?;
    Ok(r.map(|r| Policy {
        entity_type: e,
        enabled: r.get("enabled"),
        steps: r.get::<sqlx::types::Json<Vec<Step>>, _>("steps").0,
        timeout_hours: r.get("timeout_hours"),
    }))
}

/// Kebijakan aktif untuk tipe ini? (dipakai endpoint langsung untuk menolak bypass)
pub async fn required(pool: &PgPool, t: TenantId, e: EntityType) -> Result<bool, sqlx::Error> {
    Ok(policy(pool, t, e).await?.is_some_and(|p| p.enabled))
}

pub async fn list_policies(pool: &PgPool, t: TenantId) -> Result<Vec<Policy>, sqlx::Error> {
    let mut tx = tenant_tx(pool, t).await?;
    let rows = sqlx::query("SELECT entity_type, enabled, steps, timeout_hours FROM approval_policies WHERE tenant_id = $1 ORDER BY entity_type")
        .bind(t.as_uuid())
        .fetch_all(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(rows
        .iter()
        .filter_map(|r| {
            Some(Policy {
                entity_type: EntityType::parse(r.get("entity_type"))?,
                enabled: r.get("enabled"),
                steps: r.get::<sqlx::types::Json<Vec<Step>>, _>("steps").0,
                timeout_hours: r.get("timeout_hours"),
            })
        })
        .collect())
}

/// Simpan kebijakan. Role/user penyetuju harus milik tenant ini (role sistem atau kustom; user anggota).
pub async fn put_policy(
    pool: &PgPool,
    actor: &Actor,
    p: &Policy,
    ctx: Ctx<'_>,
) -> Result<(), ApprovalRepoError> {
    let t = actor.tenant_id;
    let mut tx = tenant_tx(pool, t).await?;
    let roles: Vec<Uuid> = p
        .steps
        .iter()
        .flat_map(|s| s.role_ids.iter().copied())
        .collect();
    let users: Vec<Uuid> = p
        .steps
        .iter()
        .flat_map(|s| s.user_ids.iter().copied())
        .collect();
    let known_roles: i64 =
        sqlx::query_scalar("SELECT count(DISTINCT id) FROM roles WHERE id = ANY($1)")
            .bind(&roles)
            .fetch_one(&mut *tx)
            .await?;
    let known_users: i64 = sqlx::query_scalar(
        "SELECT count(DISTINCT user_id) FROM tenant_memberships WHERE tenant_id = $1 AND user_id = ANY($2) AND status = 'active'",
    )
    .bind(t.as_uuid())
    .bind(&users)
    .fetch_one(&mut *tx)
    .await?;
    let distinct = |v: &[Uuid]| v.iter().collect::<BTreeSet<_>>().len() as i64;
    if known_roles != distinct(&roles) || known_users != distinct(&users) {
        return Err(ApprovalRepoError::InvalidDelegate);
    }
    sqlx::query(
        "INSERT INTO approval_policies (tenant_id, entity_type, enabled, steps, timeout_hours) VALUES ($1, $2, $3, $4, $5)
         ON CONFLICT (tenant_id, entity_type) DO UPDATE SET enabled = EXCLUDED.enabled, steps = EXCLUDED.steps,
             timeout_hours = EXCLUDED.timeout_hours, updated_at = now()",
    )
    .bind(t.as_uuid())
    .bind(p.entity_type.as_str())
    .bind(p.enabled)
    .bind(sqlx::types::Json(&p.steps))
    .bind(p.timeout_hours)
    .execute(&mut *tx)
    .await?;
    crate::audit_repo::record(
        &mut *tx,
        &AuditEvent::new("approval.policy_changed", "approval_policy")
            .by_user(actor.user_id)
            .in_tenant(t)
            .resource(p.entity_type.as_str())
            .request(ctx.0, ctx.1)
            .with_summary(serde_json::json!({ "enabled": p.enabled, "steps": p.steps.len(), "timeout_hours": p.timeout_hours })),
    )
    .await?;
    tx.commit().await?;
    Ok(())
}

// ------------------------------------------------------------------ permintaan

pub struct NewRequest<'a> {
    pub entity_type: EntityType,
    pub entity_id: Option<Uuid>,
    pub action: &'a str,
    pub payload: serde_json::Value,
    pub summary: &'a str,
}

/// AC-APR-001: buat permintaan sesuai kebijakan aktif (langkah disalin: perubahan kebijakan
/// tidak memengaruhi permintaan berjalan).
pub async fn create(
    pool: &PgPool,
    actor: &Actor,
    n: NewRequest<'_>,
    ctx: Ctx<'_>,
) -> Result<Uuid, ApprovalRepoError> {
    let t = actor.tenant_id;
    let mut tx = tenant_tx(pool, t).await?;
    let p = policy_in(&mut tx, t, n.entity_type)
        .await?
        .filter(|p| p.enabled)
        .ok_or(ApprovalRepoError::NoPolicy)?;
    let summary: String = n.summary.chars().take(300).collect();
    let r = sqlx::query_scalar(
        "INSERT INTO approval_requests (tenant_id, entity_type, entity_id, action, payload, summary, steps, requested_by, expires_at)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, now() + make_interval(hours => $9)) RETURNING id",
    )
    .bind(t.as_uuid())
    .bind(n.entity_type.as_str())
    .bind(n.entity_id)
    .bind(n.action)
    .bind(&n.payload)
    .bind(&summary)
    .bind(sqlx::types::Json(&p.steps))
    .bind(actor.user_id)
    .bind(p.timeout_hours)
    .fetch_one(&mut *tx)
    .await;
    let id: Uuid = match r {
        Ok(id) => id,
        Err(e)
            if violation(&e)
                .is_some_and(|(c, n)| c == "23505" && n == "uq_approval_requests_pending") =>
        {
            drop(tx);
            let mut tx = tenant_tx(pool, t).await?;
            let existing: Uuid = sqlx::query_scalar(
                "SELECT id FROM approval_requests WHERE tenant_id = $1 AND entity_type = $2
                   AND COALESCE(entity_id, '00000000-0000-0000-0000-000000000000'::uuid) = COALESCE($3, '00000000-0000-0000-0000-000000000000'::uuid)
                   AND action = $4 AND status = 'pending'",
            )
            .bind(t.as_uuid())
            .bind(n.entity_type.as_str())
            .bind(n.entity_id)
            .bind(n.action)
            .fetch_one(&mut *tx)
            .await?;
            tx.commit().await?;
            return Err(ApprovalRepoError::AlreadyPending(existing));
        }
        Err(e) => return Err(e.into()),
    };
    sqlx::query("INSERT INTO approval_decisions (tenant_id, request_id, decision, actor_user_id) VALUES ($1, $2, 'requested', $3)")
        .bind(t.as_uuid())
        .bind(id)
        .bind(actor.user_id)
        .execute(&mut *tx)
        .await?;
    outbox(
        &mut tx,
        t,
        APPROVAL_REQUESTED,
        id,
        format!("approval_requested:{id}"),
        serde_json::json!({ "entity_type": n.entity_type.as_str(), "summary": summary }),
    )
    .await?;
    audit(
        &mut tx,
        actor,
        "approval.requested",
        id,
        serde_json::json!({ "entity_type": n.entity_type.as_str(), "action": n.action }),
        ctx,
    )
    .await?;
    tx.commit().await?;
    Ok(id)
}

fn request_cols() -> String {
    format!(
        "id, entity_type, entity_id, action, payload, summary, status, current_step, steps, requested_by,
         {} AS expires_at, {} AS executed_at, execution_error, {} AS created_at",
        ts("expires_at"),
        ts("executed_at"),
        ts("created_at")
    )
}

fn request_of(r: &sqlx::postgres::PgRow) -> Request {
    Request {
        id: r.get("id"),
        entity_type: r.get("entity_type"),
        entity_id: r.get("entity_id"),
        action: r.get("action"),
        payload: r.get("payload"),
        summary: r.get("summary"),
        status: RequestStatus::parse(r.get("status")).unwrap_or(RequestStatus::Expired),
        current_step: usize::try_from(r.get::<i32, _>("current_step")).unwrap_or(0),
        steps: r.get::<sqlx::types::Json<Vec<Step>>, _>("steps").0,
        requested_by: r.get("requested_by"),
        expires_at: r.get("expires_at"),
        executed_at: r.get("executed_at"),
        execution_error: r.get("execution_error"),
        created_at: r.get("created_at"),
        decisions: vec![],
    }
}

async fn decisions(tx: &mut Tx, t: TenantId, id: Uuid) -> Result<Vec<Decision>, sqlx::Error> {
    let rows = sqlx::query(sqlx::AssertSqlSafe(format!(
        "SELECT step, decision, actor_user_id, on_behalf_user_id, reason, {} AS created_at
         FROM approval_decisions WHERE tenant_id = $1 AND request_id = $2 ORDER BY seq",
        ts("created_at")
    )))
    .bind(t.as_uuid())
    .bind(id)
    .fetch_all(&mut **tx)
    .await?;
    Ok(rows
        .iter()
        .map(|r| Decision {
            step: r.get("step"),
            decision: r.get("decision"),
            actor_user_id: r.get("actor_user_id"),
            on_behalf_user_id: r.get("on_behalf_user_id"),
            reason: r.get("reason"),
            created_at: r.get("created_at"),
        })
        .collect())
}

pub async fn get(pool: &PgPool, t: TenantId, id: Uuid) -> Result<Option<Request>, sqlx::Error> {
    let mut tx = tenant_tx(pool, t).await?;
    let r = sqlx::query(sqlx::AssertSqlSafe(format!(
        "SELECT {} FROM approval_requests WHERE tenant_id = $1 AND id = $2",
        request_cols()
    )))
    .bind(t.as_uuid())
    .bind(id)
    .fetch_optional(&mut *tx)
    .await?;
    let out = match r {
        Some(r) => {
            let mut req = request_of(&r);
            req.decisions = decisions(&mut tx, t, id).await?;
            Some(req)
        }
        None => None,
    };
    tx.commit().await?;
    Ok(out)
}

pub async fn list(
    pool: &PgPool,
    t: TenantId,
    status: Option<RequestStatus>,
    limit: i64,
    offset: i64,
) -> Result<(Vec<Request>, i64), sqlx::Error> {
    let mut tx = tenant_tx(pool, t).await?;
    let total: i64 = sqlx::query_scalar("SELECT count(*) FROM approval_requests WHERE tenant_id = $1 AND ($2::text IS NULL OR status = $2)")
        .bind(t.as_uuid())
        .bind(status.map(RequestStatus::as_str))
        .fetch_one(&mut *tx)
        .await?;
    let rows = sqlx::query(sqlx::AssertSqlSafe(format!(
        "SELECT {} FROM approval_requests WHERE tenant_id = $1 AND ($2::text IS NULL OR status = $2)
         ORDER BY created_at DESC, id LIMIT $3 OFFSET $4",
        request_cols()
    )))
    .bind(t.as_uuid())
    .bind(status.map(RequestStatus::as_str))
    .bind(limit)
    .bind(offset)
    .fetch_all(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok((rows.iter().map(request_of).collect(), total))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    /// Lanjut ke langkah berikutnya
    Advanced,
    /// Langkah terakhir disetujui → jalankan executor
    Approved,
    Rejected,
}

/// AC-APR-002/003: putuskan langkah `step`. Kunci baris permintaan → dua keputusan paralel
/// untuk langkah yang sama: hanya satu yang tercatat.
pub async fn decide(
    pool: &PgPool,
    actor: &Actor,
    id: Uuid,
    step: usize,
    approve: bool,
    reason: Option<&str>,
    ctx: Ctx<'_>,
) -> Result<(Outcome, Request), ApprovalRepoError> {
    let t = actor.tenant_id;
    let mut tx = tenant_tx(pool, t).await?;
    let r = sqlx::query(sqlx::AssertSqlSafe(format!(
        "SELECT {} FROM approval_requests WHERE tenant_id = $1 AND id = $2 FOR UPDATE",
        request_cols()
    )))
    .bind(t.as_uuid())
    .bind(id)
    .fetch_optional(&mut *tx)
    .await?
    .ok_or(ApprovalRepoError::NotFound)?;
    let mut req = request_of(&r);
    // Timeout dihormati walau job belum berjalan.
    let expired: bool =
        sqlx::query_scalar("SELECT expires_at <= now() FROM approval_requests WHERE id = $1")
            .bind(id)
            .fetch_one(&mut *tx)
            .await?;
    let prior: BTreeSet<Uuid> = sqlx::query(
        "SELECT actor_user_id, on_behalf_user_id FROM approval_decisions WHERE tenant_id = $1 AND request_id = $2 AND decision = 'approved'",
    )
    .bind(t.as_uuid())
    .bind(id)
    .fetch_all(&mut *tx)
    .await?
    .iter()
    .flat_map(|x| [x.get::<Option<Uuid>, _>("actor_user_id"), x.get::<Option<Uuid>, _>("on_behalf_user_id")])
    .flatten()
    .collect();
    let role_id: Uuid = sqlx::query_scalar("SELECT role_id FROM tenant_memberships WHERE tenant_id = $1 AND user_id = $2 AND status = 'active'")
        .bind(t.as_uuid())
        .bind(actor.user_id)
        .fetch_one(&mut *tx)
        .await?;
    // Delegasi aktif ke pelaku dari anggota aktif yang memegang approvals.decide.
    let delegators: Vec<(Uuid, Uuid)> = sqlx::query(
        "SELECT d.from_user, m.role_id FROM approval_delegations d
         JOIN tenant_memberships m ON m.tenant_id = d.tenant_id AND m.user_id = d.from_user AND m.status = 'active'
         JOIN role_permissions rp ON rp.role_id = m.role_id
         JOIN permissions p ON p.id = rp.permission_id AND p.permission_key = 'approvals.decide'
         WHERE d.tenant_id = $1 AND d.to_user = $2 AND d.revoked_at IS NULL AND now() BETWEEN d.starts_at AND d.ends_at",
    )
    .bind(t.as_uuid())
    .bind(actor.user_id)
    .fetch_all(&mut *tx)
    .await?
    .iter()
    .map(|x| (x.get("from_user"), x.get("role_id")))
    .collect();
    let state = RequestState {
        status: if expired && req.status == RequestStatus::Pending {
            RequestStatus::Expired
        } else {
            req.status
        },
        current_step: req.current_step,
        steps: req.steps.clone(),
        requested_by: req.requested_by,
        prior_approvers: prior,
    };
    let on_behalf = check_decision(
        &state,
        step,
        &Decider {
            user_id: actor.user_id,
            role_id,
            delegators,
        },
        approve,
        reason,
    )?;
    sqlx::query(
        "INSERT INTO approval_decisions (tenant_id, request_id, step, decision, actor_user_id, on_behalf_user_id, reason)
         VALUES ($1, $2, $3, $4, $5, $6, $7)",
    )
    .bind(t.as_uuid())
    .bind(id)
    .bind(i32::try_from(step).unwrap_or(i32::MAX))
    .bind(if approve { "approved" } else { "rejected" })
    .bind(actor.user_id)
    .bind((on_behalf != actor.user_id).then_some(on_behalf))
    .bind(reason)
    .execute(&mut *tx)
    .await?;
    let last = step + 1 >= req.steps.len();
    let outcome = match (approve, last) {
        (false, _) => Outcome::Rejected,
        (true, true) => Outcome::Approved,
        (true, false) => Outcome::Advanced,
    };
    match outcome {
        Outcome::Advanced => {
            sqlx::query("UPDATE approval_requests SET current_step = current_step + 1 WHERE tenant_id = $1 AND id = $2")
                .bind(t.as_uuid())
                .bind(id)
                .execute(&mut *tx)
                .await?;
            req.current_step += 1;
        }
        Outcome::Approved | Outcome::Rejected => {
            let st = if outcome == Outcome::Approved {
                RequestStatus::Approved
            } else {
                RequestStatus::Rejected
            };
            sqlx::query("UPDATE approval_requests SET status = $3, decided_at = now(), current_step = current_step + 1 WHERE tenant_id = $1 AND id = $2")
                .bind(t.as_uuid())
                .bind(id)
                .bind(st.as_str())
                .execute(&mut *tx)
                .await?;
            req.status = st;
        }
    }
    outbox(
        &mut tx,
        t,
        APPROVAL_DECIDED,
        id,
        format!("approval_decided:{id}:{step}"),
        serde_json::json!({ "approved": approve, "final": outcome != Outcome::Advanced }),
    )
    .await?;
    audit(
        &mut tx,
        actor,
        if approve { "approval.approved" } else { "approval.rejected" },
        id,
        serde_json::json!({ "step": step, "on_behalf_of": (on_behalf != actor.user_id).then_some(on_behalf), "reason": reason }),
        ctx,
    )
    .await?;
    tx.commit().await?;
    Ok((outcome, req))
}

/// Catat hasil executor (sekali; idempoten terhadap pemanggilan ulang).
pub async fn mark_executed(
    pool: &PgPool,
    t: TenantId,
    id: Uuid,
    result: Result<(), String>,
) -> Result<(), sqlx::Error> {
    let mut tx = tenant_tx(pool, t).await?;
    let n = sqlx::query(
        "UPDATE approval_requests SET executed_at = CASE WHEN $3::text IS NULL THEN now() END, execution_error = $3
         WHERE tenant_id = $1 AND id = $2 AND status = 'approved' AND executed_at IS NULL AND execution_error IS NULL",
    )
    .bind(t.as_uuid())
    .bind(id)
    .bind(result.as_ref().err())
    .execute(&mut *tx)
    .await?
    .rows_affected();
    if n == 1 {
        sqlx::query("INSERT INTO approval_decisions (tenant_id, request_id, decision, reason) VALUES ($1, $2, $3, $4)")
            .bind(t.as_uuid())
            .bind(id)
            .bind(if result.is_ok() { "executed" } else { "execution_failed" })
            .bind(result.err().map(|e| e.chars().take(500).collect::<String>()))
            .execute(&mut *tx)
            .await?;
    }
    tx.commit().await?;
    Ok(())
}

pub async fn cancel(
    pool: &PgPool,
    actor: &Actor,
    id: Uuid,
    ctx: Ctx<'_>,
) -> Result<(), ApprovalRepoError> {
    let t = actor.tenant_id;
    let mut tx = tenant_tx(pool, t).await?;
    let r = sqlx::query("SELECT requested_by, status FROM approval_requests WHERE tenant_id = $1 AND id = $2 FOR UPDATE")
        .bind(t.as_uuid())
        .bind(id)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or(ApprovalRepoError::NotFound)?;
    if r.get::<Uuid, _>("requested_by") != actor.user_id {
        return Err(ApprovalRepoError::NotRequester);
    }
    if r.get::<String, _>("status") != "pending" {
        return Err(ApprovalError::NotPending.into());
    }
    sqlx::query("UPDATE approval_requests SET status = 'cancelled', decided_at = now() WHERE tenant_id = $1 AND id = $2")
        .bind(t.as_uuid())
        .bind(id)
        .execute(&mut *tx)
        .await?;
    sqlx::query("INSERT INTO approval_decisions (tenant_id, request_id, decision, actor_user_id) VALUES ($1, $2, 'cancelled', $3)")
        .bind(t.as_uuid())
        .bind(id)
        .bind(actor.user_id)
        .execute(&mut *tx)
        .await?;
    audit(
        &mut tx,
        actor,
        "approval.cancelled",
        id,
        serde_json::json!({}),
        ctx,
    )
    .await?;
    tx.commit().await?;
    Ok(())
}

// ------------------------------------------------------------------ delegasi

pub async fn delegate(
    pool: &PgPool,
    actor: &Actor,
    to_user: Uuid,
    hours: i32,
    ctx: Ctx<'_>,
) -> Result<Uuid, ApprovalRepoError> {
    let t = actor.tenant_id;
    let mut tx = tenant_tx(pool, t).await?;
    let ok: bool = sqlx::query_scalar(
        "SELECT EXISTS (SELECT 1 FROM tenant_memberships WHERE tenant_id = $1 AND user_id = $2 AND status = 'active')",
    )
    .bind(t.as_uuid())
    .bind(to_user)
    .fetch_one(&mut *tx)
    .await?;
    if !ok || to_user == actor.user_id || !(1..=24 * 90).contains(&hours) {
        return Err(ApprovalRepoError::InvalidDelegate);
    }
    let id: Uuid = sqlx::query_scalar(
        "INSERT INTO approval_delegations (tenant_id, from_user, to_user, ends_at) VALUES ($1, $2, $3, now() + make_interval(hours => $4)) RETURNING id",
    )
    .bind(t.as_uuid())
    .bind(actor.user_id)
    .bind(to_user)
    .bind(hours)
    .fetch_one(&mut *tx)
    .await?;
    crate::audit_repo::record(
        &mut *tx,
        &AuditEvent::new("approval.delegated", "approval_delegation")
            .by_user(actor.user_id)
            .in_tenant(t)
            .resource(id)
            .request(ctx.0, ctx.1)
            .with_summary(serde_json::json!({ "to_user": to_user, "hours": hours })),
    )
    .await?;
    tx.commit().await?;
    Ok(id)
}

pub async fn delegations(pool: &PgPool, t: TenantId) -> Result<Vec<Delegation>, sqlx::Error> {
    let mut tx = tenant_tx(pool, t).await?;
    let rows = sqlx::query(sqlx::AssertSqlSafe(format!(
        "SELECT id, from_user, to_user, {} AS starts_at, {} AS ends_at FROM approval_delegations
         WHERE tenant_id = $1 AND revoked_at IS NULL AND ends_at > now() ORDER BY created_at DESC",
        ts("starts_at"),
        ts("ends_at")
    )))
    .bind(t.as_uuid())
    .fetch_all(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(rows
        .iter()
        .map(|r| Delegation {
            id: r.get("id"),
            from_user: r.get("from_user"),
            to_user: r.get("to_user"),
            starts_at: r.get("starts_at"),
            ends_at: r.get("ends_at"),
        })
        .collect())
}

/// Hanya pemberi delegasi yang dapat mencabutnya.
pub async fn revoke_delegation(
    pool: &PgPool,
    actor: &Actor,
    id: Uuid,
) -> Result<(), ApprovalRepoError> {
    let t = actor.tenant_id;
    let mut tx = tenant_tx(pool, t).await?;
    let n = sqlx::query("UPDATE approval_delegations SET revoked_at = now() WHERE tenant_id = $1 AND id = $2 AND from_user = $3 AND revoked_at IS NULL")
        .bind(t.as_uuid())
        .bind(id)
        .bind(actor.user_id)
        .execute(&mut *tx)
        .await?
        .rows_affected();
    if n == 0 {
        return Err(ApprovalRepoError::NotFound);
    }
    tx.commit().await?;
    Ok(())
}

/// Job worker: timeout → expired.
pub async fn expire(pool: &PgPool, batch: i32) -> Result<i32, sqlx::Error> {
    sqlx::query_scalar("SELECT app.expire_approvals($1)")
        .bind(batch)
        .fetch_one(pool)
        .await
}
