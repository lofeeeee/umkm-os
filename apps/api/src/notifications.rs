//! Notifikasi in-app milik pengguna (P4-4, `PRD.md §4.13`). Hanya milik pemanggil di tenant path.

use axum::{
    Json, Router,
    extract::{Path, Query, State},
    http::StatusCode,
    routing::{get, post},
};
use serde::{Deserialize, Serialize};
use umkm_infrastructure::notification_repo as repo;
use umkm_shared::http::SuccessEnvelope;
use utoipa::ToSchema;
use uuid::Uuid;

use crate::{
    ApiError, AppState,
    http_util::{internal, not_found, pool},
    middleware::ReqId,
    session::Member,
};

#[derive(Debug, Serialize, ToSchema)]
pub struct NotificationView {
    pub id: Uuid,
    pub event_type: String,
    pub title: String,
    pub body: String,
    pub link: Option<String>,
    pub read: bool,
    pub created_at: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct NotificationList {
    pub items: Vec<NotificationView>,
    pub unread: i64,
}

#[derive(Debug, Deserialize, utoipa::IntoParams)]
#[into_params(parameter_in = Query)]
pub struct NotificationQuery {
    #[serde(default)]
    pub unread: bool,
    pub limit: Option<i64>,
}

/// Notifikasi saya di toko ini (terbaru dulu) + jumlah belum dibaca.
#[utoipa::path(
    get, path = "/api/v1/tenants/{tenant_id}/notifications", tag = "notifications",
    params(("tenant_id" = Uuid, Path), NotificationQuery),
    responses((status = 200, description = "Notifikasi", body = SuccessEnvelope<NotificationList>))
)]
pub async fn list(
    State(state): State<AppState>,
    ReqId(id): ReqId,
    Member(actor): Member,
    Query(q): Query<NotificationQuery>,
) -> Result<Json<SuccessEnvelope<NotificationList>>, ApiError> {
    actor.require("tenant.read")?;
    let (items, unread) = repo::list(
        pool(&state)?,
        actor.tenant_id,
        actor.user_id,
        q.unread,
        q.limit.unwrap_or(50).clamp(1, 200),
    )
    .await
    .map_err(internal)?;
    Ok(Json(SuccessEnvelope::new(
        NotificationList {
            items: items
                .into_iter()
                .map(|n| NotificationView {
                    id: n.id,
                    event_type: n.event_type,
                    title: n.title,
                    body: n.body,
                    link: n.link,
                    read: n.read,
                    created_at: n.created_at,
                })
                .collect(),
            unread,
        },
        &id,
    )))
}

/// Tandai satu notifikasi dibaca.
#[utoipa::path(
    post, path = "/api/v1/tenants/{tenant_id}/notifications/{notification_id}/read", tag = "notifications",
    params(("tenant_id" = Uuid, Path), ("notification_id" = Uuid, Path)),
    responses((status = 204, description = "Dibaca"), (status = 404, description = "Bukan milik Anda"))
)]
pub async fn read_one(
    State(state): State<AppState>,
    Member(actor): Member,
    Path((_, nid)): Path<(Uuid, Uuid)>,
) -> Result<StatusCode, ApiError> {
    actor.require("tenant.read")?;
    let n = repo::mark_read(pool(&state)?, actor.tenant_id, actor.user_id, Some(nid))
        .await
        .map_err(internal)?;
    if n == 0 {
        let (items, _) = repo::list(pool(&state)?, actor.tenant_id, actor.user_id, false, 200)
            .await
            .map_err(internal)?;
        if !items.iter().any(|x| x.id == nid) {
            return Err(not_found());
        }
    }
    Ok(StatusCode::NO_CONTENT)
}

/// Tandai semua dibaca.
#[utoipa::path(
    post, path = "/api/v1/tenants/{tenant_id}/notifications/read-all", tag = "notifications",
    params(("tenant_id" = Uuid, Path)),
    responses((status = 204, description = "Dibaca"))
)]
pub async fn read_all(
    State(state): State<AppState>,
    Member(actor): Member,
) -> Result<StatusCode, ApiError> {
    actor.require("tenant.read")?;
    repo::mark_read(pool(&state)?, actor.tenant_id, actor.user_id, None)
        .await
        .map_err(internal)?;
    Ok(StatusCode::NO_CONTENT)
}

pub fn routes() -> Router<AppState> {
    let t = "/api/v1/tenants/{tenant_id}/notifications";
    Router::new()
        .route(t, get(list))
        .route(&format!("{t}/read-all"), post(read_all))
        .route(&format!("{t}/{{notification_id}}/read"), post(read_one))
}
