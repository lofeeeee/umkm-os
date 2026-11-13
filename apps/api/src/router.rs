use std::{sync::Arc, time::Duration};

use axum::{
    Router,
    extract::Request,
    http::{HeaderName, HeaderValue, Method, StatusCode, header},
    middleware::{from_fn, from_fn_with_state},
    response::IntoResponse,
    routing::get,
};
use tower::ServiceBuilder;
use tower_http::{
    LatencyUnit,
    catch_panic::CatchPanicLayer,
    cors::{AllowOrigin, CorsLayer},
    limit::RequestBodyLimitLayer,
    set_header::SetResponseHeaderLayer,
    timeout::TimeoutLayer,
    trace::{DefaultOnResponse, TraceLayer},
};
use tracing::Level;
use umkm_infrastructure::metrics::Metrics;
use umkm_shared::{config::ServerSettings, http::RequestId};

use crate::{
    AppState, ai, approvals, audit, auth, business, catalog, commerce, crm, customer_auth, health,
    inventory, middleware, notifications, team, tenants, tickets, upload, website,
};

#[derive(Debug, Clone, Copy)]
pub struct HttpLimits {
    pub request_timeout: Duration,
    pub body_limit_bytes: usize,
}

impl From<&ServerSettings> for HttpLimits {
    fn from(s: &ServerSettings) -> Self {
        Self {
            request_timeout: Duration::from_secs(s.request_timeout_secs),
            body_limit_bytes: s.body_limit_bytes,
        }
    }
}

/// Router lengkap aplikasi. Endpoint `/api/v1/*` ditambahkan per task berikutnya.
pub fn build_router(state: AppState, limits: HttpLimits) -> Router {
    let routes = Router::new()
        .route("/healthz", get(health::healthz))
        .route("/readyz", get(health::readyz))
        .merge(auth::routes())
        .merge(crate::mfa::routes())
        .merge(crate::chat::routes())
        .merge(tenants::routes())
        .merge(upload::routes())
        .merge(business::routes())
        .merge(website::routes())
        .merge(catalog::routes())
        .merge(inventory::routes())
        .merge(crm::routes())
        .merge(customer_auth::routes())
        .merge(commerce::routes())
        .merge(team::routes())
        .merge(approvals::routes())
        .merge(tickets::routes())
        .merge(notifications::routes())
        .merge(audit::routes())
        .merge(ai::routes())
        .merge(ai::routes_runtime());
    let metrics = state.metrics.clone();
    let cors = cors_layer(
        state
            .auth
            .as_ref()
            .map(|a| a.allowed_origins.as_slice())
            .unwrap_or_default(),
    );
    with_middleware(routes, limits, metrics)
        .with_state(state)
        .layer(cors)
}

/// CORS hanya untuk origin allowlist (ADR-0015); tanpa wildcard, dengan credentials.
/// Daftar kosong = tidak ada header CORS (request lintas-origin diblokir browser).
fn cors_layer(origins: &[String]) -> CorsLayer {
    let list: Vec<HeaderValue> = origins
        .iter()
        .filter_map(|o| HeaderValue::from_str(o).ok())
        .collect();
    CorsLayer::new()
        .allow_origin(AllowOrigin::list(list))
        .allow_credentials(true)
        .allow_methods([
            Method::GET,
            Method::POST,
            Method::PUT,
            Method::PATCH,
            Method::DELETE,
        ])
        .allow_headers([
            header::CONTENT_TYPE,
            HeaderName::from_static(auth::CSRF_HEADER),
        ])
        .max_age(Duration::from_secs(600))
}

/// Pasang middleware standar. Dipisah agar tes dapat memakai stack yang sama
/// dengan route uji (panic, slow handler, body besar).
pub fn with_middleware(
    routes: Router<AppState>,
    limits: HttpLimits,
    metrics: Arc<Metrics>,
) -> Router<AppState> {
    let security_headers = [
        ("x-content-type-options", "nosniff"),
        ("x-frame-options", "DENY"),
        ("referrer-policy", "no-referrer"),
    ];
    let mut router = routes
        // Urutan layer: terakhir dipasang = terluar.
        .layer(
            ServiceBuilder::new()
                .layer(CatchPanicLayer::custom(|_| {
                    // Isi panic tidak pernah dikirim ke klien.
                    StatusCode::INTERNAL_SERVER_ERROR.into_response()
                }))
                .layer(TimeoutLayer::with_status_code(
                    StatusCode::REQUEST_TIMEOUT,
                    limits.request_timeout,
                ))
                .layer(RequestBodyLimitLayer::new(limits.body_limit_bytes)),
        )
        .layer(from_fn(middleware::envelope::render_errors))
        // Di luar envelope: mencatat status final, termasuk 408/413/500.
        .layer(from_fn_with_state(metrics, middleware::metrics::record));
    for (name, value) in security_headers {
        router = router.layer(SetResponseHeaderLayer::overriding(
            HeaderName::from_static(name),
            HeaderValue::from_static(value),
        ));
    }
    // Default no-store; handler boleh menetapkan sendiri (mis. file bertanda tangan, P2-6).
    router = router.layer(SetResponseHeaderLayer::if_not_present(
        header::CACHE_CONTROL,
        HeaderValue::from_static("no-store"),
    ));
    router
        .layer(TraceLayer::new_for_http().make_span_with(|req: &Request| {
            let request_id = req
                .extensions()
                .get::<RequestId>()
                .map(|id| id.to_string())
                .unwrap_or_default();
            // Hanya path; query string bisa memuat token dan tidak di-log.
            tracing::info_span!("http", method = %req.method(), path = req.uri().path(), request_id)
        })
        .on_response(DefaultOnResponse::new().level(Level::INFO).latency_unit(LatencyUnit::Millis)))
        .layer(from_fn(middleware::request_id::assign))
}
