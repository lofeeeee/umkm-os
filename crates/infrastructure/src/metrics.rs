//! Metrics Prometheus/OpenMetrics minimum `PRD.md §9.8` (P0-8).
//!
//! Satu registry per proses (api, worker). Setiap proses mengisi metric yang ia
//! amati; sisanya tetap terdaftar (nilai nol) agar kontrak nama stabil sejak awal.
//!
//! Catatan nama: OpenMetrics menambahkan sufiks `_total` pada counter, sehingga
//! counter `tenant_api_usage` tampil sebagai `tenant_api_usage_total`.
//!
//! Kardinalitas label dijaga: `route` = template path (mis. `/api/v1/orders/{id}`),
//! bukan URL mentah; request tanpa route -> `unmatched`.

use prometheus_client::{
    encoding::{EncodeLabelSet, text::encode},
    metrics::{
        counter::Counter,
        family::Family,
        gauge::Gauge,
        histogram::{Histogram, exponential_buckets},
    },
    registry::Registry,
};

pub const CONTENT_TYPE: &str = "application/openmetrics-text; version=1.0.0; charset=utf-8";

/// 16 nama metric wajib `PRD.md §9.8`, sebagaimana tampil di output.
pub const REQUIRED: [&str; 16] = [
    "http_requests_total",
    "http_request_duration_seconds",
    "http_request_errors_total",
    "db_connections_active",
    "db_query_duration_seconds",
    "db_pool_wait_seconds",
    "worker_jobs_total",
    "worker_job_failures_total",
    "worker_queue_depth",
    "ai_requests_total",
    "ai_request_duration_seconds",
    "ai_tokens_total",
    "ai_provider_errors_total",
    "active_websocket_connections",
    "active_tenants",
    "tenant_api_usage",
];

#[derive(Clone, Debug, Hash, PartialEq, Eq, EncodeLabelSet)]
pub struct HttpLabels {
    pub method: String,
    pub route: String,
    pub status: u16,
}

#[derive(Clone, Debug, Hash, PartialEq, Eq, EncodeLabelSet)]
pub struct RouteLabels {
    pub method: String,
    pub route: String,
}

#[derive(Clone, Debug, Hash, PartialEq, Eq, EncodeLabelSet)]
pub struct QueryLabels {
    pub query: String,
}

#[derive(Clone, Debug, Hash, PartialEq, Eq, EncodeLabelSet)]
pub struct JobLabels {
    pub job: String,
}

#[derive(Clone, Debug, Hash, PartialEq, Eq, EncodeLabelSet)]
pub struct QueueLabels {
    pub queue: String,
}

#[derive(Clone, Debug, Hash, PartialEq, Eq, EncodeLabelSet)]
pub struct ProviderLabels {
    pub provider: String,
}

#[derive(Clone, Debug, Hash, PartialEq, Eq, EncodeLabelSet)]
pub struct TokenLabels {
    pub provider: String,
    /// `prompt` | `completion`
    pub kind: String,
}

#[derive(Clone, Debug, Hash, PartialEq, Eq, EncodeLabelSet)]
pub struct ProviderErrorLabels {
    pub provider: String,
    /// mis. `timeout`, `rate_limited`, `http_5xx`, `invalid_response`
    pub kind: String,
}

#[derive(Clone, Debug, Hash, PartialEq, Eq, EncodeLabelSet)]
pub struct TenantLabels {
    pub tenant_id: String,
}

fn latency_histogram() -> Histogram {
    // 1 ms .. ~16 s
    Histogram::new(exponential_buckets(0.001, 2.0, 15))
}

pub struct Metrics {
    registry: Registry,
    pub http_requests: Family<HttpLabels, Counter>,
    pub http_request_duration: Family<RouteLabels, Histogram, fn() -> Histogram>,
    pub http_request_errors: Family<HttpLabels, Counter>,
    pub db_connections_active: Gauge,
    pub db_query_duration: Family<QueryLabels, Histogram, fn() -> Histogram>,
    pub db_pool_wait: Histogram,
    pub worker_jobs: Family<JobLabels, Counter>,
    pub worker_job_failures: Family<JobLabels, Counter>,
    pub worker_queue_depth: Family<QueueLabels, Gauge>,
    pub ai_requests: Family<ProviderLabels, Counter>,
    pub ai_request_duration: Family<ProviderLabels, Histogram, fn() -> Histogram>,
    pub ai_tokens: Family<TokenLabels, Counter>,
    pub ai_provider_errors: Family<ProviderErrorLabels, Counter>,
    pub active_websocket_connections: Gauge,
    pub active_tenants: Gauge,
    pub tenant_api_usage: Family<TenantLabels, Counter>,
}

impl Default for Metrics {
    fn default() -> Self {
        Self::new()
    }
}

impl Metrics {
    pub fn new() -> Self {
        let mut registry = Registry::default();
        let hist: fn() -> Histogram = latency_histogram;

        let m = Self {
            http_requests: Family::default(),
            http_request_duration: Family::new_with_constructor(hist),
            http_request_errors: Family::default(),
            db_connections_active: Gauge::default(),
            db_query_duration: Family::new_with_constructor(hist),
            db_pool_wait: latency_histogram(),
            worker_jobs: Family::default(),
            worker_job_failures: Family::default(),
            worker_queue_depth: Family::default(),
            ai_requests: Family::default(),
            ai_request_duration: Family::new_with_constructor(hist),
            ai_tokens: Family::default(),
            ai_provider_errors: Family::default(),
            active_websocket_connections: Gauge::default(),
            active_tenants: Gauge::default(),
            tenant_api_usage: Family::default(),
            registry: Registry::default(),
        };

        // Counter: nama tanpa `_total` (ditambahkan encoder).
        registry.register(
            "http_requests",
            "HTTP requests by method, route template, status",
            m.http_requests.clone(),
        );
        registry.register(
            "http_request_duration_seconds",
            "HTTP request latency",
            m.http_request_duration.clone(),
        );
        registry.register(
            "http_request_errors",
            "HTTP responses with status >= 500",
            m.http_request_errors.clone(),
        );
        registry.register(
            "db_connections_active",
            "PostgreSQL pool connections in use",
            m.db_connections_active.clone(),
        );
        registry.register(
            "db_query_duration_seconds",
            "Database query latency by named query",
            m.db_query_duration.clone(),
        );
        registry.register(
            "db_pool_wait_seconds",
            "Time waiting to acquire a pool connection",
            m.db_pool_wait.clone(),
        );
        registry.register(
            "worker_jobs",
            "Worker jobs processed",
            m.worker_jobs.clone(),
        );
        registry.register(
            "worker_job_failures",
            "Worker jobs failed",
            m.worker_job_failures.clone(),
        );
        registry.register(
            "worker_queue_depth",
            "Pending jobs per queue",
            m.worker_queue_depth.clone(),
        );
        registry.register(
            "ai_requests",
            "LLM provider requests",
            m.ai_requests.clone(),
        );
        registry.register(
            "ai_request_duration_seconds",
            "LLM provider request latency",
            m.ai_request_duration.clone(),
        );
        registry.register("ai_tokens", "LLM tokens consumed", m.ai_tokens.clone());
        registry.register(
            "ai_provider_errors",
            "LLM provider errors by kind",
            m.ai_provider_errors.clone(),
        );
        registry.register(
            "active_websocket_connections",
            "Open WebSocket connections",
            m.active_websocket_connections.clone(),
        );
        registry.register(
            "active_tenants",
            "Tenants with status active",
            m.active_tenants.clone(),
        );
        registry.register(
            "tenant_api_usage",
            "API requests attributed to a verified tenant",
            m.tenant_api_usage.clone(),
        );

        Self { registry, ..m }
    }

    /// Catat satu request HTTP selesai.
    pub fn observe_http(&self, method: &str, route: &str, status: u16, seconds: f64) {
        let labels = HttpLabels {
            method: method.to_owned(),
            route: route.to_owned(),
            status,
        };
        self.http_requests.get_or_create(&labels).inc();
        if status >= 500 {
            self.http_request_errors.get_or_create(&labels).inc();
        }
        self.http_request_duration
            .get_or_create(&RouteLabels {
                method: labels.method,
                route: labels.route,
            })
            .observe(seconds);
    }

    /// Output teks OpenMetrics.
    pub fn encode(&self) -> String {
        let mut out = String::new();
        encode(&mut out, &self.registry).expect("encoding to String cannot fail");
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Baris `# TYPE` memakai nama tanpa `_total` untuk counter.
    fn type_line_present(text: &str, name: &str) -> bool {
        let base = name.strip_suffix("_total").unwrap_or(name);
        text.lines().any(|l| {
            l.starts_with(&format!("# TYPE {base} ")) || l.starts_with(&format!("# TYPE {name} "))
        })
    }

    #[test]
    fn unlabeled_metrics_exposed_immediately() {
        let text = Metrics::new().encode();
        for name in [
            "db_connections_active",
            "db_pool_wait_seconds",
            "active_websocket_connections",
            "active_tenants",
        ] {
            assert!(type_line_present(&text, name), "{name}:\n{text}");
        }
    }

    /// Family berlabel baru tampil setelah observasi pertama (konvensi Prometheus;
    /// tidak dibuat series palsu). Setelah setiap family diobservasi sekali,
    /// ke-16 metric wajib harus ada di output.
    #[test]
    fn all_16_required_metrics_exposed_after_observation() {
        let m = Metrics::new();
        m.observe_http("GET", "/x", 500, 0.01);
        m.db_query_duration
            .get_or_create(&QueryLabels {
                query: "ping".into(),
            })
            .observe(0.001);
        let job = JobLabels {
            job: "email".into(),
        };
        m.worker_jobs.get_or_create(&job).inc();
        m.worker_job_failures.get_or_create(&job).inc();
        m.worker_queue_depth
            .get_or_create(&QueueLabels {
                queue: "outbox".into(),
            })
            .set(3);
        let p = ProviderLabels {
            provider: "openai_compatible".into(),
        };
        m.ai_requests.get_or_create(&p).inc();
        m.ai_request_duration.get_or_create(&p).observe(0.5);
        m.ai_tokens
            .get_or_create(&TokenLabels {
                provider: p.provider.clone(),
                kind: "prompt".into(),
            })
            .inc_by(10);
        m.ai_provider_errors
            .get_or_create(&ProviderErrorLabels {
                provider: p.provider.clone(),
                kind: "timeout".into(),
            })
            .inc();
        m.tenant_api_usage
            .get_or_create(&TenantLabels {
                tenant_id: "t".into(),
            })
            .inc();

        let text = m.encode();
        let missing: Vec<_> = REQUIRED
            .iter()
            .filter(|n| !type_line_present(&text, n))
            .collect();
        assert!(missing.is_empty(), "missing {missing:?}:\n{text}");
        assert!(text.contains("tenant_api_usage_total{tenant_id=\"t\"} 1"));
        assert!(
            text.contains("ai_tokens_total{provider=\"openai_compatible\",kind=\"prompt\"} 10")
        );
    }

    #[test]
    fn http_observation_counts_and_errors() {
        let m = Metrics::new();
        m.observe_http("GET", "/healthz", 200, 0.002);
        m.observe_http("GET", "/healthz", 200, 0.004);
        m.observe_http("GET", "/readyz", 503, 0.010);
        let text = m.encode();
        assert!(
            text.contains(r#"http_requests_total{method="GET",route="/healthz",status="200"} 2"#),
            "{text}"
        );
        assert!(
            text.contains(
                r#"http_request_errors_total{method="GET",route="/readyz",status="503"} 1"#
            ),
            "{text}"
        );
        assert!(!text.contains(r#"http_request_errors_total{method="GET",route="/healthz""#));
        assert!(
            text.contains(
                r#"http_request_duration_seconds_count{method="GET",route="/healthz"} 2"#
            )
        );
    }

    #[test]
    fn output_is_valid_openmetrics_terminated() {
        assert!(Metrics::new().encode().ends_with("# EOF\n"));
    }
}
