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
