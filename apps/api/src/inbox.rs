//! Conversation Viewer (P6-3, `PRD.md §4.12`, FR-CNV-002/003). Tenant dari path + membership.
//!
//! - Butuh `conversations.read`.
//! - Tanpa masking hanya bila pembaca ber-`conversations.reply` DAN `customers.read` (staf yang
//!   menangani chat). Selain itu (mis. role Viewer, AC-CNV-003): kontak pelanggan, email staf, email &
//!   deret ≥ 8 digit di isi pesan disamarkan, dan ringkasan argumen tool tidak ditampilkan.

use axum::{
    Json, Router,
    extract::{Path, Query, State},
    routing::get,
};
use serde::{Deserialize, Serialize};
use umkm_domain::{
    conversation::{ConversationStatus, mask},
    rbac::Actor,
};
use umkm_infrastructure::conversation_repo::{
    self as repo, AiRunInfo, ConversationError, CustomerInfo, ListFilter, Summary,
};
use umkm_shared::http::SuccessEnvelope;
use utoipa::{IntoParams, ToSchema};
use uuid::Uuid;

use crate::{
    ApiError, AppState,
    http_util::{Page, internal, not_found, pool},
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
    pub masked: bool,
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
pub struct ConversationQuery {
    /// `AI` | `WAITING_HUMAN` | `HUMAN` | `CLOSED`
    pub status: Option<String>,
    pub assignee_user_id: Option<Uuid>,
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
            assignee: q.assignee_user_id,
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
        masked: !m.full,
        summary: m.summary(d.summary),
    };
    Ok(Json(SuccessEnvelope::new(view, &id)))
}

pub fn routes() -> Router<AppState> {
    let t = "/api/v1/tenants/{tenant_id}/conversations";
    Router::new()
        .route(t, get(list))
        .route(&format!("{t}/{{conversation_id}}"), get(detail))
}
