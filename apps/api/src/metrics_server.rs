//! Endpoint `/metrics` di port internal terpisah (P0-8). Tidak diproxy Caddy dan
//! tidak dipublish di Compose, sehingga tidak dapat diakses dari internet.

use std::sync::Arc;

use axum::{Router, http::header, response::IntoResponse, routing::get};
use umkm_infrastructure::metrics::{CONTENT_TYPE, Metrics};

/// Dipanggil tepat sebelum encode, untuk gauge yang di-sample (mis. pool DB).
pub type OnScrape = Arc<dyn Fn() + Send + Sync>;

pub fn metrics_router(metrics: Arc<Metrics>, on_scrape: OnScrape) -> Router {
    Router::new().route(
        "/metrics",
        get(move || {
            let metrics = metrics.clone();
            let on_scrape = on_scrape.clone();
            async move {
                on_scrape();
                ([(header::CONTENT_TYPE, CONTENT_TYPE)], metrics.encode()).into_response()
            }
        }),
    )
}
