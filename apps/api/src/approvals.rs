//! Approval engine (P4-2, `PRD.md §4.9`). Kebijakan: `tenant.update`. Daftar/detail:
//! `approvals.read`. Memutus: `approvals.decide` + penugasan langkah (role/user/delegasi).
//!
//! Anti-bypass (AC-APR-003): endpoint langsung yang dilindungi memanggil [`gate`]; bila
//! kebijakan aktif, perubahan TIDAK diterapkan — dibuat permintaan dan dibalas `202`.
//! Setelah langkah terakhir disetujui, [`execute`] menerapkan perubahan sebagai pemohon
//! dengan permission pemohon SAAT INI (hak yang dicabut tidak bisa dipakai lewat approval).

use axum::{
    Json, Router,
    extract::{Path, Query, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::{delete, get, post, put},
};
use serde::{Deserialize, Serialize};
use umkm_domain::{
    approvals::{ApprovalError, ENTITY_TYPES, EntityType, RequestStatus, Step, validate_policy},
    rbac::Actor,
};
use umkm_infrastructure::{
    approval_repo::{self as repo, ApprovalRepoError, NewRequest, Outcome, Policy, Request},
    rbac_repo,
};
use umkm_shared::{
    http::{ErrorBody, ErrorCode, RequestId, SuccessEnvelope},
    ids::TenantId,
};
use utoipa::ToSchema;
use uuid::Uuid;

use crate::{
    ApiError, AppState,
    auth::ClientIp,
    http_util::{Page, internal, not_found, pool},
    middleware::ReqId,
    session::Member,
    validation::one,
};

pub type Ctx<'a> = (Option<&'a str>, Option<&'a str>);

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct StepDto {
    #[serde(default)]
    pub role_ids: Vec<Uuid>,
    #[serde(default)]
    pub user_ids: Vec<Uuid>,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct PolicyDto {
    /// `order` | `refund` | `stock_adjustment` | `price_change` | `ai_response` | `business_config`
    #[serde(default)]
    pub entity_type: String,
    pub enabled: bool,
    pub steps: Vec<StepDto>,
    #[serde(default = "h48")]
    pub timeout_hours: i32,
}

fn h48() -> i32 {
    48
}

#[derive(Debug, Serialize, ToSchema)]
pub struct DecisionView {
    pub step: Option<i32>,
    pub decision: String,
    pub actor_user_id: Option<Uuid>,
    pub on_behalf_user_id: Option<Uuid>,
    pub reason: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct RequestView {
    pub id: Uuid,
    pub entity_type: String,
    pub entity_id: Option<Uuid>,
    pub action: String,
    pub summary: String,
    #[schema(value_type = Object)]
    pub payload: serde_json::Value,
    /// `pending` | `approved` | `rejected` | `expired` | `cancelled`
    pub status: String,
    pub current_step: usize,
    pub steps: Vec<StepDto>,
    pub requested_by: Uuid,
    pub expires_at: String,
    pub executed_at: Option<String>,
    pub execution_error: Option<String>,
    pub created_at: String,
    pub decisions: Vec<DecisionView>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct RequestList {
    pub items: Vec<RequestView>,
    pub total: i64,
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct DecideInput {
    /// Langkah yang diputus (0-based); harus langkah aktif
    pub step: usize,
    pub reason: Option<String>,
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct DelegateInput {
    pub to_user: Uuid,
    pub hours: i32,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct DelegationView {
    pub id: Uuid,
    pub from_user: Uuid,
    pub to_user: Uuid,
    pub starts_at: String,
    pub ends_at: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct PendingApproval {
    pub approval_request_id: Uuid,
    pub status: String,
}

#[derive(Debug, Deserialize, utoipa::IntoParams)]
pub struct ListQuery {
    pub status: Option<String>,
    pub limit: Option<i64>,
    pub offset: Option<i64>,
}

fn steps_dto(s: Vec<Step>) -> Vec<StepDto> {
    s.into_iter()
        .map(|s| StepDto {
            role_ids: s.role_ids,
            user_ids: s.user_ids,
        })
        .collect()
}

pub fn view(r: Request) -> RequestView {
    RequestView {
        id: r.id,
        entity_type: r.entity_type,
        entity_id: r.entity_id,
        action: r.action,
        summary: r.summary,
        payload: r.payload,
        status: r.status.as_str().into(),
        current_step: r.current_step,
        steps: steps_dto(r.steps),
        requested_by: r.requested_by,
        expires_at: r.expires_at,
        executed_at: r.executed_at,
        execution_error: r.execution_error,
        created_at: r.created_at,
        decisions: r
            .decisions
            .into_iter()
            .map(|d| DecisionView {
                step: d.step,
                decision: d.decision,
                actor_user_id: d.actor_user_id,
                on_behalf_user_id: d.on_behalf_user_id,
                reason: d.reason,
                created_at: d.created_at,
            })
            .collect(),
    }
}

fn policy_dto(p: Policy) -> PolicyDto {
    PolicyDto {
        entity_type: p.entity_type.as_str().into(),
        enabled: p.enabled,
        steps: steps_dto(p.steps),
        timeout_hours: p.timeout_hours,
    }
}

fn forbidden(reason: &str, msg: &str) -> ApiError {
    ApiError(
        ErrorBody::new(ErrorCode::Forbidden)
            .with_message(msg)
            .with_details(serde_json::json!({ "reason": reason })),
    )
}

fn repo_err(e: ApprovalRepoError) -> ApiError {
    use crate::http_util::conflict;
    match e {
        ApprovalRepoError::NotFound => not_found(),
        ApprovalRepoError::NoPolicy => conflict("no_policy", "Tidak ada kebijakan approval aktif"),
        ApprovalRepoError::AlreadyPending(id) => ApiError(
            ErrorBody::new(ErrorCode::Conflict)
                .with_message("Sudah ada permintaan approval yang menunggu")
                .with_details(
                    serde_json::json!({ "reason": "approval_pending", "approval_request_id": id }),
                ),
        ),
        ApprovalRepoError::Rule(ApprovalError::NotPending) => {
            conflict("not_pending", "Permintaan tidak lagi menunggu keputusan")
        }
        ApprovalRepoError::Rule(ApprovalError::WrongStep { expected }) => ApiError(
            ErrorBody::new(ErrorCode::Conflict)
                .with_message("Langkah sebelumnya belum diputus")
                .with_details(
                    serde_json::json!({ "reason": "wrong_step", "expected_step": expected }),
                ),
        ),
        ApprovalRepoError::Rule(ApprovalError::NotAssigned) => {
            forbidden("not_assigned", "Anda tidak ditugaskan untuk langkah ini")
        }
        ApprovalRepoError::Rule(ApprovalError::SelfApproval) => forbidden(
            "self_approval",
            "Pemohon tidak boleh menyetujui permintaannya sendiri",
        ),
        ApprovalRepoError::Rule(ApprovalError::RepeatApprover) => forbidden(
            "repeat_approver",
            "Penyetuju yang sama tidak boleh memutus dua langkah",
        ),
        ApprovalRepoError::Rule(ApprovalError::ReasonRequired) => one("reason", "required"),
        ApprovalRepoError::Rule(ApprovalError::InvalidPolicy(_)) => one("steps", "invalid_policy"),
        ApprovalRepoError::NotRequester => {
            forbidden("not_requester", "Hanya pemohon yang dapat membatalkan")
        }
        ApprovalRepoError::InvalidDelegate => one("users", "not_a_member"),
        ApprovalRepoError::Db(e) => internal(e),
    }
}

// ------------------------------------------------------------------ gate + executor

pub enum Gate {
    /// Tidak ada kebijakan aktif: terapkan langsung
    Direct,
    /// Permintaan dibuat; perubahan menunggu persetujuan
    Pending(Uuid),
}

/// Dipanggil endpoint yang dilindungi SEBELUM menerapkan perubahan.
#[allow(clippy::too_many_arguments)]
pub async fn gate(
    state: &AppState,
    actor: &Actor,
    entity_type: EntityType,
    entity_id: Option<Uuid>,
    action: &str,
    payload: serde_json::Value,
    summary: &str,
    ctx: Ctx<'_>,
) -> Result<Gate, ApiError> {
    let p = pool(state)?;
    if !repo::required(p, actor.tenant_id, entity_type)
        .await
        .map_err(internal)?
    {
        return Ok(Gate::Direct);
    }
    let id = repo::create(
        p,
        actor,
        NewRequest {
            entity_type,
            entity_id,
            action,
            payload,
            summary,
        },
        ctx,
    )
    .await
    .map_err(repo_err)?;
    Ok(Gate::Pending(id))
}

pub fn pending_response(id: Uuid, rid: &RequestId) -> Response {
    (
        StatusCode::ACCEPTED,
        Json(SuccessEnvelope::new(
            PendingApproval {
                approval_request_id: id,
                status: "pending".into(),
            },
            rid,
        )),
    )
        .into_response()
}

fn field<T: serde::de::DeserializeOwned>(p: &serde_json::Value, k: &str) -> Result<T, String> {
    serde_json::from_value(p.get(k).cloned().unwrap_or(serde_json::Value::Null))
        .map_err(|_| format!("payload {k} tidak valid"))
}

/// Terapkan permintaan yang disetujui sebagai pemohon (hak pemohon saat ini).
pub async fn execute(state: &AppState, t: TenantId, r: &Request) -> Result<(), String> {
    let p = pool(state).map_err(|_| "layanan tidak tersedia".to_owned())?;
    let actor = rbac_repo::load_actor(p, r.requested_by, t)
        .await
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "pemohon bukan lagi anggota aktif".to_owned())?;
    let need = |perm: &'static str| {
        actor
            .require(perm)
            .map_err(|_| format!("pemohon tidak lagi memiliki {perm}"))
    };
    let ctx = (None, None);
    let pl = &r.payload;
    match r.action.as_str() {
        "stock.mutate" => {
            need("inventory.adjust")?;
            let input: crate::inventory::MovementInput = field(pl, "input")?;
            let variant: Uuid = field(pl, "variant_id")?;
            crate::inventory::apply_movement(state, &actor, variant, input)
                .await
                .map(|_| ())
                .map_err(err_text)
        }
        "refund.create" => {
            need("payments.refund")?;
            let input: crate::commerce::RefundInput = field(pl, "input")?;
            let order: Uuid = field(pl, "order_id")?;
            crate::commerce::apply_refund(state, &actor, order, input, ctx)
                .await
                .map_err(err_text)
        }
        "commerce.settings" => {
            need("tenant.update")?;
            let input: crate::commerce::SettingsDto = field(pl, "input")?;
            crate::commerce::apply_settings(state, &actor, &input, ctx)
                .await
                .map(|_| ())
                .map_err(err_text)
        }
        "product.update" => {
            need("products.update")?;
            let input: crate::catalog::ProductInputDto = field(pl, "input")?;
            let product: Uuid = field(pl, "product_id")?;
            crate::catalog::apply_update(state, &actor, product, input, ctx)
                .await
                .map(|_| ())
                .map_err(err_text)
        }
        "order.approve" | "order.reject" => {
            let approve = r.action == "order.approve";
            need(if approve {
                "orders.approve"
            } else {
                "orders.reject"
            })?;
            let order: Uuid = field(pl, "order_id")?;
            let reason: Option<String> = field(pl, "reason")?;
            crate::commerce::apply_decision(state, &actor, order, approve, reason.as_deref(), ctx)
                .await
                .map_err(err_text)
        }
        // P6-4: draft balasan AI yang disetujui dikirim ke pelanggan atas nama pemohon.
        "ai_response.send" => {
            need("conversations.reply")?;
            let conv: Uuid = field(pl, "conversation_id")?;
            let draft: Uuid = field(pl, "draft_id")?;
            let body: String = field(pl, "body")?;
            crate::inbox::apply_send_draft(state, &actor, conv, draft, &body)
                .await
                .map(|_| ())
                .map_err(err_text)
        }
        other => Err(format!("belum ada executor untuk {other}")),
    }
}

fn err_text(e: ApiError) -> String {
    e.0.message.clone()
}

// ------------------------------------------------------------------ handler

/// Semua kebijakan approval tenant.
#[utoipa::path(
    get, path = "/api/v1/tenants/{tenant_id}/approval-policies", tag = "approvals",
    params(("tenant_id" = Uuid, Path)),
    responses((status = 200, description = "Kebijakan", body = SuccessEnvelope<Vec<PolicyDto>>))
)]
pub async fn list_policies(
    State(state): State<AppState>,
    ReqId(id): ReqId,
    Member(actor): Member,
) -> Result<Json<SuccessEnvelope<Vec<PolicyDto>>>, ApiError> {
    actor.require("approvals.read")?;
    let list = repo::list_policies(pool(&state)?, actor.tenant_id)
        .await
        .map_err(internal)?;
    Ok(Json(SuccessEnvelope::new(
        list.into_iter().map(policy_dto).collect(),
        &id,
    )))
}

/// Atur kebijakan satu tipe entity (`tenant.update`).
#[utoipa::path(
    put, path = "/api/v1/tenants/{tenant_id}/approval-policies/{entity_type}", tag = "approvals",
    params(("tenant_id" = Uuid, Path), ("entity_type" = String, Path)), request_body = PolicyDto,
    responses((status = 200, description = "Tersimpan", body = SuccessEnvelope<Vec<PolicyDto>>))
)]
pub async fn put_policy(
    State(state): State<AppState>,
    ReqId(id): ReqId,
    ClientIp(ip): ClientIp,
    Member(actor): Member,
    Path((_, et)): Path<(Uuid, String)>,
    Json(req): Json<PolicyDto>,
) -> Result<Json<SuccessEnvelope<Vec<PolicyDto>>>, ApiError> {
    actor.require("tenant.update")?;
    let entity_type = EntityType::parse(&et).ok_or_else(not_found)?;
    let steps: Vec<Step> = req
        .steps
        .into_iter()
        .map(|s| Step {
            role_ids: s.role_ids,
            user_ids: s.user_ids,
        })
        .collect();
    validate_policy(&steps, req.timeout_hours).map_err(|_| one("steps", "invalid_policy"))?;
    let p = pool(&state)?;
    repo::put_policy(
        p,
        &actor,
        &Policy {
            entity_type,
            enabled: req.enabled,
            steps,
            timeout_hours: req.timeout_hours,
        },
        (Some(id.as_str()), Some(&ip)),
    )
    .await
    .map_err(repo_err)?;
    let list = repo::list_policies(p, actor.tenant_id)
        .await
        .map_err(internal)?;
    Ok(Json(SuccessEnvelope::new(
        list.into_iter().map(policy_dto).collect(),
        &id,
    )))
}

/// Tipe entity yang didukung engine.
#[utoipa::path(
    get, path = "/api/v1/approval-entity-types", tag = "approvals",
    responses((status = 200, description = "Tipe", body = SuccessEnvelope<Vec<String>>))
)]
pub async fn entity_types(ReqId(id): ReqId) -> Json<SuccessEnvelope<Vec<String>>> {
    Json(SuccessEnvelope::new(
        ENTITY_TYPES.iter().map(|e| e.as_str().to_owned()).collect(),
        &id,
    ))
}

/// Daftar permintaan approval.
#[utoipa::path(
    get, path = "/api/v1/tenants/{tenant_id}/approvals", tag = "approvals",
    params(("tenant_id" = Uuid, Path), ListQuery),
    responses((status = 200, description = "Permintaan", body = SuccessEnvelope<RequestList>))
)]
pub async fn list(
    State(state): State<AppState>,
    ReqId(id): ReqId,
    Member(actor): Member,
    Query(q): Query<ListQuery>,
) -> Result<Json<SuccessEnvelope<RequestList>>, ApiError> {
    actor.require("approvals.read")?;
    let status = match q.status.as_deref() {
        None | Some("") => None,
        Some(s) => Some(RequestStatus::parse(s).ok_or_else(|| one("status", "invalid_status"))?),
    };
    let (limit, offset) = Page {
        limit: q.limit,
        offset: q.offset,
    }
    .bounds();
    let (items, total) = repo::list(pool(&state)?, actor.tenant_id, status, limit, offset)
        .await
        .map_err(internal)?;
    Ok(Json(SuccessEnvelope::new(
        RequestList {
            items: items.into_iter().map(view).collect(),
            total,
        },
        &id,
    )))
}

/// Detail + riwayat keputusan.
#[utoipa::path(
    get, path = "/api/v1/tenants/{tenant_id}/approvals/{approval_id}", tag = "approvals",
    params(("tenant_id" = Uuid, Path), ("approval_id" = Uuid, Path)),
    responses((status = 200, description = "Permintaan", body = SuccessEnvelope<RequestView>))
)]
pub async fn get_one(
    State(state): State<AppState>,
    ReqId(id): ReqId,
    Member(actor): Member,
    Path((_, aid)): Path<(Uuid, Uuid)>,
) -> Result<Json<SuccessEnvelope<RequestView>>, ApiError> {
    actor.require("approvals.read")?;
    let r = repo::get(pool(&state)?, actor.tenant_id, aid)
        .await
        .map_err(internal)?
        .ok_or_else(not_found)?;
    Ok(Json(SuccessEnvelope::new(view(r), &id)))
}

async fn decide(
    state: AppState,
    id: RequestId,
    ip: String,
    actor: Actor,
    aid: Uuid,
    req: DecideInput,
    approve: bool,
) -> Result<Json<SuccessEnvelope<RequestView>>, ApiError> {
    actor.require("approvals.decide")?;
    let reason = req
        .reason
        .map(|s| s.trim().chars().take(500).collect::<String>())
        .filter(|s| !s.is_empty());
    let p = pool(&state)?;
    let (outcome, r) = repo::decide(
        p,
        &actor,
        aid,
        req.step,
        approve,
        reason.as_deref(),
        (Some(id.as_str()), Some(&ip)),
    )
    .await
    .map_err(repo_err)?;
    if outcome == Outcome::Approved {
        let result = execute(&state, actor.tenant_id, &r).await;
        repo::mark_executed(p, actor.tenant_id, aid, result)
            .await
            .map_err(internal)?;
    }
    let r = repo::get(p, actor.tenant_id, aid)
        .await
        .map_err(internal)?
        .ok_or_else(not_found)?;
    Ok(Json(SuccessEnvelope::new(view(r), &id)))
}

/// Setujui langkah aktif. Langkah terakhir → perubahan diterapkan.
#[utoipa::path(
    post, path = "/api/v1/tenants/{tenant_id}/approvals/{approval_id}/approve", tag = "approvals",
    params(("tenant_id" = Uuid, Path), ("approval_id" = Uuid, Path)), request_body = DecideInput,
    responses(
        (status = 200, description = "Tercatat", body = SuccessEnvelope<RequestView>),
        (status = 403, description = "Tidak ditugaskan / pemohon sendiri", body = umkm_shared::http::ErrorEnvelope),
        (status = 409, description = "Langkah salah / tidak menunggu", body = umkm_shared::http::ErrorEnvelope)
    )
)]
pub async fn approve(
    State(state): State<AppState>,
    ReqId(id): ReqId,
    ClientIp(ip): ClientIp,
    Member(actor): Member,
    Path((_, aid)): Path<(Uuid, Uuid)>,
    Json(req): Json<DecideInput>,
) -> Result<Json<SuccessEnvelope<RequestView>>, ApiError> {
    decide(state, id, ip, actor, aid, req, true).await
}

/// Tolak langkah aktif (alasan wajib).
#[utoipa::path(
    post, path = "/api/v1/tenants/{tenant_id}/approvals/{approval_id}/reject", tag = "approvals",
    params(("tenant_id" = Uuid, Path), ("approval_id" = Uuid, Path)), request_body = DecideInput,
    responses((status = 200, description = "Ditolak", body = SuccessEnvelope<RequestView>))
)]
pub async fn reject(
    State(state): State<AppState>,
    ReqId(id): ReqId,
    ClientIp(ip): ClientIp,
    Member(actor): Member,
    Path((_, aid)): Path<(Uuid, Uuid)>,
    Json(req): Json<DecideInput>,
) -> Result<Json<SuccessEnvelope<RequestView>>, ApiError> {
    decide(state, id, ip, actor, aid, req, false).await
}

/// Pemohon membatalkan permintaannya.
#[utoipa::path(
    post, path = "/api/v1/tenants/{tenant_id}/approvals/{approval_id}/cancel", tag = "approvals",
    params(("tenant_id" = Uuid, Path), ("approval_id" = Uuid, Path)),
    responses((status = 204, description = "Dibatalkan"))
)]
pub async fn cancel(
    State(state): State<AppState>,
    ReqId(id): ReqId,
    ClientIp(ip): ClientIp,
    Member(actor): Member,
    Path((_, aid)): Path<(Uuid, Uuid)>,
) -> Result<StatusCode, ApiError> {
    repo::cancel(pool(&state)?, &actor, aid, (Some(id.as_str()), Some(&ip)))
        .await
        .map_err(repo_err)?;
    Ok(StatusCode::NO_CONTENT)
}

/// Delegasi aktif.
#[utoipa::path(
    get, path = "/api/v1/tenants/{tenant_id}/approval-delegations", tag = "approvals",
    params(("tenant_id" = Uuid, Path)),
    responses((status = 200, description = "Delegasi", body = SuccessEnvelope<Vec<DelegationView>>))
)]
pub async fn list_delegations(
    State(state): State<AppState>,
    ReqId(id): ReqId,
    Member(actor): Member,
) -> Result<Json<SuccessEnvelope<Vec<DelegationView>>>, ApiError> {
    actor.require("approvals.read")?;
    let list = repo::delegations(pool(&state)?, actor.tenant_id)
        .await
        .map_err(internal)?;
    Ok(Json(SuccessEnvelope::new(
        list.into_iter()
            .map(|d| DelegationView {
                id: d.id,
                from_user: d.from_user,
                to_user: d.to_user,
                starts_at: d.starts_at,
                ends_at: d.ends_at,
            })
            .collect(),
        &id,
    )))
}

/// Delegasikan wewenang approval sendiri (`approvals.decide`) ke anggota lain, maks 90 hari.
#[utoipa::path(
    post, path = "/api/v1/tenants/{tenant_id}/approval-delegations", tag = "approvals",
    params(("tenant_id" = Uuid, Path)), request_body = DelegateInput,
    responses((status = 201, description = "Dibuat"))
)]
pub async fn create_delegation(
    State(state): State<AppState>,
    ReqId(id): ReqId,
    ClientIp(ip): ClientIp,
    Member(actor): Member,
    Json(req): Json<DelegateInput>,
) -> Result<StatusCode, ApiError> {
    actor.require("approvals.decide")?;
    repo::delegate(
        pool(&state)?,
        &actor,
        req.to_user,
        req.hours,
        (Some(id.as_str()), Some(&ip)),
    )
    .await
    .map_err(repo_err)?;
    Ok(StatusCode::CREATED)
}

/// Cabut delegasi (hanya pemberi).
#[utoipa::path(
    delete, path = "/api/v1/tenants/{tenant_id}/approval-delegations/{delegation_id}", tag = "approvals",
    params(("tenant_id" = Uuid, Path), ("delegation_id" = Uuid, Path)),
    responses((status = 204, description = "Dicabut"))
)]
pub async fn revoke_delegation(
    State(state): State<AppState>,
    Member(actor): Member,
    Path((_, did)): Path<(Uuid, Uuid)>,
) -> Result<StatusCode, ApiError> {
    repo::revoke_delegation(pool(&state)?, &actor, did)
        .await
        .map_err(repo_err)?;
    Ok(StatusCode::NO_CONTENT)
}

pub fn routes() -> Router<AppState> {
    let t = "/api/v1/tenants/{tenant_id}";
    Router::new()
        .route("/api/v1/approval-entity-types", get(entity_types))
        .route(&format!("{t}/approval-policies"), get(list_policies))
        .route(
            &format!("{t}/approval-policies/{{entity_type}}"),
            put(put_policy),
        )
        .route(&format!("{t}/approvals"), get(list))
        .route(&format!("{t}/approvals/{{approval_id}}"), get(get_one))
        .route(
            &format!("{t}/approvals/{{approval_id}}/approve"),
            post(approve),
        )
        .route(
            &format!("{t}/approvals/{{approval_id}}/reject"),
            post(reject),
        )
        .route(
            &format!("{t}/approvals/{{approval_id}}/cancel"),
            post(cancel),
        )
        .route(
            &format!("{t}/approval-delegations"),
            get(list_delegations).post(create_delegation),
        )
        .route(
            &format!("{t}/approval-delegations/{{delegation_id}}"),
            delete(revoke_delegation),
        )
}
