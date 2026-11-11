use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
};
use umkm_shared::http::{ErrorBody, ErrorCode};

/// Error dari handler. Response dirender menjadi envelope oleh
/// `middleware::envelope`, yang memegang `request_id`.
#[derive(Debug, Clone)]
pub struct ApiError(pub ErrorBody);

impl ApiError {
    pub fn new(code: ErrorCode) -> Self {
        Self(ErrorBody::new(code))
    }
}

impl From<ErrorBody> for ApiError {
    fn from(body: ErrorBody) -> Self {
        Self(body)
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let status = StatusCode::from_u16(self.0.code.http_status())
            .unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);
        let mut res = status.into_response();
        res.extensions_mut().insert(self.0);
        res
    }
}
