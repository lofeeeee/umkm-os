//! Analitik percakapan (P6-6, `PRD.md §4.12`, FR-CNV-004). Tenant context (RLS): agregasi hanya data
//! tenant sendiri (AC-CNV-004). Sumber: `conversation_daily_stats` (agregat yang bertahan setelah purge
//! percakapan, ADR-0012) + `ai_runs` bertujuan `chat`. Hari = tanggal Asia/Jakarta.

use sqlx::{PgPool, Row};
use umkm_shared::ids::TenantId;

use crate::pgx::{sqlstate, tenant_tx};

#[derive(Debug, thiserror::Error)]
pub enum AnalyticsError {
    #[error("rentang tanggal tidak valid")]
    InvalidRange,
    #[error(transparent)]
    Db(#[from] sqlx::Error),
}

/// Rentang maksimum satu permintaan (hari, inklusif).
pub const MAX_DAYS: i32 = 366;

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Day {
    pub day: String,
    pub started: i64,
    pub handed_over: i64,
    pub closed_ai: i64,
    pub closed_human: i64,
    pub ai_turns: i64,
    pub tokens: i64,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct ConversationAnalytics {
    pub from: String,
    pub to: String,
    pub conversations: i64,
    pub ai_resolved: i64,
    pub human_resolved: i64,
    pub handed_over: i64,
    pub first_response_count: i64,
    pub first_response_ms_sum: i64,
    pub resolution_count: i64,
    pub resolution_ms_sum: i64,
    /// Giliran AI percakapan (ai_runs purpose chat).
    pub ai_turns: i64,
    /// Giliran yang berakhir dengan pesan fallback tenant (decision handover).
    pub fallback_turns: i64,
    /// Giliran yang benar-benar memanggil provider.
    pub provider_calls: i64,
    /// Panggilan provider yang gagal (error/timeout).
    pub provider_errors: i64,
    pub prompt_tokens: i64,
    pub completion_tokens: i64,
    pub daily: Vec<Day>,
}

/// `from`/`to` = `YYYY-MM-DD` (inklusif); `None` = 30 hari terakhir (Asia/Jakarta).
pub async fn conversations(
    pool: &PgPool,
    t: TenantId,
    from: Option<&str>,
    to: Option<&str>,
) -> Result<ConversationAnalytics, AnalyticsError> {
    let mut tx = tenant_tx(pool, t).await?;
    let bad_date = |e: sqlx::Error| match sqlstate(&e).as_deref() {
        Some("22007" | "22008" | "22P02") => AnalyticsError::InvalidRange,
        _ => AnalyticsError::Db(e),
    };
    let r = sqlx::query(
        "SELECT to_char(f, 'YYYY-MM-DD') AS f, to_char(t, 'YYYY-MM-DD') AS t, (t - f) AS span FROM (
            SELECT coalesce($1::date, app.jkt_day(now()) - 29) AS f, coalesce($2::date, app.jkt_day(now())) AS t) x",
    )
    .bind(from)
    .bind(to)
    .fetch_one(&mut *tx)
    .await
    .map_err(bad_date)?;
    let span: i32 = r.get("span");
    if !(0..MAX_DAYS).contains(&span) {
        return Err(AnalyticsError::InvalidRange);
    }
    let (f, to_s): (String, String) = (r.get("f"), r.get("t"));

    let s = sqlx::query(
        "SELECT coalesce(sum(started), 0)::bigint AS started, coalesce(sum(handed_over), 0)::bigint AS handed,
                coalesce(sum(closed_ai), 0)::bigint AS cai, coalesce(sum(closed_human), 0)::bigint AS chu,
                coalesce(sum(first_response_count), 0)::bigint AS frc, coalesce(sum(first_response_ms_sum), 0)::bigint AS frs,
                coalesce(sum(resolution_count), 0)::bigint AS rc, coalesce(sum(resolution_ms_sum), 0)::bigint AS rs
           FROM conversation_daily_stats WHERE tenant_id = $1 AND day BETWEEN $2::date AND $3::date",
    )
    .bind(t.as_uuid())
    .bind(&f)
    .bind(&to_s)
    .fetch_one(&mut *tx)
    .await?;
    let ai = sqlx::query(
        "SELECT count(*) AS turns,
                count(*) FILTER (WHERE decision = 'handover') AS fallback,
                count(*) FILTER (WHERE model <> '-' AND status NOT IN ('blocked', 'quota_exceeded', 'circuit_open')) AS calls,
                count(*) FILTER (WHERE status IN ('error', 'timeout')) AS errors,
                coalesce(sum(prompt_tokens), 0)::bigint AS pt, coalesce(sum(completion_tokens), 0)::bigint AS ct
           FROM ai_runs WHERE tenant_id = $1 AND purpose = 'chat'
            AND app.jkt_day(created_at) BETWEEN $2::date AND $3::date",
    )
    .bind(t.as_uuid())
    .bind(&f)
    .bind(&to_s)
    .fetch_one(&mut *tx)
    .await?;
    let daily = sqlx::query(
        "SELECT to_char(d::date, 'YYYY-MM-DD') AS day,
                coalesce(s.started, 0)::bigint AS started, coalesce(s.handed_over, 0)::bigint AS handed,
                coalesce(s.closed_ai, 0)::bigint AS cai, coalesce(s.closed_human, 0)::bigint AS chu,
                coalesce(r.turns, 0)::bigint AS turns, coalesce(r.tokens, 0)::bigint AS tokens
           FROM generate_series($2::date, $3::date, interval '1 day') d
           LEFT JOIN conversation_daily_stats s ON s.tenant_id = $1 AND s.day = d::date
           LEFT JOIN (SELECT app.jkt_day(created_at) AS day, count(*) AS turns,
                             sum(prompt_tokens + completion_tokens) AS tokens
                        FROM ai_runs WHERE tenant_id = $1 AND purpose = 'chat'
                         AND app.jkt_day(created_at) BETWEEN $2::date AND $3::date
                       GROUP BY 1) r ON r.day = d::date
          ORDER BY d",
    )
    .bind(t.as_uuid())
    .bind(&f)
    .bind(&to_s)
    .fetch_all(&mut *tx)
    .await?
    .iter()
    .map(|r| Day {
        day: r.get("day"),
        started: r.get("started"),
        handed_over: r.get("handed"),
        closed_ai: r.get("cai"),
        closed_human: r.get("chu"),
        ai_turns: r.get("turns"),
        tokens: r.get("tokens"),
    })
    .collect();
    tx.commit().await?;
    Ok(ConversationAnalytics {
        from: f,
        to: to_s,
        conversations: s.get("started"),
        ai_resolved: s.get("cai"),
        human_resolved: s.get("chu"),
        handed_over: s.get("handed"),
        first_response_count: s.get("frc"),
        first_response_ms_sum: s.get("frs"),
        resolution_count: s.get("rc"),
        resolution_ms_sum: s.get("rs"),
        ai_turns: ai.get("turns"),
        fallback_turns: ai.get("fallback"),
        provider_calls: ai.get("calls"),
        provider_errors: ai.get("errors"),
        prompt_tokens: ai.get("pt"),
        completion_tokens: ai.get("ct"),
        daily,
    })
}

// ------------------------------------------------------------------ ringkasan toko & platform (P7-5)

/// Rentang tanggal WIB tervalidasi (default 30 hari terakhir, maks `MAX_DAYS`).
async fn resolve_range<'e>(
    db: impl sqlx::PgExecutor<'e>,
    from: Option<&str>,
    to: Option<&str>,
) -> Result<(String, String), AnalyticsError> {
    let r = sqlx::query(
        "SELECT to_char(f, 'YYYY-MM-DD') AS f, to_char(t, 'YYYY-MM-DD') AS t, (t - f) AS span FROM (
            SELECT coalesce($1::date, app.jkt_day(now()) - 29) AS f, coalesce($2::date, app.jkt_day(now())) AS t) x",
    )
    .bind(from)
    .bind(to)
    .fetch_one(db)
    .await
    .map_err(|e| match sqlstate(&e).as_deref() {
        Some("22007" | "22008" | "22P02") => AnalyticsError::InvalidRange,
        _ => AnalyticsError::Db(e),
    })?;
    let span: i32 = r.get("span");
    if !(0..MAX_DAYS).contains(&span) {
        return Err(AnalyticsError::InvalidRange);
    }
    Ok((r.get("f"), r.get("t")))
}

/// Status pesanan yang dihitung sebagai omzet (pembayaran sudah dikonfirmasi; belum dibatalkan/dikembalikan).
pub const REVENUE_STATUSES: [&str; 7] = [
    "PAID",
    "APPROVAL_PENDING",
    "APPROVED",
    "PROCESSING",
    "READY_FOR_PICKUP",
    "SHIPPED",
    "COMPLETED",
];
/// Status pesanan gagal untuk tingkat pembatalan.
pub const CANCELLED_STATUSES: [&str; 3] = ["CANCELLED", "EXPIRED", "REJECTED"];

#[derive(Debug, Clone, PartialEq)]
pub struct TopProduct {
    pub name: String,
    pub quantity: i64,
    pub revenue: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct StockAlert {
    pub product: String,
    pub variant: String,
    pub available: i64,
    pub min_stock: i64,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct TenantOverview {
    pub from: String,
    pub to: String,
    pub orders: i64,
    pub paid_orders: i64,
    /// Rupiah, 2 desimal (string agar tidak kehilangan presisi).
    pub revenue: String,
    pub cancelled_orders: i64,
    pub top_products: Vec<TopProduct>,
    pub stock_alerts: i64,
    pub stock_alert_items: Vec<StockAlert>,
    pub customers_total: i64,
    pub customers_new: i64,
    pub tickets_created: i64,
    pub tickets_resolved: i64,
    pub ticket_resolution_ms_sum: i64,
    pub conversations: i64,
    pub ai_resolved: i64,
    pub human_resolved: i64,
    pub handed_over: i64,
    pub ai_platform_tokens: i64,
    pub ai_byok_tokens: i64,
    pub ai_turns: i64,
}

/// Ringkasan 9 metrik toko (FR-ANL-001) — semua dalam tenant context (RLS) + filter tenant eksplisit.
pub async fn tenant_overview(
    pool: &PgPool,
    t: TenantId,
    from: Option<&str>,
    to: Option<&str>,
) -> Result<TenantOverview, AnalyticsError> {
    let mut tx = tenant_tx(pool, t).await?;
    let (f, to_s) = resolve_range(&mut *tx, from, to).await?;
    let in_range = "app.jkt_day(created_at) BETWEEN $2::date AND $3::date";
    let o = sqlx::query(sqlx::AssertSqlSafe(format!(
        "SELECT count(*) AS orders,
                count(*) FILTER (WHERE status = ANY($4)) AS paid,
                coalesce(sum(total) FILTER (WHERE status = ANY($4)), 0)::numeric(14,2)::text AS revenue,
                count(*) FILTER (WHERE status = ANY($5)) AS cancelled
           FROM orders WHERE tenant_id = $1 AND {in_range}"
    )))
    .bind(t.as_uuid())
    .bind(&f)
    .bind(&to_s)
    .bind(&REVENUE_STATUSES[..])
    .bind(&CANCELLED_STATUSES[..])
    .fetch_one(&mut *tx)
    .await?;
    let top_products = sqlx::query(
        "SELECT i.product_name AS name, sum(i.quantity)::bigint AS qty, sum(i.line_total)::numeric(14,2)::text AS revenue
           FROM order_items i JOIN orders o ON o.tenant_id = i.tenant_id AND o.id = i.order_id
          WHERE o.tenant_id = $1 AND app.jkt_day(o.created_at) BETWEEN $2::date AND $3::date AND o.status = ANY($4)
          GROUP BY 1 ORDER BY qty DESC, name LIMIT 5",
    )
    .bind(t.as_uuid())
    .bind(&f)
    .bind(&to_s)
    .bind(&REVENUE_STATUSES[..])
    .fetch_all(&mut *tx)
    .await?
    .iter()
    .map(|r| TopProduct {
        name: r.get("name"),
        quantity: r.get("qty"),
        revenue: r.get("revenue"),
    })
    .collect();
    let alerts = "FROM inventory_levels l
        JOIN product_variants v ON v.tenant_id = l.tenant_id AND v.id = l.variant_id
        JOIN products p ON p.tenant_id = v.tenant_id AND p.id = v.product_id
       WHERE l.tenant_id = $1 AND l.min_stock IS NOT NULL AND (l.on_hand - l.reserved) <= l.min_stock
         AND p.status <> 'archived'";
    let stock_alerts: i64 =
        sqlx::query_scalar(sqlx::AssertSqlSafe(format!("SELECT count(*) {alerts}")))
            .bind(t.as_uuid())
            .fetch_one(&mut *tx)
            .await?;
    let stock_alert_items = sqlx::query(sqlx::AssertSqlSafe(format!(
        "SELECT p.name AS product, v.name AS variant, (l.on_hand - l.reserved)::bigint AS available,
                l.min_stock::bigint AS min_stock {alerts} ORDER BY (l.on_hand - l.reserved) - l.min_stock, p.name LIMIT 10"
    )))
    .bind(t.as_uuid())
    .fetch_all(&mut *tx)
    .await?
    .iter()
    .map(|r| StockAlert {
        product: r.get("product"),
        variant: r.get("variant"),
        available: r.get("available"),
        min_stock: r.get("min_stock"),
    })
    .collect();
    let c = sqlx::query(sqlx::AssertSqlSafe(format!(
        "SELECT count(*) FILTER (WHERE anonymized_at IS NULL) AS total,
                count(*) FILTER (WHERE {in_range}) AS new
           FROM customers WHERE tenant_id = $1"
    )))
    .bind(t.as_uuid())
    .bind(&f)
    .bind(&to_s)
    .fetch_one(&mut *tx)
    .await?;
    let tk = sqlx::query(
        "SELECT count(*) FILTER (WHERE app.jkt_day(created_at) BETWEEN $2::date AND $3::date) AS created,
                count(*) FILTER (WHERE resolved_at IS NOT NULL AND app.jkt_day(resolved_at) BETWEEN $2::date AND $3::date) AS resolved,
                coalesce(sum(app.ms_between(created_at, resolved_at)) FILTER (
                    WHERE resolved_at IS NOT NULL AND app.jkt_day(resolved_at) BETWEEN $2::date AND $3::date), 0)::bigint AS res_ms
           FROM tickets WHERE tenant_id = $1",
    )
    .bind(t.as_uuid())
    .bind(&f)
    .bind(&to_s)
    .fetch_one(&mut *tx)
    .await?;
    let cs = sqlx::query(
        "SELECT coalesce(sum(started), 0)::bigint AS started, coalesce(sum(closed_ai), 0)::bigint AS cai,
                coalesce(sum(closed_human), 0)::bigint AS chu, coalesce(sum(handed_over), 0)::bigint AS handed
           FROM conversation_daily_stats WHERE tenant_id = $1 AND day BETWEEN $2::date AND $3::date",
    )
    .bind(t.as_uuid())
    .bind(&f)
    .bind(&to_s)
    .fetch_one(&mut *tx)
    .await?;
    let ai = sqlx::query(sqlx::AssertSqlSafe(format!(
        "SELECT count(*) FILTER (WHERE purpose = 'chat') AS turns,
                coalesce(sum(prompt_tokens + completion_tokens) FILTER (WHERE source = 'platform'), 0)::bigint AS platform,
                coalesce(sum(prompt_tokens + completion_tokens) FILTER (WHERE source = 'byok'), 0)::bigint AS byok
           FROM ai_runs WHERE tenant_id = $1 AND {in_range}"
    )))
    .bind(t.as_uuid())
    .bind(&f)
    .bind(&to_s)
    .fetch_one(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(TenantOverview {
        from: f,
        to: to_s,
        orders: o.get("orders"),
        paid_orders: o.get("paid"),
        revenue: o.get("revenue"),
        cancelled_orders: o.get("cancelled"),
        top_products,
        stock_alerts,
        stock_alert_items,
        customers_total: c.get("total"),
        customers_new: c.get("new"),
        tickets_created: tk.get("created"),
        tickets_resolved: tk.get("resolved"),
        ticket_resolution_ms_sum: tk.get("res_ms"),
        conversations: cs.get("started"),
        ai_resolved: cs.get("cai"),
        human_resolved: cs.get("chu"),
        handed_over: cs.get("handed"),
        ai_platform_tokens: ai.get("platform"),
        ai_byok_tokens: ai.get("byok"),
        ai_turns: ai.get("turns"),
    })
}

/// Agregat platform (FR-ANL-002) lewat `app.platform_overview` — HANYA dipanggil setelah otorisasi platform di API.
pub async fn platform_overview(
    pool: &PgPool,
    from: Option<&str>,
    to: Option<&str>,
) -> Result<(String, String, serde_json::Value), AnalyticsError> {
    let mut tx = pool.begin().await?;
    let (f, to_s) = resolve_range(&mut *tx, from, to).await?;
    let v: serde_json::Value =
        sqlx::query_scalar("SELECT app.platform_overview($1::date, $2::date)")
            .bind(&f)
            .bind(&to_s)
            .fetch_one(&mut *tx)
            .await?;
    tx.commit().await?;
    Ok((f, to_s, v))
}
