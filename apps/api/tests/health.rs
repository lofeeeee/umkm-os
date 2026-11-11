//! P0-3: healthz/readyz, request ID, error envelope, limits, panic, security headers.

use std::{sync::Arc, time::Duration};

use axum::{
    Router,
    body::Body,
    http::{Method, Request, StatusCode},
    routing::{get, post},
};
use http_body_util::BodyExt;
use serde_json::Value;
use tower::ServiceExt;
use umkm_api::{
    AppState, HttpLimits, ReadinessCheck, build_router, state::CheckFuture, with_middleware,
};

const LIMITS: HttpLimits = HttpLimits {
    request_timeout: Duration::from_millis(200),
    body_limit_bytes: 64,
};

struct Check(&'static str, bool);

impl ReadinessCheck for Check {
    fn name(&self) -> &'static str {
        self.0
    }
    fn check(&self) -> CheckFuture<'_> {
        let ok = self.1;
        Box::pin(async move {
            if ok {
                Ok(())
            } else {
                Err("connection refused 10.0.0.5:5432".into())
            }
        })
    }
}

struct Hang;

impl ReadinessCheck for Hang {
    fn name(&self) -> &'static str {
        "hang"
    }
    fn check(&self) -> CheckFuture<'_> {
        Box::pin(std::future::pending())
    }
}

async fn call(app: Router, req: Request<Body>) -> (StatusCode, axum::http::HeaderMap, Value) {
    let res = app.oneshot(req).await.unwrap();
    let status = res.status();
    let headers = res.headers().clone();
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    let json = serde_json::from_slice(&bytes)
        .unwrap_or_else(|_| panic!("body bukan JSON: {:?}", String::from_utf8_lossy(&bytes)));
    (status, headers, json)
}

fn get_req(uri: &str) -> Request<Body> {
    Request::get(uri).body(Body::empty()).unwrap()
}

async fn panics() -> &'static str {
    panic!("secret-internal-detail")
}

fn test_app() -> Router {
    let routes = Router::new()
        .route("/panic", get(panics))
        .route(
            "/slow",
            get(|| async {
                tokio::time::sleep(Duration::from_secs(5)).await;
                "late"
            }),
        )
        .route("/echo", post(|body: String| async move { body }));
    with_middleware(routes, LIMITS, Arc::default()).with_state(AppState::default())
}

fn assert_error(json: &Value, headers: &axum::http::HeaderMap, code: &str) {
    assert_eq!(json["success"], false, "{json}");
    assert_eq!(json["error"]["code"], code, "{json}");
    assert!(json["error"]["message"].is_string());
    assert_eq!(
        json["meta"]["request_id"],
        headers["x-request-id"].to_str().unwrap(),
        "request_id body == header"
    );
}

#[tokio::test]
async fn healthz_returns_success_envelope_with_request_id() {
    let (status, headers, json) = call(
        build_router(AppState::default(), LIMITS),
        get_req("/healthz"),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["success"], true);
    assert_eq!(json["data"]["status"], "ok");
    let id = headers["x-request-id"].to_str().unwrap();
    assert!(id.starts_with("req-"));
    assert_eq!(json["meta"]["request_id"], id);
}

#[tokio::test]
async fn client_supplied_request_id_is_ignored() {
    let req = Request::get("/healthz")
        .header("x-request-id", "forged-id")
        .body(Body::empty())
        .unwrap();
    let (_, headers, json) = call(build_router(AppState::default(), LIMITS), req).await;
    assert_ne!(headers["x-request-id"], "forged-id");
    assert_ne!(json["meta"]["request_id"], "forged-id");
}

#[tokio::test]
async fn readyz_ok_when_all_checks_pass() {
    let state = AppState::new(vec![Arc::new(Check("db", true))]);
    let (status, _, json) = call(build_router(state, LIMITS), get_req("/readyz")).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["data"]["status"], "ready");
    assert_eq!(
        json["data"]["checks"][0],
        serde_json::json!({"name": "db", "ok": true})
    );
}

#[tokio::test]
async fn readyz_503_without_leaking_internal_error() {
    let state = AppState::new(vec![
        Arc::new(Check("db", false)),
        Arc::new(Check("redis", true)),
        Arc::new(Hang),
    ]);
    let limits = HttpLimits {
        request_timeout: Duration::from_secs(10),
        ..LIMITS
    };
    let started = std::time::Instant::now();
    let (status, headers, json) = call(build_router(state, limits), get_req("/readyz")).await;
    assert!(
        started.elapsed() < Duration::from_secs(3),
        "check harus paralel + ber-timeout"
    );
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
    assert_error(&json, &headers, "SERVICE_UNAVAILABLE");
    assert_eq!(
        json["error"]["details"]["failing_checks"],
        serde_json::json!(["db", "hang"])
    );
    assert!(
        !json.to_string().contains("10.0.0.5"),
        "topologi bocor: {json}"
    );
}

#[tokio::test]
async fn unknown_route_is_404_envelope() {
    let (status, headers, json) =
        call(build_router(AppState::default(), LIMITS), get_req("/nope")).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_error(&json, &headers, "RESOURCE_NOT_FOUND");
}

#[tokio::test]
async fn wrong_method_is_405_envelope() {
    let req = Request::builder()
        .method(Method::DELETE)
        .uri("/healthz")
        .body(Body::empty())
        .unwrap();
    let (status, headers, json) = call(build_router(AppState::default(), LIMITS), req).await;
    assert_eq!(status, StatusCode::METHOD_NOT_ALLOWED);
    assert_error(&json, &headers, "METHOD_NOT_ALLOWED");
}

#[tokio::test]
async fn panic_is_500_envelope_without_panic_message() {
    let (status, headers, json) = call(test_app(), get_req("/panic")).await;
    assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);
    assert_error(&json, &headers, "INTERNAL_ERROR");
    assert!(!json.to_string().contains("secret-internal-detail"));
}

#[tokio::test]
async fn slow_handler_times_out_with_408_envelope() {
    let (status, headers, json) = call(test_app(), get_req("/slow")).await;
    assert_eq!(status, StatusCode::REQUEST_TIMEOUT);
    assert_error(&json, &headers, "REQUEST_TIMEOUT");
}

#[tokio::test]
async fn oversized_body_is_413_envelope() {
    let req = Request::post("/echo")
        .body(Body::from(vec![b'a'; 65]))
        .unwrap();
    let (status, headers, json) = call(test_app(), req).await;
    assert_eq!(status, StatusCode::PAYLOAD_TOO_LARGE);
    assert_error(&json, &headers, "PAYLOAD_TOO_LARGE");

    let ok = test_app()
        .oneshot(
            Request::post("/echo")
                .body(Body::from(vec![b'a'; 64]))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(ok.status(), StatusCode::OK, "tepat di batas harus diterima");
}

#[tokio::test]
async fn security_headers_present_on_success_and_error() {
    for uri in ["/healthz", "/nope"] {
        let res = build_router(AppState::default(), LIMITS)
            .oneshot(get_req(uri))
            .await
            .unwrap();
        let h = res.headers();
        assert_eq!(h["x-content-type-options"], "nosniff", "{uri}");
        assert_eq!(h["x-frame-options"], "DENY", "{uri}");
        assert_eq!(h["referrer-policy"], "no-referrer", "{uri}");
        assert_eq!(h["content-type"], "application/json", "{uri}");
    }
}
