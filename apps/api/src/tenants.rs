//! Endpoint tenant dashboard (`PRD.md §7.4`: `/tenants`, `/tenants/{id}`) — P1-5.

use axum::{Json, Router, extract::State, routing::get};
use serde::{Deserialize, Serialize};
use umkm_domain::tenant::validate_business_name;
use umkm_infrastructure::rbac_repo;
use umkm_shared::http::{ErrorBody, ErrorCode, SuccessEnvelope};
use utoipa::ToSchema;
use uuid::Uuid;

use crate::{
    ApiError, AppState,
    middleware::ReqId,
    session::{Member, SessionUser},
};

#[derive(Debug, Serialize, ToSchema)]
pub struct MembershipItem {
    pub tenant_id: Uuid,
    pub name: String,
    pub slug: String,
    pub role: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct TenantView {
    pub id: Uuid,
    pub name: String,
    pub slug: String,
    pub status: String,
    pub primary_hostname: Option<String>,
    /// Role & permission pemanggil di tenant ini (untuk UI menyembunyikan aksi).
    pub my_role: String,
    pub my_permissions: Vec<String>,
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct UpdateTenantRequest {
    pub name: String,
}

fn internal(e: sqlx::Error) -> ApiError {
    tracing::error!(error = %e, "tenant query failed");
    ApiError::new(ErrorCode::InternalError)
}

/// Tenant tempat pemanggil menjadi anggota aktif (AUTH-008).
#[utoipa::path(
    get, path = "/api/v1/tenants", tag = "tenants",
    responses(
        (status = 200, description = "Daftar membership aktif", body = SuccessEnvelope<Vec<MembershipItem>>),
        (status = 401, description = "Tidak ada sesi valid", body = umkm_shared::http::ErrorEnvelope)
    )
)]
pub async fn list_tenants(
    State(state): State<AppState>,
    ReqId(id): ReqId,
    user: SessionUser,
) -> Result<Json<SuccessEnvelope<Vec<MembershipItem>>>, ApiError> {
    let pool = &state
        .auth
        .as_ref()
        .ok_or_else(|| ApiError::new(ErrorCode::ServiceUnavailable))?
        .pool;
    let items = rbac_repo::list_memberships(pool, user.user_id)
        .await
        .map_err(internal)?
        .into_iter()
        .map(|m| MembershipItem {
            tenant_id: m.tenant_id,
            name: m.name,
            slug: m.slug,
            role: m.role_name,
        })
        .collect();
    Ok(Json(SuccessEnvelope::new(items, &id)))
}

async fn view(state: &AppState, actor: &umkm_domain::rbac::Actor) -> Result<TenantView, ApiError> {
    let pool = &state
        .auth
        .as_ref()
        .ok_or_else(|| ApiError::new(ErrorCode::ServiceUnavailable))?
        .pool;
    let t = rbac_repo::get_tenant(pool, actor)
        .await
        .map_err(internal)?
        .ok_or_else(|| ApiError::new(ErrorCode::ResourceNotFound))?;
    Ok(TenantView {
        id: t.id,
        name: t.name,
        slug: t.slug,
        status: t.status,
        primary_hostname: t.primary_hostname,
        my_role: actor.role_name.clone(),
        my_permissions: actor.permissions().map(str::to_owned).collect(),
    })
}

/// Detail tenant. Butuh `tenant.read`. Bukan anggota -> 404.
#[utoipa::path(
    get, path = "/api/v1/tenants/{tenant_id}", tag = "tenants",
    params(("tenant_id" = Uuid, Path, description = "Selector tenant; diverifikasi terhadap membership")),
    responses(
        (status = 200, description = "Detail tenant", body = SuccessEnvelope<TenantView>),
        (status = 401, description = "Tidak ada sesi valid", body = umkm_shared::http::ErrorEnvelope),
        (status = 403, description = "Permission kurang; `details.required_permission`", body = umkm_shared::http::ErrorEnvelope),
        (status = 404, description = "Tenant tidak ada atau bukan anggota", body = umkm_shared::http::ErrorEnvelope)
    )
)]
pub async fn get_tenant(
    State(state): State<AppState>,
    ReqId(id): ReqId,
    Member(actor): Member,
) -> Result<Json<SuccessEnvelope<TenantView>>, ApiError> {
    actor.require("tenant.read")?;
    Ok(Json(SuccessEnvelope::new(view(&state, &actor).await?, &id)))
}

/// Ubah nama tenant. Butuh `tenant.update` + `X-CSRF-Token`.
#[utoipa::path(
    patch, path = "/api/v1/tenants/{tenant_id}", tag = "tenants",
    params(("tenant_id" = Uuid, Path, description = "Selector tenant; diverifikasi terhadap membership")),
    request_body = UpdateTenantRequest,
    responses(
        (status = 200, description = "Tenant diperbarui", body = SuccessEnvelope<TenantView>),
        (status = 401, description = "Tidak ada sesi valid", body = umkm_shared::http::ErrorEnvelope),
        (status = 403, description = "Permission kurang atau CSRF/Origin tidak valid", body = umkm_shared::http::ErrorEnvelope),
        (status = 404, description = "Tenant tidak ada atau bukan anggota", body = umkm_shared::http::ErrorEnvelope),
        (status = 422, description = "Nama tidak valid", body = umkm_shared::http::ErrorEnvelope)
    )
)]
pub async fn update_tenant(
    State(state): State<AppState>,
    ReqId(id): ReqId,
    Member(actor): Member,
    crate::auth::ClientIp(ip): crate::auth::ClientIp,
    Json(req): Json<UpdateTenantRequest>,
) -> Result<Json<SuccessEnvelope<TenantView>>, ApiError> {
    actor.require("tenant.update")?;
    let name = validate_business_name(&req.name).map_err(|e| {
        ApiError(
            ErrorBody::new(ErrorCode::ValidationFailed)
                .with_message(e.to_string())
                .with_details(serde_json::json!({"field": "name"})),
        )
    })?;
    let pool = &state
        .auth
        .as_ref()
        .ok_or_else(|| ApiError::new(ErrorCode::ServiceUnavailable))?
        .pool;
    rbac_repo::rename_tenant(pool, &actor, &name, Some(id.as_str()), Some(&ip))
        .await
        .map_err(internal)?;
    tracing::info!(user_id = %actor.user_id, tenant_id = %actor.tenant_id, "tenant renamed");
    Ok(Json(SuccessEnvelope::new(view(&state, &actor).await?, &id)))
}

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/api/v1/tenants", get(list_tenants))
        .route(
            "/api/v1/tenants/{tenant_id}",
            get(get_tenant).patch(update_tenant),
        )
}
