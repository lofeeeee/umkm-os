use std::time::Duration;

use axum::{Json, extract::State, response::IntoResponse};
use serde::Serialize;
use umkm_shared::http::{ErrorBody, ErrorCode, ErrorEnvelope, SuccessEnvelope};
use utoipa::ToSchema;

use crate::{ApiError, AppState, middleware::ReqId};

const CHECK_TIMEOUT: Duration = Duration::from_secs(2);

#[derive(Serialize, ToSchema)]
pub struct Status {
    pub status: &'static str,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct CheckResult {
    pub name: &'static str,
    pub ok: bool,
}

#[derive(Serialize, ToSchema)]
pub struct Readiness {
    pub status: &'static str,
    pub checks: Vec<CheckResult>,
}

/// Liveness: proses hidup. Tidak menyentuh dependensi.
#[utoipa::path(
    get,
    path = "/healthz",
    tag = "ops",
    responses((status = 200, description = "Proses hidup", body = SuccessEnvelope<Status>))
)]
pub async fn healthz(ReqId(id): ReqId) -> impl IntoResponse {
    Json(SuccessEnvelope::new(Status { status: "ok" }, &id))
}

/// Readiness: semua dependensi terdaftar siap. Gagal -> 503 tanpa detail internal.
/// Jalankan semua readiness check paralel (dipakai `/readyz` dan ringkasan platform P7-5).
pub async fn run_checks(state: &AppState) -> Vec<CheckResult> {
    // Semua check paralel; total latensi <= CHECK_TIMEOUT.
    let mut set = tokio::task::JoinSet::new();
    for (idx, c) in state.readiness.iter().cloned().enumerate() {
        set.spawn(async move {
            let ok = match tokio::time::timeout(CHECK_TIMEOUT, c.check()).await {
                Ok(Ok(())) => true,
                Ok(Err(e)) => {
                    tracing::warn!(check = c.name(), error = %e, "readiness check failed");
                    false
                }
                Err(_) => {
                    tracing::warn!(check = c.name(), "readiness check timed out");
                    false
                }
            };
            (idx, CheckResult { name: c.name(), ok })
        });
    }
    let mut results: Vec<(usize, CheckResult)> = set.join_all().await;
    results.sort_by_key(|(idx, _)| *idx);
    let checks: Vec<CheckResult> = results.into_iter().map(|(_, c)| c).collect();
    checks
}

#[utoipa::path(
    get,
    path = "/readyz",
    tag = "ops",
    responses(
        (status = 200, description = "Semua dependensi siap", body = SuccessEnvelope<Readiness>),
        (status = 503, description = "Ada dependensi tidak siap; `details.failing_checks` berisi nama check", body = ErrorEnvelope)
    )
)]
pub async fn readyz(
    State(state): State<AppState>,
    ReqId(id): ReqId,
) -> Result<Json<SuccessEnvelope<Readiness>>, ApiError> {
    let checks = run_checks(&state).await;
    if checks.iter().all(|c| c.ok) {
        return Ok(Json(SuccessEnvelope::new(
            Readiness {
                status: "ready",
                checks,
            },
            &id,
        )));
    }
    let failing: Vec<_> = checks.iter().filter(|c| !c.ok).map(|c| c.name).collect();
    Err(ErrorBody::new(ErrorCode::ServiceUnavailable)
        .with_details(serde_json::json!({ "failing_checks": failing }))
        .into())
}
