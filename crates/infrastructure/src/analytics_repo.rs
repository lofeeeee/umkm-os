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
