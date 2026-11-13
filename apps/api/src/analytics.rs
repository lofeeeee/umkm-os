//! Analitik percakapan (P6-6, `PRD.md §4.12`/§4.14, FR-CNV-004). Butuh `analytics.read`.
//! Definisi metrik (product-decision-log 2026-11-13):
//! - AI-resolved / human-resolved: percakapan ditutup tanpa / dengan pernah ditangani staf.
//! - Waktu respons: pesan pelanggan pertama → balasan AI/staf pertama; waktu resolusi: dibuat → ditutup.
//! - Handover rate: percakapan yang dialihkan AI→staf / percakapan baru.
//! - Fallback rate: giliran AI yang berakhir dengan pesan fallback tenant / giliran AI.
//! - Provider error rate: panggilan provider error/timeout / panggilan provider.

use axum::{
    Json, Router,
    extract::{Query, State},
    routing::get,
};
use serde::{Deserialize, Serialize};
use umkm_infrastructure::analytics_repo::{self as repo, AnalyticsError};
use umkm_shared::http::SuccessEnvelope;
use utoipa::{IntoParams, ToSchema};

use crate::{
    ApiError, AppState,
    http_util::{internal, pool},
    middleware::ReqId,
    session::Member,
    validation::one,
};

#[derive(Debug, Deserialize, IntoParams)]
pub struct RangeQuery {
    /// `YYYY-MM-DD` (Asia/Jakarta), default 29 hari sebelum `to`
    pub from: Option<String>,
    /// `YYYY-MM-DD` (inklusif), default hari ini; rentang maks 366 hari
    pub to: Option<String>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct TokenUsage {
    pub prompt: i64,
    pub completion: i64,
    pub total: i64,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct DayView {
    pub day: String,
    pub started: i64,
    pub handed_over: i64,
    pub closed_ai: i64,
    pub closed_human: i64,
    pub ai_turns: i64,
    pub tokens: i64,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct ConversationAnalyticsView {
    pub from: String,
    pub to: String,
    pub total_conversations: i64,
    pub ai_resolved: i64,
    pub human_resolved: i64,
    /// Rata-rata waktu respons pertama (ms); `null` bila belum ada data
    pub avg_first_response_ms: Option<i64>,
    /// Rata-rata waktu sampai ditutup (ms)
    pub avg_resolution_ms: Option<i64>,
    /// 0..1
    pub handover_rate: Option<f64>,
    /// 0..1
    pub fallback_rate: Option<f64>,
    pub tokens: TokenUsage,
    /// 0..1
    pub provider_error_rate: Option<f64>,
    pub ai_turns: i64,
    pub provider_calls: i64,
    pub daily: Vec<DayView>,
}

fn rate(n: i64, d: i64) -> Option<f64> {
    (d > 0).then(|| ((n as f64 / d as f64) * 10_000.0).round() / 10_000.0)
}

fn avg(sum: i64, n: i64) -> Option<i64> {
    (n > 0).then(|| sum / n)
}

fn valid_date(s: &str) -> bool {
    let b = s.as_bytes();
    b.len() == 10
        && b[4] == b'-'
        && b[7] == b'-'
        && b.iter()
            .enumerate()
            .all(|(i, c)| i == 4 || i == 7 || c.is_ascii_digit())
}

/// Analitik percakapan tenant (8 metrik + seri harian).
#[utoipa::path(
    get, path = "/api/v1/tenants/{tenant_id}/analytics/conversations", tag = "analytics",
    params(("tenant_id" = uuid::Uuid, Path), RangeQuery),
    responses(
        (status = 200, description = "Metrik percakapan", body = SuccessEnvelope<ConversationAnalyticsView>),
        (status = 403, description = "Permission kurang", body = umkm_shared::http::ErrorEnvelope),
        (status = 422, description = "Tanggal/rentang tidak valid", body = umkm_shared::http::ErrorEnvelope)
    )
)]
pub async fn conversations(
    State(state): State<AppState>,
    ReqId(id): ReqId,
    Member(actor): Member,
    Query(q): Query<RangeQuery>,
) -> Result<Json<SuccessEnvelope<ConversationAnalyticsView>>, ApiError> {
    actor.require("analytics.read")?;
    for (k, v) in [("from", &q.from), ("to", &q.to)] {
        if v.as_deref().is_some_and(|s| !valid_date(s)) {
            return Err(one(k, "invalid_date"));
        }
    }
    let a = repo::conversations(
        pool(&state)?,
        actor.tenant_id,
        q.from.as_deref(),
        q.to.as_deref(),
    )
    .await
    .map_err(|e| match e {
        AnalyticsError::InvalidRange => one("from", "invalid_range"),
        e => internal(e),
    })?;
    Ok(Json(SuccessEnvelope::new(
        ConversationAnalyticsView {
            avg_first_response_ms: avg(a.first_response_ms_sum, a.first_response_count),
            avg_resolution_ms: avg(a.resolution_ms_sum, a.resolution_count),
            handover_rate: rate(a.handed_over, a.conversations),
            fallback_rate: rate(a.fallback_turns, a.ai_turns),
            provider_error_rate: rate(a.provider_errors, a.provider_calls),
            tokens: TokenUsage {
                prompt: a.prompt_tokens,
                completion: a.completion_tokens,
                total: a.prompt_tokens + a.completion_tokens,
            },
            from: a.from,
            to: a.to,
            total_conversations: a.conversations,
            ai_resolved: a.ai_resolved,
            human_resolved: a.human_resolved,
            ai_turns: a.ai_turns,
            provider_calls: a.provider_calls,
            daily: a
                .daily
                .into_iter()
                .map(|d| DayView {
                    day: d.day,
                    started: d.started,
                    handed_over: d.handed_over,
                    closed_ai: d.closed_ai,
                    closed_human: d.closed_human,
                    ai_turns: d.ai_turns,
                    tokens: d.tokens,
                })
                .collect(),
        },
        &id,
    )))
}

pub fn routes() -> Router<AppState> {
    Router::new().route(
        "/api/v1/tenants/{tenant_id}/analytics/conversations",
        get(conversations),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rates_and_dates() {
        assert_eq!(rate(1, 3), Some(0.3333));
        assert_eq!(rate(0, 0), None);
        assert_eq!(avg(3000, 2), Some(1500));
        assert_eq!(avg(0, 0), None);
        assert!(valid_date("2026-11-13"));
        for bad in ["2026-1-13", "13-11-2026", "2026/11/13", "2026-11-1x", ""] {
            assert!(!valid_date(bad), "{bad}");
        }
    }
}
