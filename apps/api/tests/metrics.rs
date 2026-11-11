//! P0-8: HTTP metrics tercatat dengan route template, error 5xx dihitung,
//! dan /metrics hanya ada di router internal.

use std::{
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    time::Duration,
};

use axum::{Router, body::Body, http::Request, routing::get};
use http_body_util::BodyExt;
use tower::ServiceExt;
use umkm_api::{
    AppState, HttpLimits, build_router, metrics_server::metrics_router, with_middleware,
};
use umkm_infrastructure::metrics::{CONTENT_TYPE, Metrics};

const LIMITS: HttpLimits = HttpLimits {
    request_timeout: Duration::from_secs(5),
    body_limit_bytes: 1024,
};

async fn get_status(app: &Router, uri: &str) -> u16 {
    app.clone()
        .oneshot(Request::get(uri).body(Body::empty()).unwrap())
        .await
        .unwrap()
        .status()
        .as_u16()
}

async fn scrape(metrics: Arc<Metrics>, on_scrape: Arc<dyn Fn() + Send + Sync>) -> (String, String) {
    let res = metrics_router(metrics, on_scrape)
        .oneshot(Request::get("/metrics").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(res.status(), 200);
    let ct = res.headers()["content-type"].to_str().unwrap().to_owned();
    let body = res.into_body().collect().await.unwrap().to_bytes();
    (ct, String::from_utf8(body.to_vec()).unwrap())
}

#[tokio::test]
async fn http_requests_recorded_by_route_template() {
    let metrics = Arc::new(Metrics::new());
    let app = build_router(AppState::default().with_metrics(metrics.clone()), LIMITS);
    assert_eq!(get_status(&app, "/healthz").await, 200);
    assert_eq!(get_status(&app, "/healthz").await, 200);
    assert_eq!(get_status(&app, "/no/such/path/123").await, 404);

    let (ct, text) = scrape(metrics, Arc::new(|| {})).await;
    assert_eq!(ct, CONTENT_TYPE);
    assert!(
        text.contains(r#"http_requests_total{method="GET",route="/healthz",status="200"} 2"#),
        "{text}"
    );
    // URL mentah tidak menjadi label (kardinalitas).
    assert!(
        text.contains(r#"http_requests_total{method="GET",route="unmatched",status="404"} 1"#),
        "{text}"
    );
    assert!(!text.contains("/no/such/path/123"));
    assert!(
        text.contains(r#"http_request_duration_seconds_count{method="GET",route="/healthz"} 2"#)
    );
}

async fn boom() -> &'static str {
    panic!("boom")
}

#[tokio::test]
async fn server_errors_counted_including_panics() {
    let metrics = Arc::new(Metrics::new());
    let app = with_middleware(
        Router::new().route("/boom", get(boom)),
        LIMITS,
        metrics.clone(),
    )
    .with_state(AppState::default());
    assert_eq!(get_status(&app, "/boom").await, 500);
    let (_, text) = scrape(metrics, Arc::new(|| {})).await;
    assert!(
        text.contains(r#"http_request_errors_total{method="GET",route="/boom",status="500"} 1"#),
        "{text}"
    );
}

#[tokio::test]
async fn on_scrape_hook_runs_before_encode() {
    let metrics = Arc::new(Metrics::new());
    let calls = Arc::new(AtomicUsize::new(0));
    let hook = {
        let (m, c) = (metrics.clone(), calls.clone());
        Arc::new(move || {
            c.fetch_add(1, Ordering::SeqCst);
            m.db_connections_active.set(7);
        })
    };
    let (_, text) = scrape(metrics, hook).await;
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    assert!(text.contains("db_connections_active 7"), "{text}");
}

#[tokio::test]
async fn metrics_not_exposed_on_public_router() {
    let app = build_router(AppState::default(), LIMITS);
    assert_eq!(get_status(&app, "/metrics").await, 404);
}
