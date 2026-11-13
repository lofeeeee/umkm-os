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

// ------------------------------------------------------------------ ringkasan toko (P7-5, FR-ANL-001)

#[derive(Debug, Serialize, ToSchema)]
pub struct TopProductView {
    pub name: String,
    pub quantity: i64,
    pub revenue: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct StockAlertView {
    pub product: String,
    pub variant: String,
    pub available: i64,
    pub min_stock: i64,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct TenantOverviewView {
    pub from: String,
    pub to: String,
    /// 1. Omzet (pesanan terbayar, Rupiah 2 desimal) & volume pesanan
    pub revenue: String,
    pub orders: i64,
    pub paid_orders: i64,
    /// 2. Rata-rata nilai pesanan terbayar (Rupiah 2 desimal); `null` bila belum ada
    pub average_order_value: Option<String>,
    /// 3. Produk terlaris (maks 5, dari snapshot item pesanan terbayar)
    pub top_products: Vec<TopProductView>,
    /// 4. Varian dengan stok tersedia ≤ ambang minimum
    pub stock_alerts: i64,
    pub stock_alert_items: Vec<StockAlertView>,
    /// 5. Pertumbuhan pelanggan
    pub customers_total: i64,
    pub customers_new: i64,
    /// 6. Tingkat pembatalan (dibatalkan/kedaluwarsa/ditolak ÷ pesanan), 0..1
    pub cancellation_rate: Option<f64>,
    /// 7. Volume tiket & rata-rata waktu penyelesaian (ms)
    pub tickets_created: i64,
    pub tickets_resolved: i64,
    pub avg_ticket_resolution_ms: Option<i64>,
    /// 8. AI resolution / handover (agregat percakapan P6-6)
    pub conversations: i64,
    pub ai_resolved: i64,
    pub human_resolved: i64,
    pub handover_rate: Option<f64>,
    /// 9. Pemakaian AI (token) & giliran chat
    pub ai_platform_tokens: i64,
    pub ai_byok_tokens: i64,
    pub ai_turns: i64,
}

/// Rata-rata dari string desimal 2 angka (tanpa float untuk uang): `total / n`, dibulatkan ke sen.
fn money_avg(total: &str, n: i64) -> Option<String> {
    if n <= 0 {
        return None;
    }
    let (int, frac) = total.split_once('.').unwrap_or((total, "0"));
    let frac = format!("{frac:0<2}");
    let sen: i128 = int.parse::<i128>().ok()? * 100 + frac[..2].parse::<i128>().ok()?;
    let n = i128::from(n);
    let avg = (sen + n / 2) / n;
    Some(format!("{}.{:02}", avg / 100, avg % 100))
}

/// Ringkasan analitik toko (9 metrik). Butuh `analytics.read`.
#[utoipa::path(
    get, path = "/api/v1/tenants/{tenant_id}/analytics/overview", tag = "analytics",
    params(("tenant_id" = uuid::Uuid, Path), RangeQuery),
    responses(
        (status = 200, description = "Ringkasan toko", body = SuccessEnvelope<TenantOverviewView>),
        (status = 403, description = "Permission kurang", body = umkm_shared::http::ErrorEnvelope),
        (status = 422, description = "Tanggal/rentang tidak valid", body = umkm_shared::http::ErrorEnvelope)
    )
)]
pub async fn overview(
    State(state): State<AppState>,
    ReqId(id): ReqId,
    Member(actor): Member,
    Query(q): Query<RangeQuery>,
) -> Result<Json<SuccessEnvelope<TenantOverviewView>>, ApiError> {
    actor.require("analytics.read")?;
    check_dates(&q)?;
    let o = repo::tenant_overview(
        pool(&state)?,
        actor.tenant_id,
        q.from.as_deref(),
        q.to.as_deref(),
    )
    .await
    .map_err(range_err)?;
    Ok(Json(SuccessEnvelope::new(
        TenantOverviewView {
            average_order_value: money_avg(&o.revenue, o.paid_orders),
            cancellation_rate: rate(o.cancelled_orders, o.orders),
            avg_ticket_resolution_ms: avg(o.ticket_resolution_ms_sum, o.tickets_resolved),
            handover_rate: rate(o.handed_over, o.conversations),
            from: o.from,
            to: o.to,
            revenue: o.revenue,
            orders: o.orders,
            paid_orders: o.paid_orders,
            top_products: o
                .top_products
                .into_iter()
                .map(|p| TopProductView {
                    name: p.name,
                    quantity: p.quantity,
                    revenue: p.revenue,
                })
                .collect(),
            stock_alerts: o.stock_alerts,
            stock_alert_items: o
                .stock_alert_items
                .into_iter()
                .map(|s| StockAlertView {
                    product: s.product,
                    variant: s.variant,
                    available: s.available,
                    min_stock: s.min_stock,
                })
                .collect(),
            customers_total: o.customers_total,
            customers_new: o.customers_new,
            tickets_created: o.tickets_created,
            tickets_resolved: o.tickets_resolved,
            conversations: o.conversations,
            ai_resolved: o.ai_resolved,
            human_resolved: o.human_resolved,
            ai_platform_tokens: o.ai_platform_tokens,
            ai_byok_tokens: o.ai_byok_tokens,
            ai_turns: o.ai_turns,
        },
        &id,
    )))
}

pub(crate) fn check_dates(q: &RangeQuery) -> Result<(), ApiError> {
    for (k, v) in [("from", &q.from), ("to", &q.to)] {
        if v.as_deref().is_some_and(|s| !valid_date(s)) {
            return Err(one(k, "invalid_date"));
        }
    }
    Ok(())
}

pub(crate) fn range_err(e: AnalyticsError) -> ApiError {
    match e {
        AnalyticsError::InvalidRange => one("from", "invalid_range"),
        e => internal(e),
    }
}

pub fn routes() -> Router<AppState> {
    Router::new()
        .route(
            "/api/v1/tenants/{tenant_id}/analytics/conversations",
            get(conversations),
        )
        .route(
            "/api/v1/tenants/{tenant_id}/analytics/overview",
            get(overview),
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
        assert_eq!(money_avg("100000.00", 3).as_deref(), Some("33333.33"));
        assert_eq!(money_avg("10.05", 2).as_deref(), Some("5.03"));
        assert_eq!(money_avg("85000", 1).as_deref(), Some("85000.00"));
        assert_eq!(money_avg("0.00", 0), None);
        assert!(valid_date("2026-11-13"));
        for bad in ["2026-1-13", "13-11-2026", "2026/11/13", "2026-11-1x", ""] {
            assert!(!valid_date(bad), "{bad}");
        }
    }
}
