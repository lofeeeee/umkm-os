use std::{sync::Arc, time::Instant};

use axum::{
    extract::{MatchedPath, Request, State},
    middleware::Next,
    response::Response,
};
use umkm_infrastructure::metrics::Metrics;

/// Label `route` = template path dari router (kardinalitas terbatas).
/// Request yang tidak cocok dengan route mana pun -> `unmatched`.
pub async fn record(State(metrics): State<Arc<Metrics>>, req: Request, next: Next) -> Response {
    let started = Instant::now();
    let method = req.method().as_str().to_owned();
    let route = req
        .extensions()
        .get::<MatchedPath>()
        .map(|p| p.as_str().to_owned())
        .unwrap_or_else(|| "unmatched".to_owned());
    let res = next.run(req).await;
    metrics.observe_http(
        &method,
        &route,
        res.status().as_u16(),
        started.elapsed().as_secs_f64(),
    );
    res
}
