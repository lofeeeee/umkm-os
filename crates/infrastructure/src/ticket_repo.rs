//! Ticketing (P4-3, `PRD.md §4.8`). Tenant context (RLS). Setiap perubahan → `ticket_events`
//! (append-only) + outbox notifikasi; perubahan staf juga di audit log.

use sqlx::{PgPool, Row};
use umkm_application::notify::OutboxEvent;
use umkm_domain::{
    audit::AuditEvent,
    rbac::Actor,
    storage::{UploadError, sanitize_image},
    tickets::{Priority, Source, TicketStatus, can_reopen},
};
use umkm_shared::ids::TenantId;
use uuid::Uuid;

use crate::pgx::{Tx, tenant_tx, violation};

pub const TICKET_CREATED: &str = "ticket.created";
pub const TICKET_ASSIGNED: &str = "ticket.assigned";
pub const TICKET_CUSTOMER_REPLY: &str = "ticket.customer_reply";

pub type Ctx<'a> = (Option<&'a str>, Option<&'a str>);

#[derive(Debug, thiserror::Error)]
pub enum TicketError {
    #[error("tidak ditemukan")]
    NotFound,
    #[error("rujukan (order/customer) bukan milik tenant")]
    InvalidReference,
    #[error("assignee bukan anggota aktif")]
    InvalidAssignee,
    #[error("transisi status tidak diizinkan")]
    InvalidTransition(&'static str),
    #[error("ticket sudah ditutup terlalu lama untuk dibuka kembali")]
    ReopenExpired,
    #[error(transparent)]
    Upload(#[from] UploadError),
    #[error(transparent)]
    Db(#[from] sqlx::Error),
}

/// Siapa yang membaca: staf dengan cakupan, atau customer (hanya publik).
#[derive(Debug, Clone, Copy)]
pub enum Scope {
    /// `tickets.assign`: semua ticket tenant
    All,
    /// Agen: hanya ticket yang ditugaskan kepadanya (object-level, `PRD.md §3.2`)
    AssignedTo(Uuid),
    /// Customer lewat token order: hanya ticket order itu, hanya isi publik
    CustomerOrder(Uuid),
}

#[derive(Debug, Clone)]
pub struct NewTicket {
    pub source: Source,
    pub customer_id: Option<Uuid>,
    pub order_id: Option<Uuid>,
    pub conversation_id: Option<Uuid>,
    pub subject: String,
    pub description: Option<String>,
    pub priority: Priority,
    pub tags: Vec<String>,
    pub assignee: Option<Uuid>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Message {
    pub id: Uuid,
    pub visibility: String,
    pub author_type: String,
    pub author_user_id: Option<Uuid>,
    pub body: String,
    pub created_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Attachment {
    pub id: Uuid,
    pub visibility: String,
    pub content_type: String,
    pub byte_size: i32,
    pub uploaded_by: String,
    pub created_at: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Event {
    pub kind: String,
    pub data: serde_json::Value,
    pub actor_type: String,
    pub actor_user_id: Option<Uuid>,
    pub created_at: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Ticket {
    pub id: Uuid,
    pub code: String,
    pub source: String,
    pub customer_id: Option<Uuid>,
    pub order_id: Option<Uuid>,
    pub conversation_id: Option<Uuid>,
    pub subject: String,
    pub description: Option<String>,
    pub priority: String,
    pub status: TicketStatus,
    pub assignee_user_id: Option<Uuid>,
    pub tags: Vec<String>,
    pub first_response_due_at: String,
    pub resolution_due_at: String,
    pub first_responded_at: Option<String>,
    pub resolved_at: Option<String>,
    pub closed_at: Option<String>,
    pub sla_first_response_breached: bool,
    pub sla_resolution_breached: bool,
    pub reopen_count: i32,
    pub created_at: String,
    pub updated_at: String,
    pub messages: Vec<Message>,
    pub attachments: Vec<Attachment>,
    pub events: Vec<Event>,
}

const TS: &str = "'YYYY-MM-DD\"T\"HH24:MI:SS.US\"Z\"'";

fn ts(c: &str) -> String {
    format!("to_char({c} AT TIME ZONE 'UTC', {TS})")
}

fn cols() -> String {
    format!(
        "t.id, t.code, t.source, t.customer_id, t.order_id, t.conversation_id, t.subject, t.description, t.priority, t.status,
         t.assignee_user_id, t.tags, {} AS first_response_due_at, {} AS resolution_due_at, {} AS first_responded_at,
         {} AS resolved_at, {} AS closed_at,
         (COALESCE(t.first_responded_at, now()) > t.first_response_due_at) AS frb,
         (COALESCE(t.resolved_at, now()) > t.resolution_due_at) AS rb,
         t.reopen_count, {} AS created_at, {} AS updated_at",
        ts("t.first_response_due_at"),
        ts("t.resolution_due_at"),
        ts("t.first_responded_at"),
        ts("t.resolved_at"),
        ts("t.closed_at"),
        ts("t.created_at"),
        ts("t.updated_at")
    )
}

fn ticket_of(r: &sqlx::postgres::PgRow) -> Ticket {
    Ticket {
        id: r.get("id"),
        code: r.get("code"),
        source: r.get("source"),
        customer_id: r.get("customer_id"),
        order_id: r.get("order_id"),
        conversation_id: r.get("conversation_id"),
        subject: r.get("subject"),
        description: r.get("description"),
        priority: r.get("priority"),
        status: TicketStatus::parse(r.get("status")).unwrap_or(TicketStatus::Open),
        assignee_user_id: r.get("assignee_user_id"),
        tags: r.get("tags"),
        first_response_due_at: r.get("first_response_due_at"),
        resolution_due_at: r.get("resolution_due_at"),
        first_responded_at: r.get("first_responded_at"),
        resolved_at: r.get("resolved_at"),
        closed_at: r.get("closed_at"),
        sla_first_response_breached: r.get("frb"),
        sla_resolution_breached: r.get("rb"),
        reopen_count: r.get("reopen_count"),
        created_at: r.get("created_at"),
        updated_at: r.get("updated_at"),
        messages: vec![],
        attachments: vec![],
        events: vec![],
    }
}

/// Filter SQL untuk cakupan pembaca (parameter $2).
fn scope_sql(s: Scope) -> (&'static str, Option<Uuid>) {
    match s {
        Scope::All => ("($2::uuid IS NULL)", None),
        Scope::AssignedTo(u) => ("t.assignee_user_id = $2", Some(u)),
        Scope::CustomerOrder(o) => ("t.order_id = $2", Some(o)),
    }
}

async fn event(
    tx: &mut Tx,
    t: TenantId,
    ticket: Uuid,
    kind: &str,
    data: serde_json::Value,
    actor: Option<Uuid>,
    actor_type: &str,
) -> Result<(), sqlx::Error> {
    sqlx::query("INSERT INTO ticket_events (tenant_id, ticket_id, kind, data, actor_type, actor_user_id) VALUES ($1, $2, $3, $4, $5, $6)")
        .bind(t.as_uuid())
        .bind(ticket)
        .bind(kind)
        .bind(data)
        .bind(actor_type)
        .bind(actor)
        .execute(&mut **tx)
        .await?;
    Ok(())
}

async fn notify(
    tx: &mut Tx,
    t: TenantId,
    kind: &'static str,
    ticket: Uuid,
    key: String,
    extra: serde_json::Value,
) -> Result<(), sqlx::Error> {
    crate::outbox::enqueue(
        &mut **tx,
        &OutboxEvent {
            tenant_id: Some(*t.as_uuid()),
            event_type: kind,
            payload: serde_json::json!({ "ticket_id": ticket, "detail": extra }),
            idempotency_key: key,
        },
    )
    .await?;
    Ok(())
}

async fn audit(
    tx: &mut Tx,
    actor: &Actor,
    action: &'static str,
    ticket: Uuid,
    summary: serde_json::Value,
    (rid, ip): Ctx<'_>,
) -> Result<(), sqlx::Error> {
    crate::audit_repo::record(
        &mut **tx,
        &AuditEvent::new(action, "ticket")
            .by_user(actor.user_id)
            .in_tenant(actor.tenant_id)
            .resource(ticket)
            .request(rid, ip)
            .with_summary(summary),
    )
    .await
}

fn map_write(e: sqlx::Error) -> TicketError {
    match violation(&e) {
        Some((c, n))
            if c == "23503"
                && (n == "fk_tickets_customer"
                    || n == "fk_tickets_order"
                    || n == "fk_tickets_conversation") =>
        {
            TicketError::InvalidReference
        }
        Some((c, n)) if c == "23514" && n == "ck_tickets_assignee_member" => {
            TicketError::InvalidAssignee
        }
        _ => TicketError::Db(e),
    }
}

// ------------------------------------------------------------------ buat

/// AC-TKT-001: ticket dari chat/order/manual/system; `staff` = pembuat (manual).
pub async fn create(
    pool: &PgPool,
    t: TenantId,
    n: &NewTicket,
    staff: Option<&Actor>,
    ctx: Ctx<'_>,
) -> Result<Uuid, TicketError> {
    let mut tx = tenant_tx(pool, t).await?;
    let id = create_in(&mut tx, t, n, staff, ctx).await?;
    tx.commit().await?;
    Ok(id)
}

pub async fn create_in(
    tx: &mut Tx,
    t: TenantId,
    n: &NewTicket,
    staff: Option<&Actor>,
    ctx: Ctx<'_>,
) -> Result<Uuid, TicketError> {
    let id = Uuid::new_v4();
    let code = format!("TKT-{}", id.simple().to_string()[..10].to_uppercase());
    let (fr, res) = n.priority.sla_secs();
    let status = if n.assignee.is_some() {
        TicketStatus::Assigned
    } else {
        TicketStatus::Open
    };
    sqlx::query(
        "INSERT INTO tickets (id, tenant_id, code, source, customer_id, order_id, conversation_id, subject, description,
             priority, status, assignee_user_id, tags, first_response_due_at, resolution_due_at, created_by_user_id)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13,
             now() + make_interval(secs => $14), now() + make_interval(secs => $15), $16)",
    )
    .bind(id)
    .bind(t.as_uuid())
    .bind(&code)
    .bind(n.source.as_str())
    .bind(n.customer_id)
    .bind(n.order_id)
    .bind(n.conversation_id)
    .bind(&n.subject)
    .bind(&n.description)
    .bind(n.priority.as_str())
    .bind(status.as_str())
    .bind(n.assignee)
    .bind(&n.tags)
    .bind(fr as f64)
    .bind(res as f64)
    .bind(staff.map(|a| a.user_id))
    .execute(&mut **tx)
    .await
    .map_err(map_write)?;
    let actor_type = if staff.is_some() {
        "staff"
    } else if n.source == Source::Order {
        "customer"
    } else {
        "system"
    };
    event(
        tx,
        t,
        id,
        "created",
        serde_json::json!({ "source": n.source.as_str(), "priority": n.priority.as_str() }),
        staff.map(|a| a.user_id),
        actor_type,
    )
    .await?;
    notify(
        tx,
        t,
        TICKET_CREATED,
        id,
        format!("ticket_created:{id}"),
        serde_json::json!({ "code": code }),
    )
    .await?;
    if let Some(u) = n.assignee {
        notify(
            tx,
            t,
            TICKET_ASSIGNED,
            id,
            format!("ticket_assigned:{id}:{u}:0"),
            serde_json::json!({ "assignee": u }),
        )
        .await?;
    }
    if let Some(a) = staff {
        audit(
            tx,
            a,
            "ticket.created",
            id,
            serde_json::json!({ "source": n.source.as_str(), "assignee": n.assignee }),
            ctx,
        )
        .await?;
    }
    Ok(id)
}

// ------------------------------------------------------------------ baca

#[derive(Debug, Clone, Default)]
pub struct Filter {
    pub status: Option<TicketStatus>,
    pub assignee: Option<Uuid>,
    pub priority: Option<String>,
    pub tag: Option<String>,
    pub q: Option<String>,
    pub customer_id: Option<Uuid>,
    pub order_id: Option<Uuid>,
    pub limit: i64,
    pub offset: i64,
}

pub async fn list(
    pool: &PgPool,
    t: TenantId,
    scope: Scope,
    f: &Filter,
) -> Result<(Vec<Ticket>, i64), sqlx::Error> {
    let mut tx = tenant_tx(pool, t).await?;
    let (sc, sv) = scope_sql(scope);
    let q = f.q.as_deref().map(|q| {
        format!(
            "%{}%",
            q.replace('\\', "\\\\")
                .replace('%', "\\%")
                .replace('_', "\\_")
        )
    });
    let w = format!(
        "FROM tickets t WHERE t.tenant_id = $1 AND {sc}
           AND ($3::text IS NULL OR t.status = $3) AND ($4::uuid IS NULL OR t.assignee_user_id = $4)
           AND ($5::text IS NULL OR t.priority = $5) AND ($6::text IS NULL OR $6 = ANY(t.tags))
           AND ($7::text IS NULL OR t.subject ILIKE $7 OR t.code ILIKE $7)
           AND ($8::uuid IS NULL OR t.customer_id = $8) AND ($9::uuid IS NULL OR t.order_id = $9)"
    );
    let bind = |qry: sqlx::query::Query<'static, sqlx::Postgres, sqlx::postgres::PgArguments>| {
        qry.bind(t.as_uuid())
            .bind(sv)
            .bind(f.status.map(TicketStatus::as_str))
            .bind(f.assignee)
            .bind(f.priority.clone())
            .bind(f.tag.clone())
            .bind(q.clone())
            .bind(f.customer_id)
            .bind(f.order_id)
    };
    let total: i64 = bind(sqlx::query(sqlx::AssertSqlSafe(format!(
        "SELECT count(*) AS n {w}"
    ))))
    .fetch_one(&mut *tx)
    .await?
    .get("n");
    let rows = bind(sqlx::query(sqlx::AssertSqlSafe(format!(
        "SELECT {} {w} ORDER BY t.updated_at DESC, t.id LIMIT $10 OFFSET $11",
        cols()
    ))))
    .bind(f.limit)
    .bind(f.offset)
    .fetch_all(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok((rows.iter().map(ticket_of).collect(), total))
}

/// Detail sesuai cakupan; customer tidak pernah menerima pesan/lampiran internal maupun riwayat.
pub async fn get(
    pool: &PgPool,
    t: TenantId,
    scope: Scope,
    id: Uuid,
) -> Result<Option<Ticket>, sqlx::Error> {
    let mut tx = tenant_tx(pool, t).await?;
    let (sc, sv) = scope_sql(scope);
    let Some(r) = sqlx::query(sqlx::AssertSqlSafe(format!(
        "SELECT {} FROM tickets t WHERE t.tenant_id = $1 AND {sc} AND t.id = $3",
        cols()
    )))
    .bind(t.as_uuid())
    .bind(sv)
    .bind(id)
    .fetch_optional(&mut *tx)
    .await?
    else {
        tx.commit().await?;
        return Ok(None);
    };
    let mut tk = ticket_of(&r);
    let public_only = matches!(scope, Scope::CustomerOrder(_));
    tk.messages = sqlx::query(sqlx::AssertSqlSafe(format!(
        "SELECT id, visibility, author_type, author_user_id, body, {} AS created_at FROM ticket_messages
         WHERE tenant_id = $1 AND ticket_id = $2 AND (NOT $3 OR visibility = 'public') ORDER BY seq",
        ts("created_at")
    )))
    .bind(t.as_uuid())
    .bind(id)
    .bind(public_only)
    .fetch_all(&mut *tx)
    .await?
    .iter()
    .map(|m| Message { id: m.get("id"), visibility: m.get("visibility"), author_type: m.get("author_type"), author_user_id: m.get("author_user_id"), body: m.get("body"), created_at: m.get("created_at") })
    .collect();
    tk.attachments = sqlx::query(sqlx::AssertSqlSafe(format!(
        "SELECT id, visibility, content_type, byte_size, uploaded_by, {} AS created_at FROM ticket_attachments
         WHERE tenant_id = $1 AND ticket_id = $2 AND (NOT $3 OR visibility = 'public') ORDER BY created_at, id",
        ts("created_at")
    )))
    .bind(t.as_uuid())
    .bind(id)
    .bind(public_only)
    .fetch_all(&mut *tx)
    .await?
    .iter()
    .map(|a| Attachment { id: a.get("id"), visibility: a.get("visibility"), content_type: a.get("content_type"), byte_size: a.get("byte_size"), uploaded_by: a.get("uploaded_by"), created_at: a.get("created_at") })
    .collect();
    if !public_only {
        tk.events = sqlx::query(sqlx::AssertSqlSafe(format!(
            "SELECT kind, data, actor_type, actor_user_id, {} AS created_at FROM ticket_events WHERE tenant_id = $1 AND ticket_id = $2 ORDER BY seq",
            ts("created_at")
        )))
        .bind(t.as_uuid())
        .bind(id)
        .fetch_all(&mut *tx)
        .await?
        .iter()
        .map(|e| Event { kind: e.get("kind"), data: e.get("data"), actor_type: e.get("actor_type"), actor_user_id: e.get("actor_user_id"), created_at: e.get("created_at") })
        .collect();
    }
    tx.commit().await?;
    Ok(Some(tk))
}

async fn lock(
    tx: &mut Tx,
    t: TenantId,
    scope: Scope,
    id: Uuid,
) -> Result<(TicketStatus, Option<Uuid>, Option<i64>), TicketError> {
    let (sc, sv) = scope_sql(scope);
    let r = sqlx::query(sqlx::AssertSqlSafe(format!(
        "SELECT t.status, t.assignee_user_id, extract(epoch FROM now() - t.closed_at)::bigint AS since_closed
         FROM tickets t WHERE t.tenant_id = $1 AND {sc} AND t.id = $3 FOR UPDATE"
    )))
    .bind(t.as_uuid())
    .bind(sv)
    .bind(id)
    .fetch_optional(&mut **tx)
    .await?
    .ok_or(TicketError::NotFound)?;
    Ok((
        TicketStatus::parse(r.get("status")).unwrap_or(TicketStatus::Open),
        r.get("assignee_user_id"),
        r.get("since_closed"),
    ))
}

#[allow(clippy::too_many_arguments)]
async fn apply_status(
    tx: &mut Tx,
    t: TenantId,
    id: Uuid,
    from: TicketStatus,
    to: TicketStatus,
    since_closed: Option<i64>,
    actor: Option<Uuid>,
    actor_type: &str,
    reason: Option<&str>,
) -> Result<(), TicketError> {
    if !from.can_transition(to) {
        return Err(TicketError::InvalidTransition(from.as_str()));
    }
    let reopen = from.is_reopen(to);
    if reopen && !can_reopen(from, since_closed) {
        return Err(TicketError::ReopenExpired);
    }
    sqlx::query(
        "UPDATE tickets SET status = $3,
             resolved_at = CASE WHEN $3 IN ('RESOLVED', 'CLOSED') THEN COALESCE(resolved_at, now()) ELSE NULL END,
             closed_at = CASE WHEN $3 = 'CLOSED' THEN now() ELSE NULL END,
             reopen_count = reopen_count + CASE WHEN $4 THEN 1 ELSE 0 END
         WHERE tenant_id = $1 AND id = $2",
    )
    .bind(t.as_uuid())
    .bind(id)
    .bind(to.as_str())
    .bind(reopen)
    .execute(&mut **tx)
    .await
    .map_err(map_write)?;
    event(
        tx,
        t,
        id,
        if reopen { "reopened" } else { "status_changed" },
        serde_json::json!({ "from": from.as_str(), "to": to.as_str(), "reason": reason }),
        actor,
        actor_type,
    )
    .await?;
    Ok(())
}

// ------------------------------------------------------------------ staf

/// AC-TKT-003: assign/reassign/unassign (`tickets.assign`). OPEN ↔ ASSIGNED otomatis.
pub async fn assign(
    pool: &PgPool,
    actor: &Actor,
    id: Uuid,
    assignee: Option<Uuid>,
    ctx: Ctx<'_>,
) -> Result<(), TicketError> {
    let t = actor.tenant_id;
    let mut tx = tenant_tx(pool, t).await?;
    let (status, before, _) = lock(&mut tx, t, Scope::All, id).await?;
    if status.is_done() {
        return Err(TicketError::InvalidTransition(status.as_str()));
    }
    let next = match (status, assignee) {
        (TicketStatus::Open, Some(_)) => TicketStatus::Assigned,
        (TicketStatus::Assigned, None) => TicketStatus::Open,
        (s, _) => s,
    };
    sqlx::query(
        "UPDATE tickets SET assignee_user_id = $3, status = $4 WHERE tenant_id = $1 AND id = $2",
    )
    .bind(t.as_uuid())
    .bind(id)
    .bind(assignee)
    .bind(next.as_str())
    .execute(&mut *tx)
    .await
    .map_err(map_write)?;
    event(
        &mut tx,
        t,
        id,
        "assigned",
        serde_json::json!({ "from": before, "to": assignee }),
        Some(actor.user_id),
        "staff",
    )
    .await?;
    if let Some(u) = assignee {
        let n: i64 = sqlx::query_scalar("SELECT count(*) FROM ticket_events WHERE tenant_id = $1 AND ticket_id = $2 AND kind = 'assigned'")
            .bind(t.as_uuid())
            .bind(id)
            .fetch_one(&mut *tx)
            .await?;
        notify(
            &mut tx,
            t,
            TICKET_ASSIGNED,
            id,
            format!("ticket_assigned:{id}:{u}:{n}"),
            serde_json::json!({ "assignee": u }),
        )
        .await?;
    }
    audit(
        &mut tx,
        actor,
        "ticket.assigned",
        id,
        serde_json::json!({ "from": before, "to": assignee }),
        ctx,
    )
    .await?;
    tx.commit().await?;
    Ok(())
}

pub async fn set_status(
    pool: &PgPool,
    actor: &Actor,
    scope: Scope,
    id: Uuid,
    to: TicketStatus,
    reason: Option<&str>,
    ctx: Ctx<'_>,
) -> Result<(), TicketError> {
    let t = actor.tenant_id;
    let mut tx = tenant_tx(pool, t).await?;
    let (from, _, since) = lock(&mut tx, t, scope, id).await?;
    apply_status(
        &mut tx,
        t,
        id,
        from,
        to,
        since,
        Some(actor.user_id),
        "staff",
        reason,
    )
    .await?;
    let action = match to {
        TicketStatus::Closed => "ticket.closed",
        TicketStatus::Resolved => "ticket.resolved",
        _ if from.is_reopen(to) => "ticket.reopened",
        _ => "ticket.status_changed",
    };
    audit(
        &mut tx,
        actor,
        action,
        id,
        serde_json::json!({ "from": from.as_str(), "to": to.as_str(), "reason": reason }),
        ctx,
    )
    .await?;
    tx.commit().await?;
    Ok(())
}

pub async fn update_meta(
    pool: &PgPool,
    actor: &Actor,
    scope: Scope,
    id: Uuid,
    priority: Option<Priority>,
    tags: Option<&[String]>,
) -> Result<(), TicketError> {
    let t = actor.tenant_id;
    let mut tx = tenant_tx(pool, t).await?;
    lock(&mut tx, t, scope, id).await?;
    sqlx::query(
        "UPDATE tickets SET priority = COALESCE($3, priority), tags = COALESCE($4, tags) WHERE tenant_id = $1 AND id = $2",
    )
    .bind(t.as_uuid())
    .bind(id)
    .bind(priority.map(Priority::as_str))
    .bind(tags)
    .execute(&mut *tx)
    .await?;
    event(
        &mut tx,
        t,
        id,
        "updated",
        serde_json::json!({ "priority": priority.map(Priority::as_str), "tags": tags }),
        Some(actor.user_id),
        "staff",
    )
    .await?;
    tx.commit().await?;
    Ok(())
}

/// Balasan staf: publik (ke customer) atau catatan internal. Balasan publik pertama mengisi
/// `first_responded_at` (SLA).
pub async fn staff_message(
    pool: &PgPool,
    actor: &Actor,
    scope: Scope,
    id: Uuid,
    internal: bool,
    body: &str,
) -> Result<(), TicketError> {
    let t = actor.tenant_id;
    let mut tx = tenant_tx(pool, t).await?;
    lock(&mut tx, t, scope, id).await?;
    sqlx::query("INSERT INTO ticket_messages (tenant_id, ticket_id, visibility, author_type, author_user_id, body) VALUES ($1, $2, $3, 'staff', $4, $5)")
        .bind(t.as_uuid())
        .bind(id)
        .bind(if internal { "internal" } else { "public" })
        .bind(actor.user_id)
        .bind(body)
        .execute(&mut *tx)
        .await?;
    if !internal {
        sqlx::query("UPDATE tickets SET first_responded_at = COALESCE(first_responded_at, now()) WHERE tenant_id = $1 AND id = $2")
            .bind(t.as_uuid())
            .bind(id)
            .execute(&mut *tx)
            .await?;
    }
    event(
        &mut tx,
        t,
        id,
        if internal {
            "internal_note"
        } else {
            "public_reply"
        },
        serde_json::json!({}),
        Some(actor.user_id),
        "staff",
    )
    .await?;
    tx.commit().await?;
    Ok(())
}

// ------------------------------------------------------------------ customer

/// Balasan customer: WAITING_CUSTOMER → IN_PROGRESS; RESOLVED/CLOSED dibuka kembali sesuai
/// kebijakan (lewat jendela → `ReopenExpired`, customer diminta membuat ticket baru).
pub async fn customer_message(
    pool: &PgPool,
    t: TenantId,
    order: Uuid,
    id: Uuid,
    body: &str,
) -> Result<(), TicketError> {
    let mut tx = tenant_tx(pool, t).await?;
    let (status, _, since) = lock(&mut tx, t, Scope::CustomerOrder(order), id).await?;
    let to = match status {
        TicketStatus::WaitingCustomer | TicketStatus::Resolved | TicketStatus::Closed => {
            Some(TicketStatus::InProgress)
        }
        _ => None,
    };
    if let Some(to) = to {
        apply_status(&mut tx, t, id, status, to, since, None, "customer", None).await?;
    }
    sqlx::query("INSERT INTO ticket_messages (tenant_id, ticket_id, visibility, author_type, body) VALUES ($1, $2, 'public', 'customer', $3)")
        .bind(t.as_uuid())
        .bind(id)
        .bind(body)
        .execute(&mut *tx)
        .await?;
    event(
        &mut tx,
        t,
        id,
        "customer_reply",
        serde_json::json!({}),
        None,
        "customer",
    )
    .await?;
    let n: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM ticket_messages WHERE tenant_id = $1 AND ticket_id = $2",
    )
    .bind(t.as_uuid())
    .bind(id)
    .fetch_one(&mut *tx)
    .await?;
    notify(
        &mut tx,
        t,
        TICKET_CUSTOMER_REPLY,
        id,
        format!("ticket_customer_reply:{id}:{n}"),
        serde_json::json!({}),
    )
    .await?;
    tx.commit().await?;
    Ok(())
}

// ------------------------------------------------------------------ lampiran

#[allow(clippy::too_many_arguments)]
pub async fn add_attachment(
    pool: &PgPool,
    t: TenantId,
    scope: Scope,
    id: Uuid,
    bytes: &[u8],
    max: usize,
    internal: bool,
    uploader: &str,
) -> Result<Uuid, TicketError> {
    // P9-9: lampiran tiket disimpan tanpa metadata.
    let (ty, clean) = sanitize_image(bytes, max)?;
    let bytes = clean.as_slice();
    let mut tx = tenant_tx(pool, t).await?;
    lock(&mut tx, t, scope, id).await?;
    let count: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM ticket_attachments WHERE tenant_id = $1 AND ticket_id = $2",
    )
    .bind(t.as_uuid())
    .bind(id)
    .fetch_one(&mut *tx)
    .await?;
    if count >= 20 {
        return Err(UploadError::QuotaExceeded.into());
    }
    use sha2::Digest as _;
    let aid: Uuid = sqlx::query_scalar(
        "INSERT INTO ticket_attachments (tenant_id, ticket_id, visibility, content_type, byte_size, sha256, bytes, uploaded_by)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8) RETURNING id",
    )
    .bind(t.as_uuid())
    .bind(id)
    .bind(if internal { "internal" } else { "public" })
    .bind(ty.content_type())
    .bind(i32::try_from(bytes.len()).unwrap_or(i32::MAX))
    .bind(sha2::Sha256::digest(bytes).as_slice())
    .bind(bytes)
    .bind(uploader)
    .fetch_one(&mut *tx)
    .await?;
    event(&mut tx, t, id, "attachment_added", serde_json::json!({ "attachment_id": aid, "visibility": if internal { "internal" } else { "public" } }), None, if uploader == "staff" { "staff" } else { "customer" }).await?;
    tx.commit().await?;
    Ok(aid)
}

/// Bytes lampiran sesuai cakupan (customer hanya publik).
pub async fn attachment(
    pool: &PgPool,
    t: TenantId,
    scope: Scope,
    ticket: Uuid,
    aid: Uuid,
) -> Result<Option<(String, Vec<u8>)>, sqlx::Error> {
    let mut tx = tenant_tx(pool, t).await?;
    let (sc, sv) = scope_sql(scope);
    let public_only = matches!(scope, Scope::CustomerOrder(_));
    let r = sqlx::query(sqlx::AssertSqlSafe(format!(
        "SELECT a.content_type, a.bytes FROM ticket_attachments a JOIN tickets t ON t.tenant_id = a.tenant_id AND t.id = a.ticket_id
         WHERE a.tenant_id = $1 AND {sc} AND a.ticket_id = $3 AND a.id = $4 AND (NOT $5 OR a.visibility = 'public')"
    )))
    .bind(t.as_uuid())
    .bind(sv)
    .bind(ticket)
    .bind(aid)
    .bind(public_only)
    .fetch_optional(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(r.map(|r| (r.get("content_type"), r.get("bytes"))))
}
