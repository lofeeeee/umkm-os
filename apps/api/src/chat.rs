//! Web chat publik (P6-2, ADR-0006, `PRD.md §4.12`, `PRD.md §7.5`). Tenant dari Host (ADR-0014).
//!
//! - Pemberitahuan AI (D-12) ditampilkan & disetujui sebelum percakapan dibuat; versi disimpan.
//! - Sesi chat = cookie host-only `__Host-umkm_chat` (dev: `umkm_chat`), HttpOnly; hanya hash di DB.
//! - Mutasi: Origin == situs toko + body JSON (cross-site butuh preflight yang ditolak CORS).
//! - Pesan pelanggan → giliran AI hanya bila status `AI` (takeover menghentikan auto-response).
//! - Pesan keluar juga lewat SSE `GET /public/chat/stream`, difilter per percakapan + sesi dicek ulang.

use std::{convert::Infallible, time::Duration};

use axum::{
    Json, Router,
    extract::{Query, State},
    http::{HeaderMap, HeaderValue, StatusCode, header},
    response::{
        IntoResponse, Response,
        sse::{Event, KeepAlive, Sse},
    },
    routing::{get, post},
};
use futures_util::{Stream, stream};
use serde::{Deserialize, Serialize};
use tokio::sync::broadcast;
use umkm_application::ai::{
    llm::{ChatMessage, Role},
    orchestrator::{HISTORY_MAX, TurnInput},
    tools::CustomerCtx,
};
use umkm_domain::{
    ai::{AiMode, Decision, consent},
    auth::rate_rules,
    conversation::{ConversationStatus, SenderType, validate_body},
};
use umkm_infrastructure::{
    ai_repo,
    conversation_repo::{self as repo, Actor, ConversationError, Message, NewMessage, SessionRef},
};
use umkm_shared::{
    http::{ErrorCode, SuccessEnvelope},
    ids::TenantId,
};
use utoipa::{IntoParams, ToSchema};
use uuid::Uuid;

use crate::{
    ApiError, AppState,
    auth::{AuthService, ClientIp, Reject, enforce, service},
    customer_auth::{MaybeCustomer, check_origin},
    http_util::{conflict, internal, not_found, pool},
    middleware::{HostTenant, ReqId},
    realtime::{TenantEvent, emit},
    validation::one,
};

/// Teks SYSTEM saat percakapan diteruskan ke staf.
pub const HANDOVER_NOTICE: &str =
    "Pesan Anda diteruskan ke staf toko. Mohon tunggu balasan di sini.";
pub const EVENT_MESSAGE: &str = "chat.message";
pub const EVENT_STATUS: &str = "chat.status";
/// Hanya staf (`conversations.reply`); tidak pernah diteruskan ke SSE pelanggan.
pub const EVENT_DRAFT: &str = "chat.draft";

fn cookie_name(s: &AuthService) -> &'static str {
    if s.secure_cookies {
        "__Host-umkm_chat"
    } else {
        "umkm_chat"
    }
}

fn cookie(s: &AuthService, token: &str, max_age: u64) -> HeaderValue {
    let secure = if s.secure_cookies { "; Secure" } else { "" };
    HeaderValue::from_str(&format!(
        "{}={token}; Path=/; HttpOnly; SameSite=Lax; Max-Age={max_age}{secure}",
        cookie_name(s)
    ))
    .expect("ascii")
}

fn read_cookie<'a>(s: &AuthService, h: &'a HeaderMap) -> Option<&'a str> {
    let name = cookie_name(s);
    h.get_all(header::COOKIE)
        .iter()
        .filter_map(|v| v.to_str().ok())
        .flat_map(|v| v.split(';'))
        .filter_map(|kv| kv.trim().split_once('='))
        .find(|(k, _)| *k == name)
        .map(|(_, v)| v)
}

pub(crate) fn conv_err(e: ConversationError) -> ApiError {
    match e {
        ConversationError::NotFound => not_found(),
        ConversationError::Closed => conflict("conversation_closed", "Percakapan sudah berakhir"),
        ConversationError::InvalidTransition => {
            conflict("invalid_transition", "Status percakapan tidak dapat diubah")
        }
        e => internal(e),
    }
}

/// Sesi chat dari cookie; tanpa cookie / tidak valid → 401.
async fn chat_session(
    state: &AppState,
    t: TenantId,
    headers: &HeaderMap,
) -> Result<SessionRef, ApiError> {
    let s = service(state)?;
    let token = read_cookie(s, headers).ok_or_else(|| ApiError::new(ErrorCode::Unauthorized))?;
    repo::resolve_session(&s.pool, t, token)
        .await
        .map_err(conv_err)?
        .ok_or_else(|| ApiError::new(ErrorCode::Unauthorized))
}

// ------------------------------------------------------------------ view

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct ChatMessageView {
    pub id: Uuid,
    pub seq: i64,
    /// `CUSTOMER` | `AI` | `ADMIN` | `SYSTEM` (identitas staf tidak ditampilkan ke pelanggan)
    pub sender: String,
    pub body: String,
    pub created_at: String,
}

impl From<&Message> for ChatMessageView {
    fn from(m: &Message) -> Self {
        Self {
            id: m.id,
            seq: m.seq,
            sender: m.sender.as_str().into(),
            body: m.body.clone(),
            created_at: m.created_at.clone(),
        }
    }
}

#[derive(Debug, Serialize, ToSchema)]
pub struct ChatConfigView {
    /// Widget chat aktif (asisten diaktifkan pemilik toko).
    pub enabled: bool,
    pub consent_version: String,
    /// D-12: wajib ditampilkan sebelum pesan pertama.
    pub consent_text: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct ChatConversationView {
    pub conversation_id: Uuid,
    /// `AI` | `WAITING_HUMAN` | `HUMAN` | `CLOSED`
    pub status: String,
    pub messages: Vec<ChatMessageView>,
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct StartChat {
    /// Harus sama dengan `consent_version` dari `/public/chat/config`.
    pub consent_version: String,
    /// Pelanggan menyetujui pemberitahuan AI.
    pub accept: bool,
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct SendChat {
    pub body: String,
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct EmptyBody {}

#[derive(Debug, Deserialize, IntoParams)]
pub struct AfterQuery {
    /// Hanya pesan dengan seq lebih besar (default 0)
    pub after: Option<i64>,
}

async fn chat_enabled(state: &AppState, t: TenantId) -> Result<ai_repo::ChatbotConfig, ApiError> {
    ai_repo::get_config(pool(state)?, t).await.map_err(internal)
}

// ------------------------------------------------------------------ handler

/// Konfigurasi widget chat + teks pemberitahuan AI (D-12).
#[utoipa::path(
    get, path = "/api/v1/public/chat/config", tag = "chat",
    responses(
        (status = 200, description = "Konfigurasi widget", body = SuccessEnvelope<ChatConfigView>),
        (status = 404, description = "Host bukan toko aktif", body = umkm_shared::http::ErrorEnvelope)
    )
)]
pub async fn config(
    State(state): State<AppState>,
    ReqId(id): ReqId,
    HostTenant(t): HostTenant,
) -> Result<Json<SuccessEnvelope<ChatConfigView>>, ApiError> {
    let cfg = chat_enabled(&state, t).await?;
    Ok(Json(SuccessEnvelope::new(
        ChatConfigView {
            enabled: cfg.enabled,
            consent_version: consent::VERSION.into(),
            consent_text: consent::TEXT.into(),
        },
        &id,
    )))
}

/// Mulai percakapan setelah pelanggan menyetujui pemberitahuan AI. Set cookie sesi chat.
#[utoipa::path(
    post, path = "/api/v1/public/chat/conversations", tag = "chat",
    request_body = StartChat,
    responses(
        (status = 201, description = "Percakapan baru; cookie sesi chat di-set", body = SuccessEnvelope<ChatConversationView>),
        (status = 403, description = "Origin bukan situs toko", body = umkm_shared::http::ErrorEnvelope),
        (status = 409, description = "Chat tidak aktif di toko ini", body = umkm_shared::http::ErrorEnvelope),
        (status = 422, description = "Persetujuan tidak diberikan / versi berbeda", body = umkm_shared::http::ErrorEnvelope),
        (status = 429, description = "Rate limit", body = umkm_shared::http::ErrorEnvelope)
    )
)]
pub async fn start(
    State(state): State<AppState>,
    ReqId(id): ReqId,
    ClientIp(ip): ClientIp,
    HostTenant(t): HostTenant,
    headers: HeaderMap,
    MaybeCustomer(customer): MaybeCustomer,
    Json(req): Json<StartChat>,
) -> Result<Response, Reject> {
    let s = service(&state).map_err(Reject::from)?;
    check_origin(s, &headers).map_err(Reject::from)?;
    enforce(s, &rate_rules::CHAT_START_PER_IP, &ip).await?;
    if !req.accept {
        return Err(Reject::from(one("accept", "required")));
    }
    if req.consent_version != consent::VERSION {
        return Err(Reject::from(one("consent_version", "outdated")));
    }
    let cfg = chat_enabled(&state, t).await.map_err(Reject::from)?;
    if !cfg.enabled {
        return Err(Reject::from(conflict(
            "chat_disabled",
            "Chat belum aktif di toko ini",
        )));
    }
    // ADR-0005: hanya akun pelanggan terverifikasi yang ditautkan ke percakapan.
    let customer_id = match customer.filter(|c| c.session.verified) {
        Some(c) => repo::customer_for_account(&s.pool, t, c.session.account_id)
            .await
            .map_err(|e| Reject::from(conv_err(e)))?,
        None => None,
    };
    let initial = if cfg.mode == AiMode::HumanOnly {
        ConversationStatus::WaitingHuman
    } else {
        ConversationStatus::Ai
    };
    let started = repo::start(
        &s.pool,
        t,
        &repo::StartInput {
            consent_version: consent::VERSION,
            customer_id,
            initial,
        },
    )
    .await
    .map_err(|e| Reject::from(conv_err(e)))?;
    let mut messages = vec![];
    let welcome = cfg.welcome_message.trim();
    if !welcome.is_empty() {
        let m = repo::append(
            &s.pool,
            t,
            started.conversation_id,
            &NewMessage {
                sender: SenderType::System,
                session_id: None,
                sender_user_id: None,
                body: welcome,
                ai_run_id: None,
            },
        )
        .await
        .map_err(|e| Reject::from(conv_err(e)))?;
        messages.push(ChatMessageView::from(&m));
    }
    emit(
        &state,
        staff_event(
            t,
            EVENT_STATUS,
            started.conversation_id,
            serde_json::json!({ "status": initial.as_str() }),
        ),
    )
    .await;
    let body = SuccessEnvelope::new(
        ChatConversationView {
            conversation_id: started.conversation_id,
            status: initial.as_str().into(),
            messages,
        },
        &id,
    );
    let max_age = umkm_domain::conversation::SESSION_TTL.as_secs();
    Ok((
        StatusCode::CREATED,
        [(header::SET_COOKIE, cookie(s, &started.token, max_age))],
        Json(body),
    )
        .into_response())
}

/// Percakapan aktif pelanggan ini (dari cookie) + pesan setelah `after`.
#[utoipa::path(
    get, path = "/api/v1/public/chat/conversation", tag = "chat",
    params(AfterQuery),
    responses(
        (status = 200, description = "Percakapan + pesan", body = SuccessEnvelope<ChatConversationView>),
        (status = 401, description = "Tidak ada sesi chat", body = umkm_shared::http::ErrorEnvelope)
    )
)]
pub async fn current(
    State(state): State<AppState>,
    ReqId(id): ReqId,
    HostTenant(t): HostTenant,
    headers: HeaderMap,
    Query(q): Query<AfterQuery>,
) -> Result<Json<SuccessEnvelope<ChatConversationView>>, ApiError> {
    let sref = chat_session(&state, t, &headers).await?;
    let msgs = repo::messages(
        pool(&state)?,
        t,
        sref.conversation_id,
        q.after.unwrap_or(0).max(0),
        200,
    )
    .await
    .map_err(conv_err)?;
    Ok(Json(SuccessEnvelope::new(
        ChatConversationView {
            conversation_id: sref.conversation_id,
            status: sref.status.as_str().into(),
            messages: msgs.iter().map(ChatMessageView::from).collect(),
        },
        &id,
    )))
}

/// Kirim pesan pelanggan. Status `AI` → asisten menjawab dalam respons yang sama.
#[utoipa::path(
    post, path = "/api/v1/public/chat/messages", tag = "chat",
    request_body = SendChat,
    responses(
        (status = 200, description = "Pesan pelanggan + balasan (bila ada) dan status terbaru", body = SuccessEnvelope<ChatConversationView>),
        (status = 401, description = "Tidak ada sesi chat", body = umkm_shared::http::ErrorEnvelope),
        (status = 403, description = "Origin bukan situs toko", body = umkm_shared::http::ErrorEnvelope),
        (status = 409, description = "Percakapan sudah berakhir", body = umkm_shared::http::ErrorEnvelope),
        (status = 422, description = "Isi pesan tidak valid; `details.errors[0].path = body`", body = umkm_shared::http::ErrorEnvelope),
        (status = 429, description = "Rate limit", body = umkm_shared::http::ErrorEnvelope)
    )
)]
pub async fn send(
    State(state): State<AppState>,
    ReqId(id): ReqId,
    ClientIp(ip): ClientIp,
    HostTenant(t): HostTenant,
    headers: HeaderMap,
    Json(req): Json<SendChat>,
) -> Result<Json<SuccessEnvelope<ChatConversationView>>, Reject> {
    let s = service(&state).map_err(Reject::from)?;
    check_origin(s, &headers).map_err(Reject::from)?;
    enforce(s, &rate_rules::CHAT_MESSAGE_PER_IP, &ip).await?;
    let sref = chat_session(&state, t, &headers)
        .await
        .map_err(Reject::from)?;
    let body = validate_body(&req.body).map_err(|e| Reject::from(one("body", e.code())))?;
    let m = append_and_emit(
        &state,
        t,
        sref.conversation_id,
        &NewMessage {
            sender: SenderType::Customer,
            session_id: Some(sref.session_id),
            sender_user_id: None,
            body: &body,
            ai_run_id: None,
        },
    )
    .await
    .map_err(Reject::from)?;
    let mut out = vec![ChatMessageView::from(&m)];
    let status = if sref.status.ai_may_reply() {
        let (replies, status) = respond(&state, t, sref.conversation_id, &body)
            .await
            .map_err(Reject::from)?;
        out.extend(replies.iter().map(ChatMessageView::from));
        status
    } else {
        sref.status
    };
    Ok(Json(SuccessEnvelope::new(
        ChatConversationView {
            conversation_id: sref.conversation_id,
            status: status.as_str().into(),
            messages: out,
        },
        &id,
    )))
}

/// Pelanggan meminta staf (tombol "Bicara dengan admin").
#[utoipa::path(
    post, path = "/api/v1/public/chat/handover", tag = "chat",
    request_body = EmptyBody,
    responses(
        (status = 200, description = "Diteruskan ke staf", body = SuccessEnvelope<ChatConversationView>),
        (status = 401, description = "Tidak ada sesi chat", body = umkm_shared::http::ErrorEnvelope),
        (status = 409, description = "Sudah ditangani staf / berakhir", body = umkm_shared::http::ErrorEnvelope)
    )
)]
pub async fn handover(
    State(state): State<AppState>,
    ReqId(id): ReqId,
    HostTenant(t): HostTenant,
    headers: HeaderMap,
    Json(_): Json<EmptyBody>,
) -> Result<Json<SuccessEnvelope<ChatConversationView>>, ApiError> {
    let s = service(&state)?;
    check_origin(s, &headers)?;
    let sref = chat_session(&state, t, &headers).await?;
    if sref.status != ConversationStatus::Ai {
        return Err(conflict(
            "already_with_staff",
            "Percakapan sudah diteruskan ke staf",
        ));
    }
    let msgs = to_staff(&state, t, sref.conversation_id, Actor::Customer).await?;
    Ok(Json(SuccessEnvelope::new(
        ChatConversationView {
            conversation_id: sref.conversation_id,
            status: ConversationStatus::WaitingHuman.as_str().into(),
            messages: msgs.iter().map(ChatMessageView::from).collect(),
        },
        &id,
    )))
}

/// Pelanggan mengakhiri percakapan: CLOSED + sesi dicabut + cookie dihapus.
#[utoipa::path(
    post, path = "/api/v1/public/chat/end", tag = "chat",
    request_body = EmptyBody,
    responses(
        (status = 204, description = "Percakapan berakhir"),
        (status = 401, description = "Tidak ada sesi chat", body = umkm_shared::http::ErrorEnvelope)
    )
)]
pub async fn end(
    State(state): State<AppState>,
    HostTenant(t): HostTenant,
    headers: HeaderMap,
    Json(_): Json<EmptyBody>,
) -> Result<Response, ApiError> {
    let s = service(&state)?;
    check_origin(s, &headers)?;
    let sref = chat_session(&state, t, &headers).await?;
    if sref.status != ConversationStatus::Closed {
        repo::transition(
            &s.pool,
            t,
            sref.conversation_id,
            ConversationStatus::Closed,
            Actor::Customer,
        )
        .await
        .map_err(conv_err)?;
        emit_status(&state, t, sref.conversation_id, ConversationStatus::Closed).await;
    }
    repo::revoke_session(&s.pool, t, sref.session_id)
        .await
        .map_err(conv_err)?;
    Ok((
        StatusCode::NO_CONTENT,
        [(header::SET_COOKIE, cookie(s, "", 0))],
    )
        .into_response())
}

/// SSE pesan & status untuk percakapan pelanggan ini saja.
#[utoipa::path(
    get, path = "/api/v1/public/chat/stream", tag = "chat",
    responses(
        (status = 200, description = "text/event-stream: `chat.message`, `chat.status` percakapan ini", content_type = "text/event-stream", body = String),
        (status = 401, description = "Tidak ada sesi chat", body = umkm_shared::http::ErrorEnvelope),
        (status = 503, description = "Batas koneksi realtime tercapai", body = umkm_shared::http::ErrorEnvelope)
    )
)]
pub async fn chat_stream(
    State(state): State<AppState>,
    HostTenant(t): HostTenant,
    headers: HeaderMap,
) -> Result<Sse<impl Stream<Item = Result<Event, Infallible>>>, ApiError> {
    let sref = chat_session(&state, t, &headers).await?;
    let (rx, guard) = state
        .realtime
        .connect()
        .ok_or_else(|| ApiError::new(ErrorCode::ServiceUnavailable))?;
    let db = pool(&state)?.clone();
    let conv = sref.conversation_id.to_string();
    let session = sref.session_id;
    let s = stream::unfold((rx, guard, db), move |(mut rx, guard, db)| {
        let conv = conv.clone();
        async move {
            loop {
                match rx.recv().await {
                    Ok(ev) => {
                        // Guard 1: tenant ini, jenis event pelanggan, percakapan ini saja.
                        if ev.tenant_id != t
                            || ev.user_id.is_some()
                            || !(ev.kind == EVENT_MESSAGE || ev.kind == EVENT_STATUS)
                            || ev.data.get("conversation_id").and_then(|v| v.as_str())
                                != Some(conv.as_str())
                        {
                            continue;
                        }
                        // Guard 2: sesi masih berlaku (dicabut/kedaluwarsa → stream berakhir).
                        if !matches!(repo::session_active(&db, t, session).await, Ok(true)) {
                            return None;
                        }
                        let data = ev.data.get("customer").cloned().unwrap_or_default();
                        let event = Event::default()
                            .event(ev.kind.clone())
                            .json_data(&data)
                            .ok()?;
                        return Some((Ok(event), (rx, guard, db)));
                    }
                    Err(broadcast::error::RecvError::Lagged(_)) => continue,
                    Err(broadcast::error::RecvError::Closed) => return None,
                }
            }
        }
    });
    Ok(Sse::new(s).keep_alive(KeepAlive::new().interval(Duration::from_secs(15))))
}

// ------------------------------------------------------------------ alur

/// Event realtime: staf ber-`conversations.read` menerima `data`; pelanggan hanya `data.customer`.
pub(crate) fn staff_event(
    t: TenantId,
    kind: &str,
    conversation_id: Uuid,
    customer: serde_json::Value,
) -> TenantEvent {
    let mut ev = TenantEvent::tenant(
        t,
        kind,
        serde_json::json!({
            "conversation_id": conversation_id.to_string(),
            "customer": customer,
        }),
    );
    ev.permission = Some("conversations.read".into());
    ev
}

pub(crate) async fn emit_status(state: &AppState, t: TenantId, conv: Uuid, s: ConversationStatus) {
    emit(
        state,
        staff_event(
            t,
            EVENT_STATUS,
            conv,
            serde_json::json!({ "status": s.as_str() }),
        ),
    )
    .await;
}

/// Tambah pesan + kabarkan lewat realtime.
pub async fn append_and_emit(
    state: &AppState,
    t: TenantId,
    conv: Uuid,
    m: &NewMessage<'_>,
) -> Result<Message, ApiError> {
    let msg = repo::append(pool(state)?, t, conv, m)
        .await
        .map_err(conv_err)?;
    emit(
        state,
        staff_event(
            t,
            EVENT_MESSAGE,
            conv,
            serde_json::to_value(ChatMessageView::from(&msg)).unwrap_or_default(),
        ),
    )
    .await;
    Ok(msg)
}

/// AI → WAITING_HUMAN + pesan SYSTEM untuk pelanggan.
pub(crate) async fn to_staff(
    state: &AppState,
    t: TenantId,
    conv: Uuid,
    actor: Actor,
) -> Result<Vec<Message>, ApiError> {
    repo::transition(
        pool(state)?,
        t,
        conv,
        ConversationStatus::WaitingHuman,
        actor,
    )
    .await
    .map_err(conv_err)?;
    emit_status(state, t, conv, ConversationStatus::WaitingHuman).await;
    let m = append_and_emit(
        state,
        t,
        conv,
        &NewMessage {
            sender: SenderType::System,
            session_id: None,
            sender_user_id: None,
            body: HANDOVER_NOTICE,
            ai_run_id: None,
        },
    )
    .await?;
    Ok(vec![m])
}

fn history(msgs: &[Message]) -> Vec<ChatMessage> {
    msgs.iter()
        .filter_map(|m| match m.sender {
            SenderType::Customer => Some(ChatMessage::new(Role::User, m.body.clone())),
            SenderType::Ai | SenderType::Admin => {
                Some(ChatMessage::new(Role::Assistant, m.body.clone()))
            }
            SenderType::System => None,
        })
        .collect()
}

/// Satu giliran AI untuk pesan pelanggan `body` (sudah tersimpan). Mengembalikan pesan baru + status.
async fn respond(
    state: &AppState,
    t: TenantId,
    conv: Uuid,
    body: &str,
) -> Result<(Vec<Message>, ConversationStatus), ApiError> {
    let db = pool(state)?;
    let Some(ai) = state.ai.as_ref() else {
        return Ok((
            to_staff(state, t, conv, Actor::System).await?,
            ConversationStatus::WaitingHuman,
        ));
    };
    let c = repo::get(db, t, conv)
        .await
        .map_err(conv_err)?
        .ok_or_else(not_found)?;
    let mut recent = repo::recent(db, t, conv, (HISTORY_MAX + 1) as i64)
        .await
        .map_err(conv_err)?;
    recent.pop(); // pesan pelanggan yang baru disimpan = `message`
    let input = TurnInput {
        message: body.to_owned(),
        history: history(&recent),
        customer: c.customer_id.map(|customer_id| CustomerCtx {
            customer_id,
            verified: true,
        }),
    };
    let name = ai.runtime.business_name(t).await.map_err(internal)?;
    let r = match ai
        .runtime
        .answer_in(t, &name, input, "chat", Some(conv))
        .await
    {
        Ok(r) => r,
        Err(e) => {
            // Chat tidak boleh macet: provider belum diatur / chatbot dimatikan → staf.
            tracing::warn!(error = %e, "chat turn unavailable; handing over");
            return Ok((
                to_staff(state, t, conv, Actor::System).await?,
                ConversationStatus::WaitingHuman,
            ));
        }
    };
    let o = r.outcome;
    let run_id = r.run_id;
    fn ai_msg(reply: &str, run_id: Uuid) -> NewMessage<'_> {
        NewMessage {
            sender: SenderType::Ai,
            session_id: None,
            sender_user_id: None,
            body: reply,
            ai_run_id: Some(run_id),
        }
    }
    let mut out = vec![];
    match o.decision {
        Decision::SendAuto => {
            if let Some(reply) = o.reply.as_deref().and_then(|x| validate_body(x).ok()) {
                out.push(append_and_emit(state, t, conv, &ai_msg(&reply, run_id)).await?);
            }
            Ok((out, ConversationStatus::Ai))
        }
        Decision::Handover => {
            if let Some(reply) = o.reply.as_deref().and_then(|x| validate_body(x).ok()) {
                out.push(append_and_emit(state, t, conv, &ai_msg(&reply, run_id)).await?);
            }
            out.extend(to_staff(state, t, conv, Actor::Ai).await?);
            Ok((out, ConversationStatus::WaitingHuman))
        }
        // Assisted: draft untuk staf (penyimpanan & persetujuan draft: P6-4). Human queue: antre staf.
        // Assisted: draft disimpan untuk staf (tidak pernah dikirim ke pelanggan tanpa keputusan staf).
        Decision::DraftForAdmin => {
            if let Some(reply) = o.reply.as_deref().and_then(|x| validate_body(x).ok()) {
                let draft = repo::create_draft(pool(state)?, t, conv, Some(run_id), &reply)
                    .await
                    .map_err(conv_err)?;
                let mut ev = TenantEvent::tenant(
                    t,
                    EVENT_DRAFT,
                    serde_json::json!({ "conversation_id": conv.to_string(), "draft_id": draft.to_string() }),
                );
                ev.permission = Some("conversations.reply".into());
                emit(state, ev).await;
            }
            Ok((
                to_staff(state, t, conv, Actor::Ai).await?,
                ConversationStatus::WaitingHuman,
            ))
        }
        Decision::HumanQueue => Ok((
            to_staff(state, t, conv, Actor::Ai).await?,
            ConversationStatus::WaitingHuman,
        )),
    }
}

pub fn routes() -> Router<AppState> {
    let p = "/api/v1/public/chat";
    Router::new()
        .route(&format!("{p}/config"), get(config))
        .route(&format!("{p}/conversations"), post(start))
        .route(&format!("{p}/conversation"), get(current))
        .route(&format!("{p}/messages"), post(send))
        .route(&format!("{p}/handover"), post(handover))
        .route(&format!("{p}/end"), post(end))
        .route(&format!("{p}/stream"), get(chat_stream))
}
