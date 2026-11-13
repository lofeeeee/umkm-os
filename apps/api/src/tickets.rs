//! Ticketing (P4-3, `PRD.md §4.8`). Staf: baca `tickets.read`; buat/balas/ubah `tickets.reply`;
//! assign `tickets.assign`; resolve/close `tickets.close`. Object-level: tanpa `tickets.assign`
//! hanya ticket yang ditugaskan kepada pemanggil. Customer: lewat token order, isi publik saja.

use axum::{
    Json, Router,
    body::Bytes,
    extract::{Path, Query, State},
    http::{HeaderMap, HeaderValue, StatusCode, header},
    response::{IntoResponse, Response},
    routing::{get, post},
};
use serde::{Deserialize, Serialize};
use umkm_domain::{
    rbac::Actor,
    tickets::{BODY_MAX, Priority, SUBJECT_MAX, Source, TicketStatus, normalize_tags},
};
use umkm_infrastructure::ticket_repo::{
    self as repo, Filter, NewTicket, Scope, Ticket, TicketError,
};
use umkm_shared::{
    http::{ErrorBody, ErrorCode, SuccessEnvelope},
    ids::TenantId,
};
use utoipa::ToSchema;
use uuid::Uuid;

use crate::{
    ApiError, AppState,
    auth::ClientIp,
    http_util::{Page, conflict, internal, not_found, pool},
    middleware::{HostTenant, ReqId},
    session::Member,
    validation::{field_errors, one},
};

#[derive(Debug, Serialize, ToSchema)]
pub struct MessageView {
    pub id: Uuid,
    /// `public` | `internal`
    pub visibility: String,
    pub author_type: String,
    pub author_user_id: Option<Uuid>,
    pub body: String,
    pub created_at: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct AttachmentView {
    pub id: Uuid,
    pub visibility: String,
    pub content_type: String,
    pub byte_size: i32,
    pub uploaded_by: String,
    pub created_at: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct EventView {
    pub kind: String,
    #[schema(value_type = Object)]
    pub data: serde_json::Value,
    pub actor_type: String,
    pub actor_user_id: Option<Uuid>,
    pub created_at: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct TicketView {
    pub id: Uuid,
    pub code: String,
    pub source: String,
    pub customer_id: Option<Uuid>,
    pub order_id: Option<Uuid>,
    pub conversation_id: Option<Uuid>,
    pub subject: String,
    pub description: Option<String>,
    pub priority: String,
    pub status: String,
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
    pub messages: Vec<MessageView>,
    pub attachments: Vec<AttachmentView>,
    pub events: Vec<EventView>,
}

/// Bentuk untuk customer: tanpa assignee, tag, SLA internal, riwayat, catatan internal.
#[derive(Debug, Serialize, ToSchema)]
pub struct PublicTicket {
    pub id: Uuid,
    pub code: String,
    pub subject: String,
    pub status: String,
    pub created_at: String,
    pub messages: Vec<MessageView>,
    pub attachments: Vec<AttachmentView>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct TicketList {
    pub items: Vec<TicketView>,
    pub total: i64,
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct TicketInput {
    pub subject: String,
    pub description: Option<String>,
    #[serde(default = "normal")]
    pub priority: String,
    #[serde(default)]
    pub tags: Vec<String>,
    pub customer_id: Option<Uuid>,
    pub order_id: Option<Uuid>,
    pub assignee_user_id: Option<Uuid>,
}

fn normal() -> String {
    "normal".into()
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct AssignInput {
    pub assignee_user_id: Option<Uuid>,
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct StatusInput {
    pub status: String,
    pub reason: Option<String>,
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct MetaInput {
    pub priority: Option<String>,
    pub tags: Option<Vec<String>>,
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct MessageInput {
    pub body: String,
    /// Catatan internal: tidak pernah terlihat customer (AC-TKT-004)
    #[serde(default)]
    pub internal: bool,
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct CustomerTicketInput {
    pub subject: String,
    pub description: Option<String>,
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct CustomerMessageInput {
    pub body: String,
}

#[derive(Debug, Deserialize, utoipa::IntoParams)]
#[into_params(parameter_in = Query)]
pub struct TicketQuery {
    pub status: Option<String>,
    pub assignee_user_id: Option<Uuid>,
    pub priority: Option<String>,
    pub tag: Option<String>,
    pub q: Option<String>,
    pub customer_id: Option<Uuid>,
    pub order_id: Option<Uuid>,
    pub limit: Option<i64>,
    pub offset: Option<i64>,
}

#[derive(Debug, Deserialize, utoipa::IntoParams)]
#[into_params(parameter_in = Query)]
pub struct AttachQuery {
    #[serde(default)]
    pub internal: bool,
}

fn messages(t: &Ticket) -> Vec<MessageView> {
    t.messages
        .iter()
        .map(|m| MessageView {
            id: m.id,
            visibility: m.visibility.clone(),
            author_type: m.author_type.clone(),
            author_user_id: m.author_user_id,
            body: m.body.clone(),
            created_at: m.created_at.clone(),
        })
        .collect()
}

fn attachments(t: &Ticket) -> Vec<AttachmentView> {
    t.attachments
        .iter()
        .map(|a| AttachmentView {
            id: a.id,
            visibility: a.visibility.clone(),
            content_type: a.content_type.clone(),
            byte_size: a.byte_size,
            uploaded_by: a.uploaded_by.clone(),
            created_at: a.created_at.clone(),
        })
        .collect()
}

pub fn view(t: Ticket) -> TicketView {
    let (messages, attachments) = (messages(&t), attachments(&t));
    TicketView {
        id: t.id,
        code: t.code,
        source: t.source,
        customer_id: t.customer_id,
        order_id: t.order_id,
        conversation_id: t.conversation_id,
        subject: t.subject,
        description: t.description,
        priority: t.priority,
        status: t.status.as_str().into(),
        assignee_user_id: t.assignee_user_id,
        tags: t.tags,
        first_response_due_at: t.first_response_due_at,
        resolution_due_at: t.resolution_due_at,
        first_responded_at: t.first_responded_at,
        resolved_at: t.resolved_at,
        closed_at: t.closed_at,
        sla_first_response_breached: t.sla_first_response_breached,
        sla_resolution_breached: t.sla_resolution_breached,
        reopen_count: t.reopen_count,
        created_at: t.created_at,
        updated_at: t.updated_at,
        messages,
        attachments,
        events: t
            .events
            .into_iter()
            .map(|e| EventView {
                kind: e.kind,
                data: e.data,
                actor_type: e.actor_type,
                actor_user_id: e.actor_user_id,
                created_at: e.created_at,
            })
            .collect(),
    }
}

fn public_view(t: Ticket) -> PublicTicket {
    let (messages, attachments) = (messages(&t), attachments(&t));
    PublicTicket {
        id: t.id,
        code: t.code,
        subject: t.subject,
        status: t.status.as_str().into(),
        created_at: t.created_at,
        messages,
        attachments,
    }
}

fn scope(actor: &Actor) -> Scope {
    if actor.has("tickets.assign") {
        Scope::All
    } else {
        Scope::AssignedTo(actor.user_id)
    }
}

fn ticket_err(e: TicketError) -> ApiError {
    match e {
        TicketError::NotFound => not_found(),
        TicketError::InvalidReference => one("order_id", "invalid_reference"),
        TicketError::InvalidAssignee => one("assignee_user_id", "not_a_member"),
        TicketError::InvalidTransition(s) => ApiError(
            ErrorBody::new(ErrorCode::Conflict)
                .with_message("Transisi status ticket tidak diizinkan")
                .with_details(
                    serde_json::json!({ "reason": "invalid_ticket_status", "status": s }),
                ),
        ),
        TicketError::ReopenExpired => conflict(
            "reopen_expired",
            "Ticket sudah ditutup lebih dari 7 hari; buat ticket baru",
        ),
        TicketError::Upload(u) => crate::upload::upload_error(u),
        TicketError::Db(e) => internal(e),
    }
}

fn text(v: &str, path: &str, max: usize) -> Result<String, ApiError> {
    let mut e = vec![];
    let v = v.trim().to_owned();
    umkm_domain::business::plain_text(&mut e, path, Some(&v), max, true);
    if e.is_empty() {
        Ok(v)
    } else {
        Err(field_errors(e.iter().map(|x| (x.path.as_str(), x.code))))
    }
}

fn opt_text(v: Option<String>, path: &str, max: usize) -> Result<Option<String>, ApiError> {
    v.map(|s| s.trim().to_owned())
        .filter(|s| !s.is_empty())
        .map(|s| text(&s, path, max))
        .transpose()
}

async fn reload(
    state: &AppState,
    t: TenantId,
    sc: Scope,
    id: Uuid,
) -> Result<TicketView, ApiError> {
    repo::get(pool(state)?, t, sc, id)
        .await
        .map_err(internal)?
        .map(view)
        .ok_or_else(not_found)
}

fn bytes_response(ct: String, bytes: Vec<u8>) -> Result<Response, ApiError> {
    let mut res = bytes.into_response();
    let h = res.headers_mut();
    h.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_str(&ct).map_err(internal)?,
    );
    h.insert(
        "x-content-type-options",
        HeaderValue::from_static("nosniff"),
    );
    h.insert(
        header::CONTENT_SECURITY_POLICY,
        HeaderValue::from_static("default-src 'none'; sandbox"),
    );
    h.insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static("private, no-store"),
    );
    Ok(res)
}

fn max_bytes(state: &AppState) -> usize {
    state
        .files
        .as_deref()
        .map(|f| f.store.limits().max_object_bytes)
        .unwrap_or(2 * 1024 * 1024)
}

type Ok<T> = Result<Json<SuccessEnvelope<T>>, ApiError>;

// ------------------------------------------------------------------ staf

/// Daftar ticket (filter, pencarian kode/subjek).
#[utoipa::path(
    get, path = "/api/v1/tenants/{tenant_id}/tickets", tag = "tickets",
    params(("tenant_id" = Uuid, Path), TicketQuery),
    responses((status = 200, description = "Ticket", body = SuccessEnvelope<TicketList>))
)]
pub async fn list(
    State(state): State<AppState>,
    ReqId(id): ReqId,
    Member(actor): Member,
    Query(q): Query<TicketQuery>,
) -> Ok<TicketList> {
    actor.require("tickets.read")?;
    let status = match q.status.as_deref() {
        None | Some("") => None,
        Some(s) => Some(TicketStatus::parse(s).ok_or_else(|| one("status", "invalid_status"))?),
    };
    let (limit, offset) = Page {
        limit: q.limit,
        offset: q.offset,
    }
    .bounds();
    let f = Filter {
        status,
        assignee: q.assignee_user_id,
        priority: q.priority,
        tag: q.tag.map(|t| t.trim().to_lowercase()),
        q: q.q
            .map(|s| s.trim().chars().take(100).collect::<String>())
            .filter(|s| !s.is_empty()),
        customer_id: q.customer_id,
        order_id: q.order_id,
        limit,
        offset,
    };
    let (items, total) = repo::list(pool(&state)?, actor.tenant_id, scope(&actor), &f)
        .await
        .map_err(internal)?;
    Ok(Json(SuccessEnvelope::new(
        TicketList {
            items: items.into_iter().map(view).collect(),
            total,
        },
        &id,
    )))
}

/// Ticket manual oleh staf (AC-TKT-001 sumber `manual`).
#[utoipa::path(
    post, path = "/api/v1/tenants/{tenant_id}/tickets", tag = "tickets",
    params(("tenant_id" = Uuid, Path)), request_body = TicketInput,
    responses((status = 201, description = "Dibuat", body = SuccessEnvelope<TicketView>))
)]
pub async fn create(
    State(state): State<AppState>,
    ReqId(id): ReqId,
    ClientIp(ip): ClientIp,
    Member(actor): Member,
    Json(req): Json<TicketInput>,
) -> Result<(StatusCode, Json<SuccessEnvelope<TicketView>>), ApiError> {
    actor.require("tickets.reply")?;
    if req.assignee_user_id.is_some() {
        actor.require("tickets.assign")?;
    }
    let subject = text(&req.subject, "subject", SUBJECT_MAX)?;
    let description = opt_text(req.description, "description", BODY_MAX)?;
    let priority = Priority::parse(&req.priority).ok_or_else(|| one("priority", "invalid"))?;
    let tags = normalize_tags(&req.tags).ok_or_else(|| one("tags", "invalid_tag"))?;
    let n = NewTicket {
        source: Source::Manual,
        customer_id: req.customer_id,
        order_id: req.order_id,
        conversation_id: None,
        subject,
        description,
        priority,
        tags,
        assignee: req.assignee_user_id,
    };
    let tid = repo::create(
        pool(&state)?,
        actor.tenant_id,
        &n,
        Some(&actor),
        (Some(id.as_str()), Some(&ip)),
    )
    .await
    .map_err(ticket_err)?;
    Ok((
        StatusCode::CREATED,
        Json(SuccessEnvelope::new(
            reload(&state, actor.tenant_id, Scope::All, tid).await?,
            &id,
        )),
    ))
}

/// Detail + pesan (termasuk catatan internal) + lampiran + riwayat.
#[utoipa::path(
    get, path = "/api/v1/tenants/{tenant_id}/tickets/{ticket_id}", tag = "tickets",
    params(("tenant_id" = Uuid, Path), ("ticket_id" = Uuid, Path)),
    responses((status = 200, description = "Ticket", body = SuccessEnvelope<TicketView>))
)]
pub async fn get_one(
    State(state): State<AppState>,
    ReqId(id): ReqId,
    Member(actor): Member,
    Path((_, tid)): Path<(Uuid, Uuid)>,
) -> Ok<TicketView> {
    actor.require("tickets.read")?;
    Ok(Json(SuccessEnvelope::new(
        reload(&state, actor.tenant_id, scope(&actor), tid).await?,
        &id,
    )))
}

/// Assign / reassign / lepas (AC-TKT-003).
#[utoipa::path(
    post, path = "/api/v1/tenants/{tenant_id}/tickets/{ticket_id}/assign", tag = "tickets",
    params(("tenant_id" = Uuid, Path), ("ticket_id" = Uuid, Path)), request_body = AssignInput,
    responses((status = 200, description = "Tersimpan", body = SuccessEnvelope<TicketView>))
)]
pub async fn assign(
    State(state): State<AppState>,
    ReqId(id): ReqId,
    ClientIp(ip): ClientIp,
    Member(actor): Member,
    Path((_, tid)): Path<(Uuid, Uuid)>,
    Json(req): Json<AssignInput>,
) -> Ok<TicketView> {
    actor.require("tickets.assign")?;
    repo::assign(
        pool(&state)?,
        &actor,
        tid,
        req.assignee_user_id,
        (Some(id.as_str()), Some(&ip)),
    )
    .await
    .map_err(ticket_err)?;
    Ok(Json(SuccessEnvelope::new(
        reload(&state, actor.tenant_id, Scope::All, tid).await?,
        &id,
    )))
}

/// Ubah status (AC-TKT-002). RESOLVED/CLOSED butuh `tickets.close`; reopen sesuai kebijakan.
#[utoipa::path(
    post, path = "/api/v1/tenants/{tenant_id}/tickets/{ticket_id}/status", tag = "tickets",
    params(("tenant_id" = Uuid, Path), ("ticket_id" = Uuid, Path)), request_body = StatusInput,
    responses(
        (status = 200, description = "Status berubah", body = SuccessEnvelope<TicketView>),
        (status = 409, description = "Transisi tidak diizinkan / reopen kedaluwarsa", body = umkm_shared::http::ErrorEnvelope)
    )
)]
pub async fn set_status(
    State(state): State<AppState>,
    ReqId(id): ReqId,
    ClientIp(ip): ClientIp,
    Member(actor): Member,
    Path((_, tid)): Path<(Uuid, Uuid)>,
    Json(req): Json<StatusInput>,
) -> Ok<TicketView> {
    let to = TicketStatus::parse(&req.status).ok_or_else(|| one("status", "invalid_status"))?;
    if to == TicketStatus::Assigned {
        return Err(one("status", "use_assign_endpoint"));
    }
    actor.require(if to.is_done() {
        "tickets.close"
    } else {
        "tickets.reply"
    })?;
    let reason = opt_text(req.reason, "reason", 500)?;
    let sc = scope(&actor);
    repo::set_status(
        pool(&state)?,
        &actor,
        sc,
        tid,
        to,
        reason.as_deref(),
        (Some(id.as_str()), Some(&ip)),
    )
    .await
    .map_err(ticket_err)?;
    Ok(Json(SuccessEnvelope::new(
        reload(&state, actor.tenant_id, sc, tid).await?,
        &id,
    )))
}

/// Ubah prioritas / tag.
#[utoipa::path(
    patch, path = "/api/v1/tenants/{tenant_id}/tickets/{ticket_id}", tag = "tickets",
    params(("tenant_id" = Uuid, Path), ("ticket_id" = Uuid, Path)), request_body = MetaInput,
    responses((status = 200, description = "Tersimpan", body = SuccessEnvelope<TicketView>))
)]
pub async fn update_meta(
    State(state): State<AppState>,
    ReqId(id): ReqId,
    Member(actor): Member,
    Path((_, tid)): Path<(Uuid, Uuid)>,
    Json(req): Json<MetaInput>,
) -> Ok<TicketView> {
    actor.require("tickets.reply")?;
    let priority = req
        .priority
        .as_deref()
        .map(|p| Priority::parse(p).ok_or_else(|| one("priority", "invalid")))
        .transpose()?;
    let tags = req
        .tags
        .as_deref()
        .map(|t| normalize_tags(t).ok_or_else(|| one("tags", "invalid_tag")))
        .transpose()?;
    let sc = scope(&actor);
    repo::update_meta(pool(&state)?, &actor, sc, tid, priority, tags.as_deref())
        .await
        .map_err(ticket_err)?;
    Ok(Json(SuccessEnvelope::new(
        reload(&state, actor.tenant_id, sc, tid).await?,
        &id,
    )))
}

/// Balasan publik atau catatan internal (AC-TKT-004).
#[utoipa::path(
    post, path = "/api/v1/tenants/{tenant_id}/tickets/{ticket_id}/messages", tag = "tickets",
    params(("tenant_id" = Uuid, Path), ("ticket_id" = Uuid, Path)), request_body = MessageInput,
    responses((status = 201, description = "Terkirim", body = SuccessEnvelope<TicketView>))
)]
pub async fn add_message(
    State(state): State<AppState>,
    ReqId(id): ReqId,
    Member(actor): Member,
    Path((_, tid)): Path<(Uuid, Uuid)>,
    Json(req): Json<MessageInput>,
) -> Result<(StatusCode, Json<SuccessEnvelope<TicketView>>), ApiError> {
    actor.require("tickets.reply")?;
    let body = text(&req.body, "body", BODY_MAX)?;
    let sc = scope(&actor);
    repo::staff_message(pool(&state)?, &actor, sc, tid, req.internal, &body)
        .await
        .map_err(ticket_err)?;
    Ok((
        StatusCode::CREATED,
        Json(SuccessEnvelope::new(
            reload(&state, actor.tenant_id, sc, tid).await?,
            &id,
        )),
    ))
}

/// Lampiran gambar (`?internal=true` untuk internal).
#[utoipa::path(
    post, path = "/api/v1/tenants/{tenant_id}/tickets/{ticket_id}/attachments", tag = "tickets",
    params(("tenant_id" = Uuid, Path), ("ticket_id" = Uuid, Path), AttachQuery),
    request_body(content = Vec<u8>, content_type = "image/png"),
    responses((status = 201, description = "Tersimpan"))
)]
pub async fn add_attachment(
    State(state): State<AppState>,
    Member(actor): Member,
    Path((_, tid)): Path<(Uuid, Uuid)>,
    Query(q): Query<AttachQuery>,
    body: Bytes,
) -> Result<StatusCode, ApiError> {
    actor.require("tickets.reply")?;
    repo::add_attachment(
        pool(&state)?,
        actor.tenant_id,
        scope(&actor),
        tid,
        &body,
        max_bytes(&state),
        q.internal,
        "staff",
    )
    .await
    .map_err(ticket_err)?;
    Ok(StatusCode::CREATED)
}

/// Unduh lampiran (staf).
#[utoipa::path(
    get, path = "/api/v1/tenants/{tenant_id}/tickets/{ticket_id}/attachments/{attachment_id}", tag = "tickets",
    params(("tenant_id" = Uuid, Path), ("ticket_id" = Uuid, Path), ("attachment_id" = Uuid, Path)),
    responses((status = 200, description = "Gambar", content_type = "image/png"))
)]
pub async fn get_attachment(
    State(state): State<AppState>,
    Member(actor): Member,
    Path((_, tid, aid)): Path<(Uuid, Uuid, Uuid)>,
) -> Result<Response, ApiError> {
    actor.require("tickets.read")?;
    let (ct, b) = repo::attachment(pool(&state)?, actor.tenant_id, scope(&actor), tid, aid)
        .await
        .map_err(internal)?
        .ok_or_else(not_found)?;
    bytes_response(ct, b)
}

// ------------------------------------------------------------------ customer (token order)

async fn order_scope(
    state: &AppState,
    t: TenantId,
    headers: &HeaderMap,
) -> Result<(Uuid, Option<Uuid>), ApiError> {
    let o = crate::commerce::order_from_token(state, t, headers).await?;
    Ok((o.id, Some(o.customer_id)))
}

/// Customer melaporkan masalah pesanan (AC-TKT-001 sumber `order`).
#[utoipa::path(
    post, path = "/api/v1/public/orders/current/tickets", tag = "public",
    params(("X-Order-Token" = String, Header)), request_body = CustomerTicketInput,
    responses((status = 201, description = "Dibuat", body = SuccessEnvelope<PublicTicket>))
)]
pub async fn customer_create(
    State(state): State<AppState>,
    ReqId(id): ReqId,
    ClientIp(ip): ClientIp,
    HostTenant(t): HostTenant,
    headers: HeaderMap,
    Json(req): Json<CustomerTicketInput>,
) -> Result<Response, ApiError> {
    crate::commerce::check_shop_origin(&state, &headers)?;
    crate::commerce::rate_limit(&state, &ip).await?;
    let (order, customer) = order_scope(&state, t, &headers).await?;
    let n = NewTicket {
        source: Source::Order,
        customer_id: customer,
        order_id: Some(order),
        conversation_id: None,
        subject: text(&req.subject, "subject", SUBJECT_MAX)?,
        description: opt_text(req.description, "description", BODY_MAX)?,
        priority: Priority::Normal,
        tags: vec![],
        assignee: None,
    };
    let p = pool(&state)?;
    let tid = repo::create(p, t, &n, None, (Some(id.as_str()), Some(&ip)))
        .await
        .map_err(ticket_err)?;
    let tk = repo::get(p, t, Scope::CustomerOrder(order), tid)
        .await
        .map_err(internal)?
        .ok_or_else(not_found)?;
    Ok((
        StatusCode::CREATED,
        Json(SuccessEnvelope::new(public_view(tk), &id)),
    )
        .into_response())
}

/// Ticket pesanan ini beserta balasan publik.
#[utoipa::path(
    get, path = "/api/v1/public/orders/current/tickets", tag = "public",
    params(("X-Order-Token" = String, Header)),
    responses((status = 200, description = "Ticket", body = SuccessEnvelope<Vec<PublicTicket>>))
)]
pub async fn customer_list(
    State(state): State<AppState>,
    ReqId(id): ReqId,
    HostTenant(t): HostTenant,
    headers: HeaderMap,
) -> Result<Response, ApiError> {
    let (order, _) = order_scope(&state, t, &headers).await?;
    let p = pool(&state)?;
    let sc = Scope::CustomerOrder(order);
    let (items, _) = repo::list(
        p,
        t,
        sc,
        &Filter {
            limit: 50,
            ..Filter::default()
        },
    )
    .await
    .map_err(internal)?;
    let mut out = vec![];
    for i in items {
        if let Some(tk) = repo::get(p, t, sc, i.id).await.map_err(internal)? {
            out.push(public_view(tk));
        }
    }
    let mut res = Json(SuccessEnvelope::new(out, &id)).into_response();
    res.headers_mut().insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static("private, no-store"),
    );
    Ok(res)
}

/// Balasan customer (WAITING_CUSTOMER → IN_PROGRESS; reopen sesuai kebijakan).
#[utoipa::path(
    post, path = "/api/v1/public/orders/current/tickets/{ticket_id}/messages", tag = "public",
    params(("X-Order-Token" = String, Header), ("ticket_id" = Uuid, Path)), request_body = CustomerMessageInput,
    responses((status = 201, description = "Terkirim", body = SuccessEnvelope<PublicTicket>))
)]
pub async fn customer_reply(
    State(state): State<AppState>,
    ReqId(id): ReqId,
    ClientIp(ip): ClientIp,
    HostTenant(t): HostTenant,
    Path(tid): Path<Uuid>,
    headers: HeaderMap,
    Json(req): Json<CustomerMessageInput>,
) -> Result<Response, ApiError> {
    crate::commerce::check_shop_origin(&state, &headers)?;
    crate::commerce::rate_limit(&state, &ip).await?;
    let (order, _) = order_scope(&state, t, &headers).await?;
    let body = text(&req.body, "body", BODY_MAX)?;
    let p = pool(&state)?;
    repo::customer_message(p, t, order, tid, &body)
        .await
        .map_err(ticket_err)?;
    let tk = repo::get(p, t, Scope::CustomerOrder(order), tid)
        .await
        .map_err(internal)?
        .ok_or_else(not_found)?;
    Ok((
        StatusCode::CREATED,
        Json(SuccessEnvelope::new(public_view(tk), &id)),
    )
        .into_response())
}

/// Lampiran dari customer (selalu publik).
#[utoipa::path(
    post, path = "/api/v1/public/orders/current/tickets/{ticket_id}/attachments", tag = "public",
    params(("X-Order-Token" = String, Header), ("ticket_id" = Uuid, Path)),
    request_body(content = Vec<u8>, content_type = "image/png"),
    responses((status = 201, description = "Tersimpan"))
)]
pub async fn customer_attach(
    State(state): State<AppState>,
    ClientIp(ip): ClientIp,
    HostTenant(t): HostTenant,
    Path(tid): Path<Uuid>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<StatusCode, ApiError> {
    crate::commerce::check_shop_origin(&state, &headers)?;
    crate::commerce::rate_limit(&state, &ip).await?;
    let (order, _) = order_scope(&state, t, &headers).await?;
    repo::add_attachment(
        pool(&state)?,
        t,
        Scope::CustomerOrder(order),
        tid,
        &body,
        max_bytes(&state),
        false,
        "customer",
    )
    .await
    .map_err(ticket_err)?;
    Ok(StatusCode::CREATED)
}

/// Unduh lampiran publik (customer).
#[utoipa::path(
    get, path = "/api/v1/public/orders/current/tickets/{ticket_id}/attachments/{attachment_id}", tag = "public",
    params(("X-Order-Token" = String, Header), ("ticket_id" = Uuid, Path), ("attachment_id" = Uuid, Path)),
    responses((status = 200, description = "Gambar", content_type = "image/png"))
)]
pub async fn customer_attachment(
    State(state): State<AppState>,
    HostTenant(t): HostTenant,
    Path((tid, aid)): Path<(Uuid, Uuid)>,
    headers: HeaderMap,
) -> Result<Response, ApiError> {
    let (order, _) = order_scope(&state, t, &headers).await?;
    let (ct, b) = repo::attachment(pool(&state)?, t, Scope::CustomerOrder(order), tid, aid)
        .await
        .map_err(internal)?
        .ok_or_else(not_found)?;
    bytes_response(ct, b)
}

pub fn routes() -> Router<AppState> {
    let t = "/api/v1/tenants/{tenant_id}/tickets";
    let c = "/api/v1/public/orders/current/tickets";
    Router::new()
        .route(t, get(list).post(create))
        .route(
            &format!("{t}/{{ticket_id}}"),
            get(get_one).patch(update_meta),
        )
        .route(&format!("{t}/{{ticket_id}}/assign"), post(assign))
        .route(&format!("{t}/{{ticket_id}}/status"), post(set_status))
        .route(&format!("{t}/{{ticket_id}}/messages"), post(add_message))
        .route(
            &format!("{t}/{{ticket_id}}/attachments"),
            post(add_attachment),
        )
        .route(
            &format!("{t}/{{ticket_id}}/attachments/{{attachment_id}}"),
            get(get_attachment),
        )
        .route(c, get(customer_list).post(customer_create))
        .route(&format!("{c}/{{ticket_id}}/messages"), post(customer_reply))
        .route(
            &format!("{c}/{{ticket_id}}/attachments"),
            post(customer_attach),
        )
        .route(
            &format!("{c}/{{ticket_id}}/attachments/{{attachment_id}}"),
            get(customer_attachment),
        )
}
