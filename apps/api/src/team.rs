//! Tim & permission (P4-1, `PRD.md §3`). Baca `members.read`; ubah anggota/role `members.manage`;
//! undang `members.invite`. Anti-eskalasi: tidak ada pelaku yang dapat memberi/menyentuh
//! permission yang tidak ia miliki (termasuk lewat role kustom).

use axum::{
    Json, Router,
    extract::{Path, State},
    http::{HeaderMap, StatusCode, header},
    response::{IntoResponse, Response},
    routing::{delete, get, patch, post, put},
};
use serde::{Deserialize, Serialize};
use umkm_domain::{
    auth::{Email, PasswordHasher as _, rate_rules, validate_password},
    rbac::ALL_PERMISSIONS,
    team::{TeamError, parse_permissions, validate_role_name},
};
use umkm_infrastructure::{
    auth::{hash_token, new_token},
    auth_repo,
    team_repo::{self as repo, Accepter, Invitation, Member as TeamMember, Role, TeamRepoError},
};
use umkm_shared::http::{ErrorBody, ErrorCode, SuccessEnvelope};
use utoipa::ToSchema;
use uuid::Uuid;

use crate::{
    ApiError, AppState,
    auth::{ClientIp, SessionResponse, csrf_token},
    http_util::{conflict, internal, not_found, pool},
    middleware::ReqId,
    session::{Member, SessionUser},
    validation::one,
};

#[derive(Debug, Serialize, ToSchema)]
pub struct MemberView {
    pub membership_id: Uuid,
    pub user_id: Uuid,
    pub email: String,
    pub role_id: Uuid,
    pub role_name: String,
    /// `active` | `suspended`
    pub status: String,
    pub created_at: String,
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct MemberUpdate {
    pub role_id: Option<Uuid>,
    /// `active` | `suspended` | `removed`
    pub status: Option<String>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct RoleView {
    pub id: Uuid,
    pub name: String,
    pub is_system: bool,
    pub permissions: Vec<String>,
    pub members: i64,
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct RoleInput {
    pub name: String,
    pub permissions: Vec<String>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct InvitationView {
    pub id: Uuid,
    pub email: String,
    pub role_id: Uuid,
    pub role_name: String,
    pub expires_at: String,
    pub created_at: String,
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct InviteInput {
    pub email: String,
    pub role_id: Uuid,
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct AcceptInput {
    pub token: String,
    /// Wajib bila belum punya akun (tanpa sesi); diabaikan bila sudah login.
    pub password: Option<String>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct AcceptResult {
    pub tenant_id: Uuid,
    pub session: Option<SessionResponse>,
}

fn team_err(e: TeamRepoError) -> ApiError {
    match e {
        TeamRepoError::NotFound => not_found(),
        TeamRepoError::Rule(TeamError::Escalation) => ApiError(
            ErrorBody::new(ErrorCode::Forbidden)
                .with_message("Tidak boleh memberi izin yang tidak Anda miliki")
                .with_details(serde_json::json!({ "reason": "permission_escalation" })),
        ),
        TeamRepoError::Rule(TeamError::SelfChange) => {
            conflict("self_change", "Tidak dapat mengubah keanggotaan sendiri")
        }
        TeamRepoError::Rule(TeamError::LastOwner) => conflict(
            "last_owner",
            "Toko harus memiliki minimal satu Pemilik aktif",
        ),
        TeamRepoError::Rule(TeamError::UnknownPermission(_)) => {
            one("permissions", "unknown_permission")
        }
        TeamRepoError::Rule(TeamError::InvalidName) => one("name", "invalid"),
        TeamRepoError::SystemRole => conflict("system_role", "Peran bawaan tidak dapat diubah"),
        TeamRepoError::RoleInUse => {
            conflict("role_in_use", "Peran masih dipakai anggota atau undangan")
        }
        TeamRepoError::RoleNameTaken => conflict("role_name_taken", "Nama peran sudah dipakai"),
        TeamRepoError::AlreadyMember => {
            conflict("already_member", "Sudah menjadi anggota toko ini")
        }
        TeamRepoError::EmailMismatch => ApiError(
            ErrorBody::new(ErrorCode::Forbidden)
                .with_message("Undangan untuk email lain")
                .with_details(serde_json::json!({ "reason": "email_mismatch" })),
        ),
        TeamRepoError::AccountExists => conflict(
            "account_exists",
            "Email sudah punya akun: masuk dulu lalu terima undangan",
        ),
        TeamRepoError::Db(e) => internal(e),
    }
}

fn member_view(m: TeamMember) -> MemberView {
    MemberView {
        membership_id: m.membership_id,
        user_id: m.user_id,
        email: m.email,
        role_id: m.role_id,
        role_name: m.role_name,
        status: m.status,
        created_at: m.created_at,
    }
}

fn role_view(r: Role) -> RoleView {
    RoleView {
        id: r.id,
        name: r.name,
        is_system: r.is_system,
        permissions: r.permissions,
        members: r.members,
    }
}

fn invitation_view(i: Invitation) -> InvitationView {
    InvitationView {
        id: i.id,
        email: i.email,
        role_id: i.role_id,
        role_name: i.role_name,
        expires_at: i.expires_at,
        created_at: i.created_at,
    }
}

type Ok<T> = Result<Json<SuccessEnvelope<T>>, ApiError>;

/// Katalog permission yang dikenal (untuk editor role kustom).
#[utoipa::path(
    get, path = "/api/v1/permissions", tag = "team",
    responses((status = 200, description = "Permission", body = SuccessEnvelope<Vec<String>>))
)]
pub async fn permissions(ReqId(id): ReqId, _u: SessionUser) -> Ok<Vec<String>> {
    Ok(Json(SuccessEnvelope::new(
        ALL_PERMISSIONS.iter().map(|s| (*s).to_owned()).collect(),
        &id,
    )))
}

/// Anggota tim toko.
#[utoipa::path(
    get, path = "/api/v1/tenants/{tenant_id}/members", tag = "team",
    params(("tenant_id" = Uuid, Path)),
    responses((status = 200, description = "Anggota", body = SuccessEnvelope<Vec<MemberView>>))
)]
pub async fn list_members(
    State(state): State<AppState>,
    ReqId(id): ReqId,
    Member(actor): Member,
) -> Ok<Vec<MemberView>> {
    actor.require("members.read")?;
    let list = repo::list_members(pool(&state)?, actor.tenant_id)
        .await
        .map_err(internal)?;
    Ok(Json(SuccessEnvelope::new(
        list.into_iter().map(member_view).collect(),
        &id,
    )))
}

/// Ubah role/status anggota (`members.manage`). Anti-eskalasi + minimal satu Owner aktif.
#[utoipa::path(
    patch, path = "/api/v1/tenants/{tenant_id}/members/{membership_id}", tag = "team",
    params(("tenant_id" = Uuid, Path), ("membership_id" = Uuid, Path)), request_body = MemberUpdate,
    responses(
        (status = 200, description = "Tersimpan", body = SuccessEnvelope<Vec<MemberView>>),
        (status = 403, description = "Eskalasi hak", body = umkm_shared::http::ErrorEnvelope),
        (status = 409, description = "Owner terakhir / diri sendiri", body = umkm_shared::http::ErrorEnvelope)
    )
)]
pub async fn update_member(
    State(state): State<AppState>,
    ReqId(id): ReqId,
    ClientIp(ip): ClientIp,
    Member(actor): Member,
    Path((_, mid)): Path<(Uuid, Uuid)>,
    Json(req): Json<MemberUpdate>,
) -> Ok<Vec<MemberView>> {
    actor.require("members.manage")?;
    if let Some(s) = req.status.as_deref()
        && !matches!(s, "active" | "suspended" | "removed")
    {
        return Err(one("status", "invalid_status"));
    }
    let p = pool(&state)?;
    repo::update_member(
        p,
        &actor,
        mid,
        req.role_id,
        req.status.as_deref(),
        (Some(id.as_str()), Some(&ip)),
    )
    .await
    .map_err(team_err)?;
    let list = repo::list_members(p, actor.tenant_id)
        .await
        .map_err(internal)?;
    Ok(Json(SuccessEnvelope::new(
        list.into_iter().map(member_view).collect(),
        &id,
    )))
}

/// Role sistem + kustom beserta permission.
#[utoipa::path(
    get, path = "/api/v1/tenants/{tenant_id}/roles", tag = "team",
    params(("tenant_id" = Uuid, Path)),
    responses((status = 200, description = "Role", body = SuccessEnvelope<Vec<RoleView>>))
)]
pub async fn list_roles(
    State(state): State<AppState>,
    ReqId(id): ReqId,
    Member(actor): Member,
) -> Ok<Vec<RoleView>> {
    actor.require("members.read")?;
    let list = repo::list_roles(pool(&state)?, actor.tenant_id)
        .await
        .map_err(internal)?;
    Ok(Json(SuccessEnvelope::new(
        list.into_iter().map(role_view).collect(),
        &id,
    )))
}

async fn save_role(
    state: &AppState,
    actor: &umkm_domain::rbac::Actor,
    rid: Option<Uuid>,
    req: RoleInput,
    ctx: (Option<&str>, Option<&str>),
) -> Result<Uuid, ApiError> {
    let name = validate_role_name(&req.name).map_err(|_| one("name", "invalid"))?;
    let perms = parse_permissions(&req.permissions)
        .map_err(|_| one("permissions", "unknown_permission"))?;
    repo::save_role(pool(state)?, actor, rid, &name, &perms, ctx)
        .await
        .map_err(team_err)
}

/// Buat role kustom (`members.manage`); permission ⊆ milik pelaku.
#[utoipa::path(
    post, path = "/api/v1/tenants/{tenant_id}/roles", tag = "team",
    params(("tenant_id" = Uuid, Path)), request_body = RoleInput,
    responses((status = 201, description = "Dibuat", body = SuccessEnvelope<Vec<RoleView>>))
)]
pub async fn create_role(
    State(state): State<AppState>,
    ReqId(id): ReqId,
    ClientIp(ip): ClientIp,
    Member(actor): Member,
    Json(req): Json<RoleInput>,
) -> Result<(StatusCode, Json<SuccessEnvelope<Vec<RoleView>>>), ApiError> {
    actor.require("members.manage")?;
    save_role(&state, &actor, None, req, (Some(id.as_str()), Some(&ip))).await?;
    let list = repo::list_roles(pool(&state)?, actor.tenant_id)
        .await
        .map_err(internal)?;
    Ok((
        StatusCode::CREATED,
        Json(SuccessEnvelope::new(
            list.into_iter().map(role_view).collect(),
            &id,
        )),
    ))
}

/// Ubah role kustom.
#[utoipa::path(
    put, path = "/api/v1/tenants/{tenant_id}/roles/{role_id}", tag = "team",
    params(("tenant_id" = Uuid, Path), ("role_id" = Uuid, Path)), request_body = RoleInput,
    responses((status = 200, description = "Tersimpan", body = SuccessEnvelope<Vec<RoleView>>))
)]
pub async fn update_role(
    State(state): State<AppState>,
    ReqId(id): ReqId,
    ClientIp(ip): ClientIp,
    Member(actor): Member,
    Path((_, rid)): Path<(Uuid, Uuid)>,
    Json(req): Json<RoleInput>,
) -> Ok<Vec<RoleView>> {
    actor.require("members.manage")?;
    save_role(
        &state,
        &actor,
        Some(rid),
        req,
        (Some(id.as_str()), Some(&ip)),
    )
    .await?;
    let list = repo::list_roles(pool(&state)?, actor.tenant_id)
        .await
        .map_err(internal)?;
    Ok(Json(SuccessEnvelope::new(
        list.into_iter().map(role_view).collect(),
        &id,
    )))
}

/// Hapus role kustom yang tidak dipakai.
#[utoipa::path(
    delete, path = "/api/v1/tenants/{tenant_id}/roles/{role_id}", tag = "team",
    params(("tenant_id" = Uuid, Path), ("role_id" = Uuid, Path)),
    responses((status = 204, description = "Dihapus"))
)]
pub async fn delete_role(
    State(state): State<AppState>,
    ReqId(id): ReqId,
    ClientIp(ip): ClientIp,
    Member(actor): Member,
    Path((_, rid)): Path<(Uuid, Uuid)>,
) -> Result<StatusCode, ApiError> {
    actor.require("members.manage")?;
    repo::delete_role(pool(&state)?, &actor, rid, (Some(id.as_str()), Some(&ip)))
        .await
        .map_err(team_err)?;
    Ok(StatusCode::NO_CONTENT)
}

/// Undangan tertunda.
#[utoipa::path(
    get, path = "/api/v1/tenants/{tenant_id}/invitations", tag = "team",
    params(("tenant_id" = Uuid, Path)),
    responses((status = 200, description = "Undangan", body = SuccessEnvelope<Vec<InvitationView>>))
)]
pub async fn list_invitations(
    State(state): State<AppState>,
    ReqId(id): ReqId,
    Member(actor): Member,
) -> Ok<Vec<InvitationView>> {
    actor.require("members.read")?;
    let list = repo::list_invitations(pool(&state)?, actor.tenant_id)
        .await
        .map_err(internal)?;
    Ok(Json(SuccessEnvelope::new(
        list.into_iter().map(invitation_view).collect(),
        &id,
    )))
}

/// Undang anggota via email (AUTH-009). Email lewat outbox (D-08: adapter log-only).
#[utoipa::path(
    post, path = "/api/v1/tenants/{tenant_id}/invitations", tag = "team",
    params(("tenant_id" = Uuid, Path)), request_body = InviteInput,
    responses((status = 201, description = "Undangan dibuat", body = SuccessEnvelope<Vec<InvitationView>>))
)]
pub async fn invite(
    State(state): State<AppState>,
    ReqId(id): ReqId,
    ClientIp(ip): ClientIp,
    Member(actor): Member,
    Json(req): Json<InviteInput>,
) -> Result<(StatusCode, Json<SuccessEnvelope<Vec<InvitationView>>>), ApiError> {
    actor.require("members.invite")?;
    let email = Email::parse(&req.email).map_err(|_| one("email", "invalid_email"))?;
    // P7-2: anggota aktif + undangan tertunda (kursi yang dipesan) tidak boleh melebihi paket.
    let plan = crate::billing::plan_of(&state, actor.tenant_id).await?;
    let used = crate::billing::usage(&state, actor.tenant_id).await?;
    crate::billing::check_count(
        "max_members",
        plan.plan.limits.max_members,
        used.active_members + used.pending_invitations,
    )?;
    let p = pool(&state)?;
    repo::invite(
        p,
        &actor,
        &email.canonical(),
        req.role_id,
        (Some(id.as_str()), Some(&ip)),
    )
    .await
    .map_err(team_err)?;
    let list = repo::list_invitations(p, actor.tenant_id)
        .await
        .map_err(internal)?;
    Ok((
        StatusCode::CREATED,
        Json(SuccessEnvelope::new(
            list.into_iter().map(invitation_view).collect(),
            &id,
        )),
    ))
}

/// Cabut undangan.
#[utoipa::path(
    delete, path = "/api/v1/tenants/{tenant_id}/invitations/{invitation_id}", tag = "team",
    params(("tenant_id" = Uuid, Path), ("invitation_id" = Uuid, Path)),
    responses((status = 204, description = "Dicabut"))
)]
pub async fn revoke_invitation(
    State(state): State<AppState>,
    ReqId(id): ReqId,
    ClientIp(ip): ClientIp,
    Member(actor): Member,
    Path((_, iid)): Path<(Uuid, Uuid)>,
) -> Result<StatusCode, ApiError> {
    actor.require("members.invite")?;
    repo::revoke_invitation(pool(&state)?, &actor, iid, (Some(id.as_str()), Some(&ip)))
        .await
        .map_err(team_err)?;
    Ok(StatusCode::NO_CONTENT)
}

/// Terima undangan (AUTH-010, sekali pakai). Login → email akun harus sama dengan undangan.
/// Tanpa sesi → akun baru dibuat dengan password (token membuktikan kepemilikan inbox).
#[utoipa::path(
    post, path = "/api/v1/invitations/accept", tag = "team", request_body = AcceptInput,
    responses(
        (status = 200, description = "Bergabung", body = SuccessEnvelope<AcceptResult>),
        (status = 404, description = "Token tidak valid/kedaluwarsa/sudah dipakai", body = umkm_shared::http::ErrorEnvelope)
    )
)]
pub async fn accept(
    State(state): State<AppState>,
    ReqId(id): ReqId,
    ClientIp(ip): ClientIp,
    session: Option<SessionUser>,
    headers: HeaderMap,
    Json(req): Json<AcceptInput>,
) -> Result<Response, ApiError> {
    let svc = state
        .auth
        .as_deref()
        .ok_or_else(|| ApiError::new(ErrorCode::ServiceUnavailable))?;
    svc.check_origin(&headers)?;
    match svc
        .limiter
        .check(&rate_rules::VERIFY_TOKEN_PER_IP, &format!("invite:{ip}"))
        .await
    {
        Ok(d) if d.allowed => {}
        Ok(_) => return Err(ApiError::new(ErrorCode::RateLimited)),
        Err(_) => return Err(ApiError::new(ErrorCode::ServiceUnavailable)),
    }
    let token = req.token.trim();
    if token.is_empty() || token.len() > 128 {
        return Err(not_found());
    }
    let hash = hash_token(token);
    let p = &svc.pool;
    let (t, user, new_session) = match session {
        Some(s) => {
            let email: String = sqlx::query_scalar("SELECT email FROM users WHERE id = $1")
                .bind(s.user_id)
                .fetch_one(p)
                .await
                .map_err(internal)?;
            let (t, u) = repo::accept(
                p,
                hash,
                Accepter::Existing {
                    user_id: s.user_id,
                    email: &email,
                },
            )
            .await
            .map_err(team_err)?;
            (t, u, None)
        }
        None => {
            let pw = req.password.ok_or_else(|| one("password", "required"))?;
            let invited = repo::peek(p, hash)
                .await
                .map_err(internal)?
                .ok_or_else(not_found)?;
            let email = Email::parse(&invited).map_err(internal)?;
            validate_password(&pw, &email).map_err(|_| one("password", "weak_password"))?;
            let hasher = svc.hasher.clone();
            let hashed = tokio::task::spawn_blocking(move || hasher.hash(&pw))
                .await
                .map_err(internal)?
                .map_err(|_| ApiError::new(ErrorCode::InternalError))?;
            let (t, u) = repo::accept(
                p,
                hash,
                Accepter::New {
                    password_hash: &hashed,
                },
            )
            .await
            .map_err(team_err)?;
            let (token, th) = new_token().map_err(|_| ApiError::new(ErrorCode::InternalError))?;
            let sid = auth_repo::create_session(p, u, th, svc.policy.session_ttl, None)
                .await
                .map_err(|_| ApiError::new(ErrorCode::InternalError))?;
            (t, u, Some((token, sid)))
        }
    };
    let session = new_session.as_ref().map(|(tok, sid)| SessionResponse {
        user_id: user,
        session_id: *sid,
        csrf_token: csrf_token(tok),
    });
    let body = Json(SuccessEnvelope::new(
        AcceptResult {
            tenant_id: *t.as_uuid(),
            session,
        },
        &id,
    ));
    Ok(match new_session {
        Some((tok, _)) => ([(header::SET_COOKIE, svc.set_cookie(&tok))], body).into_response(),
        None => body.into_response(),
    })
}

pub fn routes() -> Router<AppState> {
    let t = "/api/v1/tenants/{tenant_id}";
    Router::new()
        .route("/api/v1/permissions", get(permissions))
        .route(&format!("{t}/members"), get(list_members))
        .route(
            &format!("{t}/members/{{membership_id}}"),
            patch(update_member),
        )
        .route(&format!("{t}/roles"), get(list_roles).post(create_role))
        .route(
            &format!("{t}/roles/{{role_id}}"),
            put(update_role).delete(delete_role),
        )
        .route(
            &format!("{t}/invitations"),
            get(list_invitations).post(invite),
        )
        .route(
            &format!("{t}/invitations/{{invitation_id}}"),
            delete(revoke_invitation),
        )
        .route("/api/v1/invitations/accept", post(accept))
}
