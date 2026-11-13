//! Audit log tenant (P4-5, `PRD.md §6.9`). Hanya `audit.read` (preset: Owner). Append-only:
//! tidak ada endpoint ubah/hapus.

use axum::{
    Json, Router,
    extract::{Query, State},
    routing::get,
};
use serde::{Deserialize, Serialize};
use umkm_infrastructure::audit_repo::{self as repo, AuditFilter};
use umkm_shared::http::SuccessEnvelope;
use utoipa::ToSchema;
use uuid::Uuid;

use crate::{
    ApiError, AppState,
    http_util::{Page, internal, pool},
    middleware::ReqId,
    session::Member,
    validation::one,
};

#[derive(Debug, Serialize, ToSchema)]
pub struct AuditView {
    pub id: Uuid,
    pub actor_type: String,
    pub actor_user_id: Option<Uuid>,
    pub action: String,
    pub resource_type: String,
    pub resource_id: Option<String>,
    pub request_id: Option<String>,
    pub ip: Option<String>,
    #[schema(value_type = Object)]
    pub summary: serde_json::Value,
    pub created_at: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct AuditList {
    pub items: Vec<AuditView>,
    pub total: i64,
}

#[derive(Debug, Deserialize, utoipa::IntoParams)]
#[into_params(parameter_in = Query)]
pub struct AuditQuery {
    /// Awalan aksi, mis. `order.`
    pub action: Option<String>,
    pub actor_user_id: Option<Uuid>,
    pub resource_type: Option<String>,
    pub resource_id: Option<String>,
    /// RFC3339
    pub from: Option<String>,
    /// RFC3339 (eksklusif)
    pub to: Option<String>,
    pub limit: Option<i64>,
    pub offset: Option<i64>,
}

fn rfc3339(v: Option<String>, path: &str) -> Result<Option<String>, ApiError> {
    match v {
        None => Ok(None),
        Some(s) => {
            let ok = s.len() <= 40
                && s.len() >= 10
                && s.as_bytes()[..4].iter().all(u8::is_ascii_digit)
                && s.bytes()
                    .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b':' | b'.' | b'+'));
            if ok {
                Ok(Some(s))
            } else {
                Err(one(path, "invalid_datetime"))
            }
        }
    }
}

/// Audit log toko (terbaru dulu, filter aksi/aktor/resource/waktu).
#[utoipa::path(
    get, path = "/api/v1/tenants/{tenant_id}/audit-logs", tag = "audit",
    params(("tenant_id" = Uuid, Path), AuditQuery),
    responses((status = 200, description = "Audit log", body = SuccessEnvelope<AuditList>))
)]
pub async fn list(
    State(state): State<AppState>,
    ReqId(id): ReqId,
    Member(actor): Member,
    Query(q): Query<AuditQuery>,
) -> Result<Json<SuccessEnvelope<AuditList>>, ApiError> {
    actor.require("audit.read")?;
    let (limit, offset) = Page {
        limit: q.limit,
        offset: q.offset,
    }
    .bounds();
    let action = q.action.filter(|a| !a.is_empty());
    if action.as_deref().is_some_and(|a| {
        a.len() > 60
            || !a
                .bytes()
                .all(|b| b.is_ascii_lowercase() || b == b'.' || b == b'_')
    }) {
        return Err(one("action", "invalid"));
    }
    let f = AuditFilter {
        action_prefix: action,
        actor_user_id: q.actor_user_id,
        resource_type: q.resource_type.filter(|s| !s.is_empty()),
        resource_id: q.resource_id.filter(|s| !s.is_empty()),
        from: rfc3339(q.from, "from")?,
        to: rfc3339(q.to, "to")?,
        limit,
        offset,
    };
    let (rows, total) = repo::list(pool(&state)?, actor.tenant_id, &f)
        .await
        .map_err(|e| {
            // Tanggal yang lolos pola tapi tak valid → 22007/22008 dari PostgreSQL.
            if e.as_database_error()
                .and_then(|d| d.code())
                .is_some_and(|c| c.starts_with("22"))
            {
                return one("from", "invalid_datetime");
            }
            internal(e)
        })?;
    Ok(Json(SuccessEnvelope::new(
        AuditList {
            items: rows
                .into_iter()
                .map(|r| AuditView {
                    id: r.id,
                    actor_type: r.actor_type,
                    actor_user_id: r.actor_user_id,
                    action: r.action,
                    resource_type: r.resource_type,
                    resource_id: r.resource_id,
                    request_id: r.request_id,
                    ip: r.ip,
                    summary: r.summary,
                    created_at: r.created_at,
                })
                .collect(),
            total,
        },
        &id,
    )))
}

pub fn routes() -> Router<AppState> {
    Router::new().route("/api/v1/tenants/{tenant_id}/audit-logs", get(list))
}
