//! Conversation (P6-1, `PRD.md §4.12`, ADR-0006). Tenant context (RLS). Token sesi chat hanya
//! disimpan hash-nya; pesan & event append-only (dijaga GRANT + trigger DB).

use std::time::Duration;

use sqlx::{PgPool, Row};
use umkm_domain::{
    auth::TokenHash,
    conversation::{ConversationStatus, SESSION_TTL, SenderType},
};
use umkm_shared::ids::TenantId;
use uuid::Uuid;

use crate::{
    auth::{hash_token, new_token},
    pgx::{tenant_tx, violation},
};

#[derive(Debug, thiserror::Error)]
pub enum ConversationError {
    #[error("tidak ditemukan")]
    NotFound,
    #[error("percakapan sudah ditutup")]
    Closed,
    #[error("transisi status tidak diizinkan")]
    InvalidTransition,
    #[error("rujukan bukan milik tenant")]
    InvalidReference,
    #[error("acak tidak tersedia")]
    Random,
    #[error(transparent)]
    Db(#[from] sqlx::Error),
}

fn map_err(e: sqlx::Error) -> ConversationError {
    match violation(&e) {
        Some((s, c)) if s == "23514" && c == "ck_conversation_messages_open" => {
            ConversationError::Closed
        }
        Some((s, c))
            if s == "23514"
                && (c == "ck_conversations_transition" || c == "ck_conversations_initial") =>
        {
            ConversationError::InvalidTransition
        }
        Some((s, _)) if s == "23503" => ConversationError::InvalidReference,
        Some((s, c)) if s == "23514" && c == "ck_conversations_assignee_member" => {
            ConversationError::InvalidReference
        }
        _ => ConversationError::Db(e),
    }
}

#[derive(Debug, Clone)]
pub struct Started {
    pub conversation_id: Uuid,
    pub session_id: Uuid,
    /// Token sesi chat (rahasia pelanggan); hanya hash yang disimpan.
    pub token: String,
}

pub struct StartInput<'a> {
    pub consent_version: &'a str,
    pub customer_id: Option<Uuid>,
    pub initial: ConversationStatus,
}

/// Mulai percakapan + sesi + event `created` dalam satu transaksi.
pub async fn start(
    pool: &PgPool,
    t: TenantId,
    input: &StartInput<'_>,
) -> Result<Started, ConversationError> {
    if !input.initial.is_initial() {
        return Err(ConversationError::InvalidTransition);
    }
    let (token, hash) = new_token().map_err(|_| ConversationError::Random)?;
    let mut tx = tenant_tx(pool, t).await?;
    let conversation_id: Uuid = sqlx::query_scalar(
        "INSERT INTO conversations (tenant_id, status, customer_id, consent_version, consented_at)
         VALUES ($1, $2, $3, $4, now()) RETURNING id",
    )
    .bind(t.as_uuid())
    .bind(input.initial.as_str())
    .bind(input.customer_id)
    .bind(input.consent_version)
    .fetch_one(&mut *tx)
    .await
    .map_err(map_err)?;
    let session_id: Uuid = sqlx::query_scalar(
        "INSERT INTO conversation_sessions (tenant_id, conversation_id, token_hash, expires_at)
         VALUES ($1, $2, $3, now() + make_interval(secs => $4)) RETURNING id",
    )
    .bind(t.as_uuid())
    .bind(conversation_id)
    .bind(&hash.0[..])
    .bind(SESSION_TTL.as_secs_f64())
    .fetch_one(&mut *tx)
    .await
    .map_err(map_err)?;
    sqlx::query(
        "INSERT INTO conversation_events (tenant_id, conversation_id, kind, to_status, actor_type, data)
         VALUES ($1, $2, 'created', $3, 'customer', jsonb_build_object('consent_version', $4::text))",
    )
    .bind(t.as_uuid())
    .bind(conversation_id)
    .bind(input.initial.as_str())
    .bind(input.consent_version)
    .execute(&mut *tx)
    .await
    .map_err(map_err)?;
    tx.commit().await?;
    Ok(Started {
        conversation_id,
        session_id,
        token,
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SessionRef {
    pub conversation_id: Uuid,
    pub session_id: Uuid,
    pub status: ConversationStatus,
}

fn status_of(s: &str) -> ConversationStatus {
    // CHECK DB menjamin hanya 4 nilai.
    ConversationStatus::parse(s).unwrap_or(ConversationStatus::Closed)
}

/// Token → sesi aktif (belum dicabut/kedaluwarsa) milik tenant ini; masa berlaku diperpanjang.
pub async fn resolve_session(
    pool: &PgPool,
    t: TenantId,
    token: &str,
) -> Result<Option<SessionRef>, ConversationError> {
    let TokenHash(hash) = hash_token(token);
    let mut tx = tenant_tx(pool, t).await?;
    let row = sqlx::query(
        "UPDATE conversation_sessions s
            SET last_seen_at = now(), expires_at = now() + make_interval(secs => $3)
           FROM conversations c
          WHERE s.tenant_id = $1 AND s.token_hash = $2 AND s.revoked_at IS NULL AND s.expires_at > now()
            AND c.tenant_id = s.tenant_id AND c.id = s.conversation_id
      RETURNING s.id, s.conversation_id, c.status",
    )
    .bind(t.as_uuid())
    .bind(&hash[..])
    .bind(SESSION_TTL.as_secs_f64())
    .fetch_optional(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(row.map(|r| SessionRef {
        conversation_id: r.get("conversation_id"),
        session_id: r.get("id"),
        status: status_of(r.get::<&str, _>("status")),
    }))
}

pub struct NewMessage<'a> {
    pub sender: SenderType,
    pub session_id: Option<Uuid>,
    pub sender_user_id: Option<Uuid>,
    /// Sudah divalidasi `conversation::validate_body`.
    pub body: &'a str,
    pub ai_run_id: Option<Uuid>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Message {
    pub id: Uuid,
    pub seq: i64,
    pub sender: SenderType,
    pub sender_user_id: Option<Uuid>,
    pub body: String,
    pub ai_run_id: Option<Uuid>,
    /// RFC 3339 UTC (diformat DB, seperti repo lain).
    pub created_at: String,
}

const MESSAGE_COLS: &str = "id, seq, sender_type, sender_user_id, body, ai_run_id,
    to_char(created_at AT TIME ZONE 'UTC', 'YYYY-MM-DD\"T\"HH24:MI:SS.US\"Z\"') AS created_at";

fn message(r: &sqlx::postgres::PgRow) -> Message {
    Message {
        id: r.get("id"),
        seq: r.get("seq"),
        sender: SenderType::parse(r.get::<&str, _>("sender_type")).unwrap_or(SenderType::System),
        sender_user_id: r.get("sender_user_id"),
        body: r.get("body"),
        ai_run_id: r.get("ai_run_id"),
        created_at: r.get("created_at"),
    }
}

/// Tambah pesan. Percakapan CLOSED → `Closed` (ditegakkan trigger).
pub async fn append(
    pool: &PgPool,
    t: TenantId,
    conversation_id: Uuid,
    m: &NewMessage<'_>,
) -> Result<Message, ConversationError> {
    let mut tx = tenant_tx(pool, t).await?;
    let exists: Option<Uuid> =
        sqlx::query_scalar("SELECT id FROM conversations WHERE tenant_id = $1 AND id = $2")
            .bind(t.as_uuid())
            .bind(conversation_id)
            .fetch_optional(&mut *tx)
            .await?;
    if exists.is_none() {
        return Err(ConversationError::NotFound);
    }
    let row = sqlx::query(sqlx::AssertSqlSafe(format!(
        "INSERT INTO conversation_messages
             (tenant_id, conversation_id, session_id, sender_type, sender_user_id, body, ai_run_id)
         VALUES ($1, $2, $3, $4, $5, $6, $7) RETURNING {MESSAGE_COLS}"
    )))
    .bind(t.as_uuid())
    .bind(conversation_id)
    .bind(m.session_id)
    .bind(m.sender.as_str())
    .bind(m.sender_user_id)
    .bind(m.body)
    .bind(m.ai_run_id)
    .fetch_one(&mut *tx)
    .await
    .map_err(map_err)?;
    tx.commit().await?;
    Ok(message(&row))
}

/// Pesan setelah `after_seq` (urut naik), maksimal `limit`.
pub async fn messages(
    pool: &PgPool,
    t: TenantId,
    conversation_id: Uuid,
    after_seq: i64,
    limit: i64,
) -> Result<Vec<Message>, ConversationError> {
    let mut tx = tenant_tx(pool, t).await?;
    let rows = sqlx::query(sqlx::AssertSqlSafe(format!(
        "SELECT {MESSAGE_COLS} FROM conversation_messages
          WHERE tenant_id = $1 AND conversation_id = $2 AND seq > $3
          ORDER BY seq LIMIT $4"
    )))
    .bind(t.as_uuid())
    .bind(conversation_id)
    .bind(after_seq)
    .bind(limit.clamp(1, 200))
    .fetch_all(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(rows.iter().map(message).collect())
}

/// Siapa yang mengubah status.
#[derive(Debug, Clone, Copy)]
pub enum Actor {
    Customer,
    Ai,
    Staff(Uuid),
    System,
}

impl Actor {
    fn parts(self) -> (&'static str, Option<Uuid>) {
        match self {
            Self::Customer => ("customer", None),
            Self::Ai => ("ai", None),
            Self::Staff(u) => ("staff", Some(u)),
            Self::System => ("system", None),
        }
    }
}

fn event_kind(from: ConversationStatus, to: ConversationStatus) -> &'static str {
    use ConversationStatus::*;
    match (from, to) {
        (_, WaitingHuman) => "handover_requested",
        (_, Human) => "taken_over",
        (_, Ai) => "released",
        (_, Closed) => "closed",
    }
}

/// Ubah status + event riwayat. `Human` wajib `Actor::Staff` (menjadi assignee).
pub async fn transition(
    pool: &PgPool,
    t: TenantId,
    conversation_id: Uuid,
    to: ConversationStatus,
    actor: Actor,
) -> Result<ConversationStatus, ConversationError> {
    let mut tx = tenant_tx(pool, t).await?;
    let from: Option<String> = sqlx::query_scalar(
        "SELECT status FROM conversations WHERE tenant_id = $1 AND id = $2 FOR UPDATE",
    )
    .bind(t.as_uuid())
    .bind(conversation_id)
    .fetch_optional(&mut *tx)
    .await?;
    let from = status_of(&from.ok_or(ConversationError::NotFound)?);
    if !from.can_transition(to) {
        return Err(ConversationError::InvalidTransition);
    }
    let (actor_type, actor_user) = actor.parts();
    let assignee = match (to, actor) {
        (ConversationStatus::Human, Actor::Staff(u)) => Some(u),
        (ConversationStatus::Human, _) => return Err(ConversationError::InvalidTransition),
        _ => None,
    };
    sqlx::query(
        "UPDATE conversations
            SET status = $3,
                assignee_user_id = CASE WHEN $3 = 'HUMAN' THEN $4 ELSE assignee_user_id END,
                human_involved = human_involved OR $3 = 'HUMAN',
                closed_at = CASE WHEN $3 = 'CLOSED' THEN now() ELSE NULL END
          WHERE tenant_id = $1 AND id = $2",
    )
    .bind(t.as_uuid())
    .bind(conversation_id)
    .bind(to.as_str())
    .bind(assignee)
    .execute(&mut *tx)
    .await
    .map_err(map_err)?;
    sqlx::query(
        "INSERT INTO conversation_events
             (tenant_id, conversation_id, kind, from_status, to_status, actor_type, actor_user_id)
         VALUES ($1, $2, $3, $4, $5, $6, $7)",
    )
    .bind(t.as_uuid())
    .bind(conversation_id)
    .bind(event_kind(from, to))
    .bind(from.as_str())
    .bind(to.as_str())
    .bind(actor_type)
    .bind(actor_user)
    .execute(&mut *tx)
    .await
    .map_err(map_err)?;
    tx.commit().await?;
    Ok(to)
}

/// Purge retensi ADR-0012 (lintas tenant lewat fungsi SECURITY DEFINER).
pub async fn purge(pool: &PgPool, older_than: Duration) -> Result<i64, sqlx::Error> {
    sqlx::query_scalar("SELECT app.purge_conversations(now() - make_interval(secs => $1))")
        .bind(older_than.as_secs_f64())
        .fetch_one(pool)
        .await
}

/// Ringkasan satu percakapan untuk alur chat.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ConversationRef {
    pub id: Uuid,
    pub status: ConversationStatus,
    pub customer_id: Option<Uuid>,
}

pub async fn get(
    pool: &PgPool,
    t: TenantId,
    conversation_id: Uuid,
) -> Result<Option<ConversationRef>, ConversationError> {
    let mut tx = tenant_tx(pool, t).await?;
    let row = sqlx::query(
        "SELECT id, status, customer_id FROM conversations WHERE tenant_id = $1 AND id = $2",
    )
    .bind(t.as_uuid())
    .bind(conversation_id)
    .fetch_optional(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(row.map(|r| ConversationRef {
        id: r.get("id"),
        status: status_of(r.get::<&str, _>("status")),
        customer_id: r.get("customer_id"),
    }))
}

/// `limit` pesan terakhir, urut naik (riwayat untuk konteks AI).
pub async fn recent(
    pool: &PgPool,
    t: TenantId,
    conversation_id: Uuid,
    limit: i64,
) -> Result<Vec<Message>, ConversationError> {
    let mut tx = tenant_tx(pool, t).await?;
    let rows = sqlx::query(sqlx::AssertSqlSafe(format!(
        "SELECT * FROM (SELECT {MESSAGE_COLS} FROM conversation_messages
          WHERE tenant_id = $1 AND conversation_id = $2 ORDER BY seq DESC LIMIT $3) m ORDER BY seq"
    )))
    .bind(t.as_uuid())
    .bind(conversation_id)
    .bind(limit.clamp(1, 200))
    .fetch_all(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(rows.iter().map(message).collect())
}

/// Sesi masih aktif (dicek ulang per event SSE; tanpa memperpanjang masa berlaku).
pub async fn session_active(
    pool: &PgPool,
    t: TenantId,
    session_id: Uuid,
) -> Result<bool, ConversationError> {
    let mut tx = tenant_tx(pool, t).await?;
    let ok: Option<bool> = sqlx::query_scalar(
        "SELECT true FROM conversation_sessions s JOIN conversations c
             ON c.tenant_id = s.tenant_id AND c.id = s.conversation_id
          WHERE s.tenant_id = $1 AND s.id = $2 AND s.revoked_at IS NULL AND s.expires_at > now()",
    )
    .bind(t.as_uuid())
    .bind(session_id)
    .fetch_optional(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(ok.unwrap_or(false))
}

/// Cabut sesi chat (pelanggan mengakhiri percakapan).
pub async fn revoke_session(
    pool: &PgPool,
    t: TenantId,
    session_id: Uuid,
) -> Result<(), ConversationError> {
    let mut tx = tenant_tx(pool, t).await?;
    sqlx::query("UPDATE conversation_sessions SET revoked_at = now() WHERE tenant_id = $1 AND id = $2 AND revoked_at IS NULL")
        .bind(t.as_uuid())
        .bind(session_id)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(())
}

/// Customer milik akun pelanggan terverifikasi (ADR-0005), bila ada.
pub async fn customer_for_account(
    pool: &PgPool,
    t: TenantId,
    account_id: Uuid,
) -> Result<Option<Uuid>, ConversationError> {
    let mut tx = tenant_tx(pool, t).await?;
    let id: Option<Uuid> = sqlx::query_scalar(
        "SELECT id FROM customers WHERE tenant_id = $1 AND account_id = $2 ORDER BY created_at LIMIT 1",
    )
    .bind(t.as_uuid())
    .bind(account_id)
    .fetch_optional(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(id)
}

// ------------------------------------------------------------------ viewer (P6-3)

fn ts(c: &str) -> String {
    format!("to_char({c} AT TIME ZONE 'UTC', 'YYYY-MM-DD\"T\"HH24:MI:SS.US\"Z\"')")
}

#[derive(Debug, Clone, Default)]
pub struct ListFilter {
    pub status: Option<ConversationStatus>,
    pub assignee: Option<Uuid>,
    pub limit: i64,
    pub offset: i64,
}

#[derive(Debug, Clone)]
pub struct CustomerInfo {
    pub id: Uuid,
    pub name: String,
    pub email: Option<String>,
    pub phone: Option<String>,
    /// Ditautkan dari akun pelanggan terverifikasi (ADR-0005).
    pub has_account: bool,
}

#[derive(Debug, Clone)]
pub struct Summary {
    pub id: Uuid,
    pub channel: String,
    pub status: ConversationStatus,
    pub customer: Option<CustomerInfo>,
    pub assignee_user_id: Option<Uuid>,
    pub assignee_email: Option<String>,
    pub human_involved: bool,
    pub message_count: i64,
    pub last_message: Option<String>,
    pub last_message_at: String,
    pub created_at: String,
}

fn customer_of(r: &sqlx::postgres::PgRow) -> Option<CustomerInfo> {
    r.get::<Option<Uuid>, _>("customer_id")
        .map(|id| CustomerInfo {
            id,
            name: r.get::<Option<String>, _>("c_name").unwrap_or_default(),
            email: r.get("c_email"),
            phone: r.get("c_phone"),
            has_account: r.get::<Option<bool>, _>("c_account").unwrap_or(false),
        })
}

fn summary_cols() -> String {
    format!(
        "c.id, c.channel, c.status, c.customer_id, cu.name AS c_name, cu.email AS c_email, cu.phone AS c_phone,
         (cu.account_id IS NOT NULL) AS c_account, c.assignee_user_id, u.email AS assignee_email, c.human_involved,
         (SELECT count(*) FROM conversation_messages m WHERE m.tenant_id = c.tenant_id AND m.conversation_id = c.id) AS message_count,
         (SELECT m.body FROM conversation_messages m WHERE m.tenant_id = c.tenant_id AND m.conversation_id = c.id
            ORDER BY m.seq DESC LIMIT 1) AS last_message,
         {} AS last_message_at, {} AS created_at",
        ts("c.last_message_at"),
        ts("c.created_at")
    )
}

const SUMMARY_FROM: &str = "FROM conversations c
    LEFT JOIN customers cu ON cu.tenant_id = c.tenant_id AND cu.id = c.customer_id
    LEFT JOIN users u ON u.id = c.assignee_user_id";

fn summary(r: &sqlx::postgres::PgRow) -> Summary {
    Summary {
        id: r.get("id"),
        channel: r.get("channel"),
        status: status_of(r.get::<&str, _>("status")),
        customer: customer_of(r),
        assignee_user_id: r.get("assignee_user_id"),
        assignee_email: r.get("assignee_email"),
        human_involved: r.get("human_involved"),
        message_count: r.get("message_count"),
        last_message: r.get("last_message"),
        last_message_at: r.get("last_message_at"),
        created_at: r.get("created_at"),
    }
}

/// Daftar percakapan tenant (terbaru dulu) + total.
pub async fn list(
    pool: &PgPool,
    t: TenantId,
    f: &ListFilter,
) -> Result<(Vec<Summary>, i64), ConversationError> {
    let mut tx = tenant_tx(pool, t).await?;
    let filter = "WHERE c.tenant_id = $1 AND ($2::text IS NULL OR c.status = $2)
        AND ($3::uuid IS NULL OR c.assignee_user_id = $3)";
    let total: i64 = sqlx::query_scalar(sqlx::AssertSqlSafe(format!(
        "SELECT count(*) FROM conversations c {filter}"
    )))
    .bind(t.as_uuid())
    .bind(f.status.map(|s| s.as_str()))
    .bind(f.assignee)
    .fetch_one(&mut *tx)
    .await?;
    let rows = sqlx::query(sqlx::AssertSqlSafe(format!(
        "SELECT {} {SUMMARY_FROM} {filter} ORDER BY c.last_message_at DESC, c.id LIMIT $4 OFFSET $5",
        summary_cols()
    )))
    .bind(t.as_uuid())
    .bind(f.status.map(|s| s.as_str()))
    .bind(f.assignee)
    .bind(f.limit.clamp(1, 100))
    .bind(f.offset.max(0))
    .fetch_all(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok((rows.iter().map(summary).collect(), total))
}

#[derive(Debug, Clone)]
pub struct ToolCallInfo {
    pub name: String,
    pub status: String,
    pub args_summary: serde_json::Value,
}

/// Jejak AI satu pesan (provider/model, token, latensi, tool).
#[derive(Debug, Clone)]
pub struct AiRunInfo {
    pub id: Uuid,
    pub provider_name: Option<String>,
    pub source: String,
    pub model: String,
    pub status: String,
    pub decision: Option<String>,
    pub error_code: Option<String>,
    pub prompt_tokens: i32,
    pub completion_tokens: i32,
    pub latency_ms: i32,
    pub tool_calls: Vec<ToolCallInfo>,
}

#[derive(Debug, Clone)]
pub struct TimelineMessage {
    pub message: Message,
    pub sender_email: Option<String>,
    pub ai: Option<AiRunInfo>,
}

#[derive(Debug, Clone)]
pub struct EventInfo {
    pub kind: String,
    pub from_status: Option<String>,
    pub to_status: Option<String>,
    pub actor_type: String,
    pub actor_user_id: Option<Uuid>,
    pub actor_email: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Clone)]
pub struct ApprovalInfo {
    pub id: Uuid,
    pub status: String,
    pub action: String,
    pub summary: String,
    pub current_step: i32,
    pub created_at: String,
}

#[derive(Debug, Clone)]
pub struct Detail {
    pub summary: Summary,
    pub consent_version: String,
    pub consented_at: String,
    pub closed_at: Option<String>,
    pub messages: Vec<TimelineMessage>,
    pub events: Vec<EventInfo>,
    pub approvals: Vec<ApprovalInfo>,
    /// Run AI tanpa pesan (mis. handover tanpa balasan) tetap terlihat untuk audit biaya.
    pub runs_without_message: Vec<AiRunInfo>,
}

/// Viewer satu percakapan (FR-CNV-002). Semua query di tenant context (RLS).
pub async fn detail(
    pool: &PgPool,
    t: TenantId,
    id: Uuid,
) -> Result<Option<Detail>, ConversationError> {
    let mut tx = tenant_tx(pool, t).await?;
    let Some(row) = sqlx::query(sqlx::AssertSqlSafe(format!(
        "SELECT {}, c.consent_version, {} AS consented_at, {} AS closed_at {SUMMARY_FROM}
          WHERE c.tenant_id = $1 AND c.id = $2",
        summary_cols(),
        ts("c.consented_at"),
        ts("c.closed_at")
    )))
    .bind(t.as_uuid())
    .bind(id)
    .fetch_optional(&mut *tx)
    .await?
    else {
        return Ok(None);
    };
    let runs = sqlx::query(sqlx::AssertSqlSafe(
        "SELECT r.id, p.name AS provider_name, r.source, r.model, r.status, r.decision, r.error_code,
                r.prompt_tokens, r.completion_tokens, r.latency_ms
           FROM ai_runs r LEFT JOIN ai_providers p ON p.tenant_id = r.tenant_id AND p.id = r.provider_id
          WHERE r.tenant_id = $1 AND r.conversation_id = $2 ORDER BY r.created_at"
            .to_owned(),
    ))
    .bind(t.as_uuid())
    .bind(id)
    .fetch_all(&mut *tx)
    .await?;
    let tools = sqlx::query(
        "SELECT tc.ai_run_id, tc.tool_name, tc.result_status, tc.args_summary
           FROM ai_tool_calls tc JOIN ai_runs r ON r.tenant_id = tc.tenant_id AND r.id = tc.ai_run_id
          WHERE tc.tenant_id = $1 AND r.conversation_id = $2 ORDER BY tc.created_at",
    )
    .bind(t.as_uuid())
    .bind(id)
    .fetch_all(&mut *tx)
    .await?;
    let mut run_infos: Vec<AiRunInfo> = runs
        .iter()
        .map(|r| {
            let rid: Uuid = r.get("id");
            AiRunInfo {
                id: rid,
                provider_name: r.get("provider_name"),
                source: r.get("source"),
                model: r.get("model"),
                status: r.get("status"),
                decision: r.get("decision"),
                error_code: r.get("error_code"),
                prompt_tokens: r.get("prompt_tokens"),
                completion_tokens: r.get("completion_tokens"),
                latency_ms: r.get("latency_ms"),
                tool_calls: tools
                    .iter()
                    .filter(|x| x.get::<Uuid, _>("ai_run_id") == rid)
                    .map(|x| ToolCallInfo {
                        name: x.get("tool_name"),
                        status: x.get("result_status"),
                        args_summary: x.get("args_summary"),
                    })
                    .collect(),
            }
        })
        .collect();
    let msgs = sqlx::query(sqlx::AssertSqlSafe(format!(
        "SELECT {MESSAGE_COLS}, (SELECT email FROM users u WHERE u.id = sender_user_id) AS sender_email
           FROM conversation_messages WHERE tenant_id = $1 AND conversation_id = $2 ORDER BY seq"
    )))
    .bind(t.as_uuid())
    .bind(id)
    .fetch_all(&mut *tx)
    .await?;
    let messages: Vec<TimelineMessage> = msgs
        .iter()
        .map(|r| {
            let m = message(r);
            let ai = m
                .ai_run_id
                .and_then(|rid| run_infos.iter().find(|x| x.id == rid).cloned());
            TimelineMessage {
                sender_email: r.get("sender_email"),
                ai,
                message: m,
            }
        })
        .collect();
    run_infos.retain(|r| !messages.iter().any(|m| m.message.ai_run_id == Some(r.id)));
    let events = sqlx::query(sqlx::AssertSqlSafe(format!(
        "SELECT e.kind, e.from_status, e.to_status, e.actor_type, e.actor_user_id, u.email AS actor_email,
                {} AS created_at
           FROM conversation_events e LEFT JOIN users u ON u.id = e.actor_user_id
          WHERE e.tenant_id = $1 AND e.conversation_id = $2 ORDER BY e.seq",
        ts("e.created_at")
    )))
    .bind(t.as_uuid())
    .bind(id)
    .fetch_all(&mut *tx)
    .await?
    .iter()
    .map(|r| EventInfo {
        kind: r.get("kind"),
        from_status: r.get("from_status"),
        to_status: r.get("to_status"),
        actor_type: r.get("actor_type"),
        actor_user_id: r.get("actor_user_id"),
        actor_email: r.get("actor_email"),
        created_at: r.get("created_at"),
    })
    .collect();
    let approvals = sqlx::query(sqlx::AssertSqlSafe(format!(
        "SELECT id, status, action, summary, current_step, {} AS created_at FROM approval_requests
          WHERE tenant_id = $1 AND entity_type = 'ai_response' AND entity_id = $2 ORDER BY created_at",
        ts("created_at")
    )))
    .bind(t.as_uuid())
    .bind(id)
    .fetch_all(&mut *tx)
    .await?
    .iter()
    .map(|r| ApprovalInfo {
        id: r.get("id"),
        status: r.get("status"),
        action: r.get("action"),
        summary: r.get("summary"),
        current_step: r.get("current_step"),
        created_at: r.get("created_at"),
    })
    .collect();
    tx.commit().await?;
    Ok(Some(Detail {
        summary: summary(&row),
        consent_version: row.get("consent_version"),
        consented_at: row.get("consented_at"),
        closed_at: row.get("closed_at"),
        messages,
        events,
        approvals,
        runs_without_message: run_infos,
    }))
}
