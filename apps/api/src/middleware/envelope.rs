use axum::{
    Json,
    extract::Request,
    http::{HeaderValue, header},
    middleware::Next,
    response::{IntoResponse, Response},
};
use umkm_shared::http::{ErrorBody, ErrorCode, ErrorEnvelope, RequestId};

/// Setiap response >= 400 dirender ulang menjadi error envelope `PRD.md §7.4`,
/// termasuk yang lahir dari framework (404/405/415), timeout, body limit, dan panic.
/// Body asli dibuang sehingga teks internal tidak pernah bocor (`PRD.md §6.4`).
pub async fn render_errors(req: Request, next: Next) -> Response {
    let request_id = req
        .extensions()
        .get::<RequestId>()
        .cloned()
        .unwrap_or_else(RequestId::generate);
    let res = next.run(req).await;
    let status = res.status();
    if !(status.is_client_error() || status.is_server_error()) {
        return res;
    }
    let body = res
        .extensions()
        .get::<ErrorBody>()
        .cloned()
        .unwrap_or_else(|| ErrorBody::new(ErrorCode::from_http_status(status.as_u16())));
    if status.is_server_error() {
        tracing::error!(status = status.as_u16(), code = ?body.code, "request failed");
    }
    let (mut parts, _) = res.into_parts();
    parts.headers.remove(header::CONTENT_LENGTH);
    parts.headers.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("application/json"),
    );
    let rendered = Json(ErrorEnvelope::new(body, &request_id)).into_response();
    Response::from_parts(parts, rendered.into_body())
}
