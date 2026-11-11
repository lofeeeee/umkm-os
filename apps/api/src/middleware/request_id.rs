use axum::{
    extract::{FromRequestParts, Request},
    http::{HeaderName, HeaderValue, request::Parts},
    middleware::Next,
    response::Response,
};
use umkm_shared::http::RequestId;

pub const REQUEST_ID_HEADER: HeaderName = HeaderName::from_static("x-request-id");

/// Selalu buat ID baru di server; header `x-request-id` dari klien dibuang agar
/// log tidak bisa dipalsukan. ID dikirim balik di header response.
pub async fn assign(mut req: Request, next: Next) -> Response {
    let id = RequestId::generate();
    let header = HeaderValue::from_str(id.as_str()).expect("request id is ascii");
    req.headers_mut().insert(REQUEST_ID_HEADER, header.clone());
    req.extensions_mut().insert(id);
    let mut res = next.run(req).await;
    res.headers_mut().insert(REQUEST_ID_HEADER, header);
    res
}

/// Extractor untuk handler.
pub struct ReqId(pub RequestId);

impl<S: Send + Sync> FromRequestParts<S> for ReqId {
    type Rejection = std::convert::Infallible;

    async fn from_request_parts(parts: &mut Parts, _: &S) -> Result<Self, Self::Rejection> {
        // Middleware `assign` selalu terpasang; fallback hanya untuk router uji tanpa middleware.
        Ok(Self(
            parts
                .extensions
                .get::<RequestId>()
                .cloned()
                .unwrap_or_else(RequestId::generate),
        ))
    }
}
