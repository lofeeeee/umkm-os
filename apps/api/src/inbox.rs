//! Conversation Viewer (P6-3, `PRD.md §4.12`, FR-CNV-002/003). Tenant dari path + membership.
//!
//! - Butuh `conversations.read`.
//! - Tanpa masking hanya bila pembaca ber-`conversations.reply` DAN `customers.read` (staf yang
//!   menangani chat). Selain itu (mis. role Viewer, AC-CNV-003): kontak pelanggan, email staf, email &
//!   deret ≥ 8 digit di isi pesan disamarkan, dan ringkasan argumen tool tidak ditampilkan.

use axum::{
    Json, Router,
    extract::{Path, Query, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::{get, post},
};
use serde::{Deserialize, Serialize};
use umkm_domain::{
    conversation::{ConversationStatus, mask},
    rbac::Actor,
};
use umkm_infrastructure::conversation_repo::{
    self as repo, Actor as ConvActor, AiRunInfo, ConversationError, CustomerInfo, ListFilter,
    Summary,
};
use umkm_shared::http::SuccessEnvelope;
use utoipa::{IntoParams, ToSchema};
use uuid::Uuid;

use crate::{
    ApiError, AppState,
    approvals::{Gate, gate, pending_response},
    auth::ClientIp,
    chat::{ChatMessageView, append_and_emit, emit_status},
    http_util::{Page, conflict, internal, not_found, pool},
    middleware::ReqId,
    session::Member,
    validation::one,
};

type Ok<T> = Result<Json<SuccessEnvelope<T>>, ApiError>;

/// Pembaca berhak melihat data tanpa masking.
pub fn sees_full(actor: &Actor) -> bool {
    actor.has("conversations.reply") && actor.has("customers.read")
}

fn repo_err(e: ConversationError) -> ApiError {
    match e {
        ConversationError::NotFound => not_found(),
        ConversationError::Closed => conflict("conversation_closed", "Percakapan sudah ditutup"),
        ConversationError::InvalidTransition => {
            conflict("invalid_transition", "Status percakapan tidak dapat diubah")
        }
        ConversationError::DraftDecided => conflict("draft_decided", "Draf sudah diputus"),
        e => internal(e),
    }
}

#[derive(Debug, Serialize, ToSchema)]
pub struct CustomerContext {
    pub id: Uuid,
    pub name: String,
    pub email: Option<String>,
    pub phone: Option<String>,
    pub has_account: bool,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct ConversationSummaryView {
    pub id: Uuid,
    pub channel: String,
    /// `AI` | `WAITING_HUMAN` | `HUMAN` | `CLOSED`
    pub status: String,
    pub customer: Option<CustomerContext>,
    pub assignee_user_id: Option<Uuid>,
    pub assignee_email: Option<String>,
    pub human_involved: bool,
    pub message_count: i64,
    pub last_message: Option<String>,
    pub last_message_at: String,
    pub created_at: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct ConversationList {
    pub items: Vec<ConversationSummaryView>,
    pub total: i64,
    /// Data sensitif disamarkan untuk pembaca ini.
    pub masked: bool,
    /// Ringkasan antrean inbox (seluruh tenant, bukan hanya halaman ini).
    pub counts: InboxCounts,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct InboxCounts {
    pub ai: i64,
    pub waiting_human: i64,
    pub human: i64,
    pub closed: i64,
    /// HUMAN yang ditangani pemanggil
    pub mine: i64,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct ToolCallView {
    pub name: String,
    /// `ok` | `denied` | `invalid` | `error`
    pub status: String,
    /// Ringkasan argumen tervalidasi; `null` bila disamarkan.
    #[schema(value_type = Option<Object>)]
    pub args_summary: Option<serde_json::Value>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct AiRunView {
    pub run_id: Uuid,
    pub provider_name: Option<String>,
    /// `platform` | `byok`
    pub source: String,
    pub model: String,
    pub status: String,
    pub decision: Option<String>,
    pub error_code: Option<String>,
    pub prompt_tokens: i32,
    pub completion_tokens: i32,
    pub latency_ms: i32,
    pub tool_calls: Vec<ToolCallView>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct TimelineMessageView {
    pub id: Uuid,
    pub seq: i64,
    /// `CUSTOMER` | `AI` | `ADMIN` | `SYSTEM`
    pub sender: String,
    pub sender_user_id: Option<Uuid>,
    pub sender_email: Option<String>,
    pub body: String,
    pub created_at: String,
    pub ai: Option<AiRunView>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct HistoryEventView {
    /// `created` | `assigned` | `unassigned` | `handover_requested` | `taken_over` | `released` | `closed`
    pub kind: String,
    pub from_status: Option<String>,
    pub to_status: Option<String>,
    /// `customer` | `ai` | `staff` | `system`
    pub actor_type: String,
    pub actor_user_id: Option<Uuid>,
    pub actor_email: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct ApprovalView {
    pub id: Uuid,
    pub status: String,
    pub action: String,
    pub summary: String,
    pub current_step: i32,
    pub created_at: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct ConversationDetailView {
    #[serde(flatten)]
    pub summary: ConversationSummaryView,
    pub consent_version: String,
    pub consented_at: String,
    pub closed_at: Option<String>,
    pub timeline: Vec<TimelineMessageView>,
    /// Run AI tanpa pesan terkirim (mis. handover / antre staf).
    pub other_ai_runs: Vec<AiRunView>,
    pub history: Vec<HistoryEventView>,
    pub approvals: Vec<ApprovalView>,
    /// Draft balasan AI (mode Assisted); tidak terlihat pelanggan.
    pub drafts: Vec<DraftView>,
    /// Ticket dari percakapan ini (ticket ≠ conversation, FR-TKT-006).
    pub tickets: Vec<LinkedTicketView>,
    pub masked: bool,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct LinkedTicketView {
    pub id: Uuid,
    pub code: String,
    pub subject: String,
    pub status: String,
    pub created_at: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct DraftView {
    pub id: Uuid,
    pub body: String,
    /// `pending` | `approval_pending` | `sent` | `discarded`
    pub status: String,
    pub ai_run_id: Option<Uuid>,
    pub decided_by_user_id: Option<Uuid>,
    pub sent_message_id: Option<Uuid>,
    pub created_at: String,
}

struct Masker {
    full: bool,
}

impl Masker {
    fn text(&self, s: String) -> String {
        if self.full { s } else { mask::text(&s) }
    }
    fn email(&self, s: Option<String>) -> Option<String> {
        if self.full {
            s
        } else {
            s.map(|e| mask::email(&e))
        }
    }
    fn customer(&self, c: CustomerInfo) -> CustomerContext {
        CustomerContext {
            id: c.id,
            name: if self.full {
                c.name
            } else {
                mask::name(&c.name)
            },
            email: self.email(c.email),
            phone: if self.full {
                c.phone
            } else {
                c.phone.map(|p| mask::phone(&p))
            },
            has_account: c.has_account,
        }
    }
    fn summary(&self, s: Summary) -> ConversationSummaryView {
        ConversationSummaryView {
            id: s.id,
            channel: s.channel,
            status: s.status.as_str().into(),
            customer: s.customer.map(|c| self.customer(c)),
            assignee_user_id: s.assignee_user_id,
            assignee_email: self.email(s.assignee_email),
            human_involved: s.human_involved,
            message_count: s.message_count,
            last_message: s.last_message.map(|m| self.text(m)),
            last_message_at: s.last_message_at,
            created_at: s.created_at,
        }
    }
    fn run(&self, r: AiRunInfo) -> AiRunView {
        AiRunView {
            run_id: r.id,
            provider_name: r.provider_name,
            source: r.source,
            model: r.model,
            status: r.status,
            decision: r.decision,
            error_code: r.error_code,
            prompt_tokens: r.prompt_tokens,
            completion_tokens: r.completion_tokens,
            latency_ms: r.latency_ms,
            tool_calls: r
                .tool_calls
                .into_iter()
                .map(|c| ToolCallView {
                    name: c.name,
                    status: c.status,
                    args_summary: self.full.then_some(c.args_summary),
                })
                .collect(),
        }
    }
}

#[derive(Debug, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct ConversationQuery {
    /// `AI` | `WAITING_HUMAN` | `HUMAN` | `CLOSED`
    pub status: Option<String>,
    pub assignee_user_id: Option<Uuid>,
    /// `true` = hanya yang ditangani pemanggil (mengabaikan `assignee_user_id`).
    pub mine: Option<bool>,
    pub limit: Option<i64>,
    pub offset: Option<i64>,
}

/// Daftar percakapan (terbaru dulu). Butuh `conversations.read`.
#[utoipa::path(
    get, path = "/api/v1/tenants/{tenant_id}/conversations", tag = "conversations",
    params(("tenant_id" = Uuid, Path), ConversationQuery),
    responses(
        (status = 200, description = "Daftar; `masked` = data sensitif disamarkan", body = SuccessEnvelope<ConversationList>),
        (status = 403, description = "Permission kurang", body = umkm_shared::http::ErrorEnvelope),
        (status = 404, description = "Bukan anggota", body = umkm_shared::http::ErrorEnvelope),
        (status = 422, description = "Filter tidak valid", body = umkm_shared::http::ErrorEnvelope)
    )
)]
pub async fn list(
    State(state): State<AppState>,
    ReqId(id): ReqId,
    Member(actor): Member,
    Query(q): Query<ConversationQuery>,
) -> Ok<ConversationList> {
    actor.require("conversations.read")?;
    let status = match q.status.as_deref() {
        None | Some("") => None,
        Some(s) => {
            Some(ConversationStatus::parse(s).ok_or_else(|| one("status", "invalid_status"))?)
        }
    };
    let (limit, offset) = Page {
        limit: q.limit,
        offset: q.offset,
    }
    .bounds();
    let (items, total) = repo::list(
        pool(&state)?,
        actor.tenant_id,
        &ListFilter {
            status,
            assignee: if q.mine == Some(true) {
                Some(actor.user_id)
            } else {
                q.assignee_user_id
            },
            // Antrean menunggu staf dilayani FIFO.
            oldest_first: status == Some(ConversationStatus::WaitingHuman),
            limit,
            offset,
        },
    )
    .await
    .map_err(repo_err)?;
    let m = Masker {
        full: sees_full(&actor),
    };
    Ok(Json(SuccessEnvelope::new(
        ConversationList {
            items: items.into_iter().map(|s| m.summary(s)).collect(),
            total,
            masked: !m.full,
            counts: {
                let c = repo::counts(pool(&state)?, actor.tenant_id, actor.user_id)
                    .await
                    .map_err(repo_err)?;
                InboxCounts {
                    ai: c.ai,
                    waiting_human: c.waiting_human,
                    human: c.human,
                    closed: c.closed,
                    mine: c.mine,
                }
            },
        },
        &id,
    )))
}

/// Viewer satu percakapan: timeline + jejak AI + riwayat + approval. Butuh `conversations.read`.
#[utoipa::path(
    get, path = "/api/v1/tenants/{tenant_id}/conversations/{conversation_id}", tag = "conversations",
    params(("tenant_id" = Uuid, Path), ("conversation_id" = Uuid, Path)),
    responses(
        (status = 200, description = "Detail percakapan", body = SuccessEnvelope<ConversationDetailView>),
        (status = 403, description = "Permission kurang", body = umkm_shared::http::ErrorEnvelope),
        (status = 404, description = "Tidak ada di tenant ini", body = umkm_shared::http::ErrorEnvelope)
    )
)]
pub async fn detail(
    State(state): State<AppState>,
    ReqId(id): ReqId,
    Member(actor): Member,
    Path((_, conversation_id)): Path<(Uuid, Uuid)>,
) -> Ok<ConversationDetailView> {
    actor.require("conversations.read")?;
    let d = repo::detail(pool(&state)?, actor.tenant_id, conversation_id)
        .await
        .map_err(repo_err)?
        .ok_or_else(not_found)?;
    let m = Masker {
        full: sees_full(&actor),
    };
    let view = ConversationDetailView {
        consent_version: d.consent_version,
        consented_at: d.consented_at,
        closed_at: d.closed_at,
        timeline: d
            .messages
            .into_iter()
            .map(|t| TimelineMessageView {
                id: t.message.id,
                seq: t.message.seq,
                sender: t.message.sender.as_str().into(),
                sender_user_id: t.message.sender_user_id,
                sender_email: m.email(t.sender_email),
                body: m.text(t.message.body),
                created_at: t.message.created_at,
                ai: t.ai.map(|r| m.run(r)),
            })
            .collect(),
        other_ai_runs: d
            .runs_without_message
            .into_iter()
            .map(|r| m.run(r))
            .collect(),
        history: d
            .events
            .into_iter()
            .map(|e| HistoryEventView {
                kind: e.kind,
                from_status: e.from_status,
                to_status: e.to_status,
                actor_type: e.actor_type,
                actor_user_id: e.actor_user_id,
                actor_email: m.email(e.actor_email),
                created_at: e.created_at,
            })
            .collect(),
        approvals: d
            .approvals
            .into_iter()
            .map(|a| ApprovalView {
                id: a.id,
                status: a.status,
                action: a.action,
                summary: m.text(a.summary),
                current_step: a.current_step,
                created_at: a.created_at,
            })
            .collect(),
        drafts: repo::drafts(pool(&state)?, actor.tenant_id, conversation_id)
            .await
            .map_err(repo_err)?
            .into_iter()
            .map(|x| DraftView {
                id: x.id,
                body: m.text(x.body),
                status: x.status,
                ai_run_id: x.ai_run_id,
                decided_by_user_id: x.decided_by_user_id,
                sent_message_id: x.sent_message_id,
                created_at: x.created_at,
            })
            .collect(),
        tickets: repo::tickets(pool(&state)?, actor.tenant_id, conversation_id)
            .await
            .map_err(repo_err)?
            .into_iter()
            .map(|x| LinkedTicketView {
                id: x.id,
                code: x.code,
                subject: m.text(x.subject),
                status: x.status,
                created_at: x.created_at,
            })
            .collect(),
        masked: !m.full,
        summary: m.summary(d.summary),
    };
    Ok(Json(SuccessEnvelope::new(view, &id)))
}

// ------------------------------------------------------------------ aksi staf (P6-4)

type Ctx<'a> = (Option<&'a str>, Option<&'a str>);

#[derive(Debug, Serialize, ToSchema)]
pub struct StaffActionView {
    /// Status percakapan setelah aksi
    pub status: String,
    /// Pesan yang terkirim ke pelanggan (bila ada)
    pub message: Option<ChatMessageView>,
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct StaffReply {
    pub body: String,
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct SendDraft {
    /// Teks yang dikirim; kosong = teks draft apa adanya (terkirim sebagai pesan AI).
    pub body: Option<String>,
}

fn assigned_to_other() -> ApiError {
    conflict("assigned_to_other", "Percakapan sedang ditangani staf lain")
}

async fn load(
    state: &AppState,
    actor: &Actor,
    conv: Uuid,
) -> Result<repo::ConversationRef, ApiError> {
    repo::get(pool(state)?, actor.tenant_id, conv)
        .await
        .map_err(repo_err)?
        .ok_or_else(not_found)
}

/// Pastikan pelaku menangani percakapan: AI/menunggu → ambil alih (butuh `conversations.takeover`);
/// HUMAN milik staf lain → 409. Takeover menghentikan auto-response AI.
async fn ensure_handling(
    state: &AppState,
    actor: &Actor,
    c: &repo::ConversationRef,
) -> Result<(), ApiError> {
    match c.status {
        ConversationStatus::Closed => {
            Err(conflict("conversation_closed", "Percakapan sudah ditutup"))
        }
        ConversationStatus::Human if c.assignee_user_id == Some(actor.user_id) => Ok(()),
        ConversationStatus::Human => Err(assigned_to_other()),
        ConversationStatus::Ai | ConversationStatus::WaitingHuman => {
            actor.require("conversations.takeover")?;
            repo::transition(
                pool(state)?,
                actor.tenant_id,
                c.id,
                ConversationStatus::Human,
                ConvActor::Staff(actor.user_id),
            )
            .await
            .map_err(repo_err)?;
            emit_status(state, actor.tenant_id, c.id, ConversationStatus::Human).await;
            Ok(())
        }
    }
}

fn action_view(
    status: ConversationStatus,
    message: Option<&repo::Message>,
    rid: &umkm_shared::http::RequestId,
) -> Json<SuccessEnvelope<StaffActionView>> {
    Json(SuccessEnvelope::new(
        StaffActionView {
            status: status.as_str().into(),
            message: message.map(ChatMessageView::from),
        },
        rid,
    ))
}

/// Ambil alih percakapan (AI berhenti membalas otomatis). Butuh `conversations.takeover`.
#[utoipa::path(
    post, path = "/api/v1/tenants/{tenant_id}/conversations/{conversation_id}/takeover", tag = "conversations",
    params(("tenant_id" = Uuid, Path), ("conversation_id" = Uuid, Path)),
    responses(
        (status = 200, description = "Ditangani pelaku (HUMAN)", body = SuccessEnvelope<StaffActionView>),
        (status = 403, description = "Permission kurang", body = umkm_shared::http::ErrorEnvelope),
        (status = 404, description = "Tidak ada di tenant ini", body = umkm_shared::http::ErrorEnvelope),
        (status = 409, description = "Ditutup / ditangani staf lain", body = umkm_shared::http::ErrorEnvelope)
    )
)]
pub async fn takeover(
    State(state): State<AppState>,
    ReqId(id): ReqId,
    Member(actor): Member,
    Path((_, conv)): Path<(Uuid, Uuid)>,
) -> Ok<StaffActionView> {
    actor.require("conversations.takeover")?;
    let c = load(&state, &actor, conv).await?;
    ensure_handling(&state, &actor, &c).await?;
    Ok(action_view(ConversationStatus::Human, None, &id))
}

/// Kembalikan percakapan ke asisten AI. Butuh `conversations.takeover`; hanya penangan saat ini.
#[utoipa::path(
    post, path = "/api/v1/tenants/{tenant_id}/conversations/{conversation_id}/release", tag = "conversations",
    params(("tenant_id" = Uuid, Path), ("conversation_id" = Uuid, Path)),
    responses(
        (status = 200, description = "Kembali ke AI", body = SuccessEnvelope<StaffActionView>),
        (status = 409, description = "Status tidak dapat diubah / staf lain", body = umkm_shared::http::ErrorEnvelope)
    )
)]
pub async fn release(
    State(state): State<AppState>,
    ReqId(id): ReqId,
    Member(actor): Member,
    Path((_, conv)): Path<(Uuid, Uuid)>,
) -> Ok<StaffActionView> {
    actor.require("conversations.takeover")?;
    let c = load(&state, &actor, conv).await?;
    if c.status == ConversationStatus::Human && c.assignee_user_id != Some(actor.user_id) {
        return Err(assigned_to_other());
    }
    repo::transition(
        pool(&state)?,
        actor.tenant_id,
        conv,
        ConversationStatus::Ai,
        ConvActor::Staff(actor.user_id),
    )
    .await
    .map_err(repo_err)?;
    emit_status(&state, actor.tenant_id, conv, ConversationStatus::Ai).await;
    Ok(action_view(ConversationStatus::Ai, None, &id))
}

/// Balasan staf ke pelanggan (ADMIN). Dari status AI/menunggu = ambil alih otomatis.
#[utoipa::path(
    post, path = "/api/v1/tenants/{tenant_id}/conversations/{conversation_id}/messages", tag = "conversations",
    params(("tenant_id" = Uuid, Path), ("conversation_id" = Uuid, Path)), request_body = StaffReply,
    responses(
        (status = 200, description = "Terkirim (juga lewat SSE pelanggan)", body = SuccessEnvelope<StaffActionView>),
        (status = 403, description = "Permission kurang", body = umkm_shared::http::ErrorEnvelope),
        (status = 409, description = "Ditutup / ditangani staf lain", body = umkm_shared::http::ErrorEnvelope),
        (status = 422, description = "Isi tidak valid", body = umkm_shared::http::ErrorEnvelope)
    )
)]
pub async fn reply(
    State(state): State<AppState>,
    ReqId(id): ReqId,
    Member(actor): Member,
    Path((_, conv)): Path<(Uuid, Uuid)>,
    Json(req): Json<StaffReply>,
) -> Ok<StaffActionView> {
    actor.require("conversations.reply")?;
    let body =
        umkm_domain::conversation::validate_body(&req.body).map_err(|e| one("body", e.code()))?;
    let c = load(&state, &actor, conv).await?;
    ensure_handling(&state, &actor, &c).await?;
    let m = append_and_emit(
        &state,
        actor.tenant_id,
        conv,
        &repo::NewMessage {
            sender: umkm_domain::conversation::SenderType::Admin,
            session_id: None,
            sender_user_id: Some(actor.user_id),
            body: &body,
            ai_run_id: None,
        },
    )
    .await?;
    Ok(action_view(ConversationStatus::Human, Some(&m), &id))
}

/// Tutup percakapan. Butuh `conversations.reply`.
#[utoipa::path(
    post, path = "/api/v1/tenants/{tenant_id}/conversations/{conversation_id}/close", tag = "conversations",
    params(("tenant_id" = Uuid, Path), ("conversation_id" = Uuid, Path)),
    responses(
        (status = 200, description = "Ditutup", body = SuccessEnvelope<StaffActionView>),
        (status = 409, description = "Sudah ditutup / staf lain", body = umkm_shared::http::ErrorEnvelope)
    )
)]
pub async fn close(
    State(state): State<AppState>,
    ReqId(id): ReqId,
    Member(actor): Member,
    Path((_, conv)): Path<(Uuid, Uuid)>,
) -> Ok<StaffActionView> {
    actor.require("conversations.reply")?;
    let c = load(&state, &actor, conv).await?;
    if c.status == ConversationStatus::Human && c.assignee_user_id != Some(actor.user_id) {
        return Err(assigned_to_other());
    }
    repo::transition(
        pool(&state)?,
        actor.tenant_id,
        conv,
        ConversationStatus::Closed,
        ConvActor::Staff(actor.user_id),
    )
    .await
    .map_err(repo_err)?;
    emit_status(&state, actor.tenant_id, conv, ConversationStatus::Closed).await;
    Ok(action_view(ConversationStatus::Closed, None, &id))
}

/// Kirim draft (setelah approval engine bila berlaku). Dipakai handler & executor approval.
pub async fn apply_send_draft(
    state: &AppState,
    actor: &Actor,
    conv: Uuid,
    draft: Uuid,
    body: &str,
) -> Result<repo::Message, ApiError> {
    let c = load(state, actor, conv).await?;
    ensure_handling(state, actor, &c).await?;
    let m = repo::decide_draft(
        pool(state)?,
        actor.tenant_id,
        draft,
        actor.user_id,
        Some(body),
    )
    .await
    .map_err(repo_err)?
    .ok_or_else(|| internal("draft tanpa pesan"))?;
    crate::realtime::emit(
        state,
        crate::chat::staff_event(
            actor.tenant_id,
            crate::chat::EVENT_MESSAGE,
            conv,
            serde_json::to_value(ChatMessageView::from(&m)).unwrap_or_default(),
        ),
    )
    .await;
    Ok(m)
}

/// Setujui & kirim draft AI (boleh diedit). Kebijakan approval `ai_response` → 202 menunggu persetujuan.
#[utoipa::path(
    post, path = "/api/v1/tenants/{tenant_id}/conversations/{conversation_id}/drafts/{draft_id}/send", tag = "conversations",
    params(("tenant_id" = Uuid, Path), ("conversation_id" = Uuid, Path), ("draft_id" = Uuid, Path)), request_body = SendDraft,
    responses(
        (status = 200, description = "Terkirim ke pelanggan", body = SuccessEnvelope<StaffActionView>),
        (status = 202, description = "Menunggu approval (`approval_request_id`)", body = SuccessEnvelope<crate::approvals::PendingApproval>),
        (status = 403, description = "Permission kurang", body = umkm_shared::http::ErrorEnvelope),
        (status = 404, description = "Draft tidak ada di percakapan ini", body = umkm_shared::http::ErrorEnvelope),
        (status = 409, description = "Draft sudah diputus / percakapan ditutup / staf lain", body = umkm_shared::http::ErrorEnvelope),
        (status = 422, description = "Isi tidak valid", body = umkm_shared::http::ErrorEnvelope)
    )
)]
pub async fn send_draft(
    State(state): State<AppState>,
    ReqId(id): ReqId,
    ClientIp(ip): ClientIp,
    Member(actor): Member,
    Path((_, conv, draft_id)): Path<(Uuid, Uuid, Uuid)>,
    Json(req): Json<SendDraft>,
) -> Result<Response, ApiError> {
    actor.require("conversations.reply")?;
    let d = repo::draft(pool(&state)?, actor.tenant_id, conv, draft_id)
        .await
        .map_err(repo_err)?
        .ok_or_else(not_found)?;
    if d.status != "pending" {
        return Err(conflict("draft_decided", "Draf sudah diputus"));
    }
    let body = umkm_domain::conversation::validate_body(
        req.body
            .as_deref()
            .filter(|b| !b.trim().is_empty())
            .unwrap_or(&d.body),
    )
    .map_err(|e| one("body", e.code()))?;
    let c = load(&state, &actor, conv).await?;
    if c.status == ConversationStatus::Closed {
        return Err(conflict("conversation_closed", "Percakapan sudah ditutup"));
    }
    let ctx: Ctx<'_> = (Some(id.as_str()), Some(&ip));
    match gate(
        &state,
        &actor,
        umkm_domain::approvals::EntityType::AiResponse,
        Some(conv),
        "ai_response.send",
        serde_json::json!({ "conversation_id": conv, "draft_id": draft_id, "body": body }),
        "Kirim balasan AI ke pelanggan",
        ctx,
    )
    .await?
    {
        Gate::Pending(req_id) => {
            repo::mark_draft_approval(
                pool(&state)?,
                actor.tenant_id,
                draft_id,
                actor.user_id,
                req_id,
            )
            .await
            .map_err(repo_err)?;
            Ok(pending_response(req_id, &id))
        }
        Gate::Direct => {
            let m = apply_send_draft(&state, &actor, conv, draft_id, &body).await?;
            Ok(action_view(ConversationStatus::Human, Some(&m), &id).into_response())
        }
    }
}

/// Buang draft AI. Butuh `conversations.reply`.
#[utoipa::path(
    post, path = "/api/v1/tenants/{tenant_id}/conversations/{conversation_id}/drafts/{draft_id}/discard", tag = "conversations",
    params(("tenant_id" = Uuid, Path), ("conversation_id" = Uuid, Path), ("draft_id" = Uuid, Path)),
    responses(
        (status = 204, description = "Dibuang"),
        (status = 404, description = "Draft tidak ada", body = umkm_shared::http::ErrorEnvelope),
        (status = 409, description = "Draf sudah diputus", body = umkm_shared::http::ErrorEnvelope)
    )
)]
pub async fn discard_draft(
    State(state): State<AppState>,
    Member(actor): Member,
    Path((_, conv, draft_id)): Path<(Uuid, Uuid, Uuid)>,
) -> Result<StatusCode, ApiError> {
    actor.require("conversations.reply")?;
    repo::draft(pool(&state)?, actor.tenant_id, conv, draft_id)
        .await
        .map_err(repo_err)?
        .ok_or_else(not_found)?;
    repo::decide_draft(
        pool(&state)?,
        actor.tenant_id,
        draft_id,
        actor.user_id,
        None,
    )
    .await
    .map_err(repo_err)?;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct AssignInput {
    /// Anggota aktif yang punya `conversations.reply`
    pub user_id: Uuid,
}

/// Tugaskan percakapan ke anggota lain (status HUMAN). Butuh `conversations.takeover`.
#[utoipa::path(
    post, path = "/api/v1/tenants/{tenant_id}/conversations/{conversation_id}/assign", tag = "conversations",
    params(("tenant_id" = Uuid, Path), ("conversation_id" = Uuid, Path)), request_body = AssignInput,
    responses(
        (status = 200, description = "Ditugaskan", body = SuccessEnvelope<StaffActionView>),
        (status = 403, description = "Permission kurang", body = umkm_shared::http::ErrorEnvelope),
        (status = 409, description = "Percakapan ditutup", body = umkm_shared::http::ErrorEnvelope),
        (status = 422, description = "`user_id` bukan anggota aktif yang dapat membalas", body = umkm_shared::http::ErrorEnvelope)
    )
)]
pub async fn assign(
    State(state): State<AppState>,
    ReqId(id): ReqId,
    Member(actor): Member,
    Path((_, conv)): Path<(Uuid, Uuid)>,
    Json(req): Json<AssignInput>,
) -> Ok<StaffActionView> {
    actor.require("conversations.takeover")?;
    let target =
        umkm_infrastructure::rbac_repo::load_actor(pool(&state)?, req.user_id, actor.tenant_id)
            .await
            .map_err(internal)?;
    if !target.is_some_and(|a| a.has("conversations.reply")) {
        return Err(one("user_id", "not_a_member"));
    }
    load(&state, &actor, conv).await?;
    let s = repo::assign(
        pool(&state)?,
        actor.tenant_id,
        conv,
        req.user_id,
        actor.user_id,
    )
    .await
    .map_err(|e| match e {
        ConversationError::InvalidReference => one("user_id", "not_a_member"),
        e => repo_err(e),
    })?;
    emit_status(&state, actor.tenant_id, conv, s).await;
    Ok(action_view(s, None, &id))
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct TicketFromConversation {
    pub subject: String,
    pub description: Option<String>,
    /// `low` | `normal` | `high` | `urgent` (default `normal`)
    pub priority: Option<String>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct CreatedTicket {
    pub id: Uuid,
}

/// Buat ticket dari percakapan (sumber `chat`, pelanggan ikut tertaut, ditugaskan ke pembuat). Butuh `conversations.read` +
/// `tickets.reply`. Ticket berdiri sendiri: menutup salah satunya tidak menutup yang lain (AC-TKT-006).
#[utoipa::path(
    post, path = "/api/v1/tenants/{tenant_id}/conversations/{conversation_id}/tickets", tag = "conversations",
    params(("tenant_id" = Uuid, Path), ("conversation_id" = Uuid, Path)), request_body = TicketFromConversation,
    responses(
        (status = 201, description = "Ticket dibuat", body = SuccessEnvelope<CreatedTicket>),
        (status = 403, description = "Permission kurang", body = umkm_shared::http::ErrorEnvelope),
        (status = 404, description = "Percakapan tidak ada", body = umkm_shared::http::ErrorEnvelope),
        (status = 422, description = "Input tidak valid", body = umkm_shared::http::ErrorEnvelope)
    )
)]
pub async fn create_ticket(
    State(state): State<AppState>,
    ReqId(id): ReqId,
    ClientIp(ip): ClientIp,
    Member(actor): Member,
    Path((_, conv)): Path<(Uuid, Uuid)>,
    Json(req): Json<TicketFromConversation>,
) -> Result<(StatusCode, Json<SuccessEnvelope<CreatedTicket>>), ApiError> {
    use umkm_domain::tickets::{BODY_MAX, Priority, SUBJECT_MAX, Source};
    actor.require("conversations.read")?;
    actor.require("tickets.reply")?;
    let subject = req.subject.trim().to_owned();
    if subject.is_empty() || subject.chars().count() > SUBJECT_MAX {
        return Err(one(
            "subject",
            if subject.is_empty() {
                "required"
            } else {
                "too_long"
            },
        ));
    }
    let description = req
        .description
        .map(|d| d.trim().to_owned())
        .filter(|d| !d.is_empty());
    if description
        .as_ref()
        .is_some_and(|d| d.chars().count() > BODY_MAX)
    {
        return Err(one("description", "too_long"));
    }
    let priority = Priority::parse(req.priority.as_deref().unwrap_or("normal"))
        .ok_or_else(|| one("priority", "invalid"))?;
    let c = load(&state, &actor, conv).await?;
    let tid = umkm_infrastructure::ticket_repo::create(
        pool(&state)?,
        actor.tenant_id,
        &umkm_infrastructure::ticket_repo::NewTicket {
            source: Source::Chat,
            customer_id: c.customer_id,
            order_id: None,
            conversation_id: Some(conv),
            subject,
            description,
            priority,
            tags: vec![],
            // Ditugaskan ke pembuatnya agar staf chat dapat menindaklanjuti (tanpa tickets.assign hanya
            // ticket miliknya yang terlihat, P4-3). Menugaskan orang lain tetap lewat modul ticket.
            assignee: Some(actor.user_id),
        },
        Some(&actor),
        (Some(id.as_str()), Some(&ip)),
    )
    .await
    .map_err(internal)?;
    Ok((
        StatusCode::CREATED,
        Json(SuccessEnvelope::new(CreatedTicket { id: tid }, &id)),
    ))
}

pub fn routes() -> Router<AppState> {
    let t = "/api/v1/tenants/{tenant_id}/conversations";
    let c = format!("{t}/{{conversation_id}}");
    Router::new()
        .route(t, get(list))
        .route(&c, get(detail))
        .route(&format!("{c}/takeover"), post(takeover))
        .route(&format!("{c}/release"), post(release))
        .route(&format!("{c}/messages"), post(reply))
        .route(&format!("{c}/close"), post(close))
        .route(&format!("{c}/assign"), post(assign))
        .route(&format!("{c}/tickets"), post(create_ticket))
        .route(&format!("{c}/drafts/{{draft_id}}/send"), post(send_draft))
        .route(
            &format!("{c}/drafts/{{draft_id}}/discard"),
            post(discard_draft),
        )
}
