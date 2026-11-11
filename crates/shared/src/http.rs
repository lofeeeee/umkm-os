//! Kontrak envelope API `PRD.md §7.4`, framework-agnostic.
//!
//! Sengaja tanpa axum: `domain -> shared`, dan domain tidak boleh bergantung
//! pada framework HTTP (ADR-0013). Integrasi Axum ada di `apps/api`.

use std::fmt;

use serde::Serialize;

/// ID request yang dibuat server (`req-<uuid v4>`). Tidak pernah diambil dari input klien.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RequestId(String);

impl RequestId {
    pub fn generate() -> Self {
        Self(format!("req-{}", uuid::Uuid::new_v4()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for RequestId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct Meta {
    pub request_id: String,
}

impl Meta {
    pub fn new(request_id: &RequestId) -> Self {
        Self {
            request_id: request_id.as_str().to_owned(),
        }
    }
}

#[derive(Debug, Serialize)]
pub struct SuccessEnvelope<T> {
    pub success: bool,
    pub data: T,
    pub meta: Meta,
}

impl<T> SuccessEnvelope<T> {
    pub fn new(data: T, request_id: &RequestId) -> Self {
        Self {
            success: true,
            data,
            meta: Meta::new(request_id),
        }
    }
}

#[derive(Debug, Serialize)]
pub struct ErrorEnvelope {
    pub success: bool,
    pub error: ErrorBody,
    pub meta: Meta,
}

impl ErrorEnvelope {
    pub fn new(error: ErrorBody, request_id: &RequestId) -> Self {
        Self {
            success: false,
            error,
            meta: Meta::new(request_id),
        }
    }
}

/// Isi `error`. `message` ditujukan ke klien: jangan isi dengan SQL, stack trace,
/// secret, atau topologi internal (`PRD.md §6.4`).
#[derive(Debug, Clone, Serialize)]
pub struct ErrorBody {
    pub code: ErrorCode,
    pub message: String,
    pub details: Option<serde_json::Value>,
}

impl ErrorBody {
    pub fn new(code: ErrorCode) -> Self {
        Self {
            code,
            message: code.default_message().to_owned(),
            details: None,
        }
    }

    pub fn with_message(mut self, message: impl Into<String>) -> Self {
        self.message = message.into();
        self
    }

    pub fn with_details(mut self, details: serde_json::Value) -> Self {
        self.details = Some(details);
        self
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ErrorCode {
    BadRequest,
    Unauthorized,
    Forbidden,
    ResourceNotFound,
    MethodNotAllowed,
    RequestTimeout,
    Conflict,
    PayloadTooLarge,
    UnsupportedMediaType,
    ValidationFailed,
    RateLimited,
    InternalError,
    ServiceUnavailable,
}

impl ErrorCode {
    pub fn http_status(self) -> u16 {
        match self {
            Self::BadRequest => 400,
            Self::Unauthorized => 401,
            Self::Forbidden => 403,
            Self::ResourceNotFound => 404,
            Self::MethodNotAllowed => 405,
            Self::RequestTimeout => 408,
            Self::Conflict => 409,
            Self::PayloadTooLarge => 413,
            Self::UnsupportedMediaType => 415,
            Self::ValidationFailed => 422,
            Self::RateLimited => 429,
            Self::InternalError => 500,
            Self::ServiceUnavailable => 503,
        }
    }

    pub fn default_message(self) -> &'static str {
        match self {
            Self::BadRequest => "Bad request",
            Self::Unauthorized => "Authentication required",
            Self::Forbidden => "Permission denied",
            Self::ResourceNotFound => "Resource not found",
            Self::MethodNotAllowed => "Method not allowed",
            Self::RequestTimeout => "Request timed out",
            Self::Conflict => "Conflict",
            Self::PayloadTooLarge => "Payload too large",
            Self::UnsupportedMediaType => "Unsupported media type",
            Self::ValidationFailed => "Validation failed",
            Self::RateLimited => "Too many requests",
            Self::InternalError => "Internal server error",
            Self::ServiceUnavailable => "Service unavailable",
        }
    }

    /// Pemetaan status HTTP -> kode, untuk error yang lahir di middleware/framework.
    pub fn from_http_status(status: u16) -> Self {
        match status {
            401 => Self::Unauthorized,
            403 => Self::Forbidden,
            404 => Self::ResourceNotFound,
            405 => Self::MethodNotAllowed,
            408 => Self::RequestTimeout,
            409 => Self::Conflict,
            413 => Self::PayloadTooLarge,
            415 => Self::UnsupportedMediaType,
            422 => Self::ValidationFailed,
            429 => Self::RateLimited,
            503 => Self::ServiceUnavailable,
            400..=499 => Self::BadRequest,
            _ => Self::InternalError,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn request_id_format() {
        let a = RequestId::generate();
        let b = RequestId::generate();
        assert!(
            a.as_str().starts_with("req-") && a.as_str().len() == 40,
            "{a}"
        );
        assert_ne!(a, b);
    }

    #[test]
    fn error_envelope_matches_prd_shape() {
        let id = RequestId::generate();
        let v = serde_json::to_value(ErrorEnvelope::new(
            ErrorBody::new(ErrorCode::ResourceNotFound),
            &id,
        ))
        .unwrap();
        assert_eq!(
            v,
            serde_json::json!({
                "success": false,
                "error": {"code": "RESOURCE_NOT_FOUND", "message": "Resource not found", "details": null},
                "meta": {"request_id": id.as_str()}
            })
        );
    }

    #[test]
    fn success_envelope_matches_prd_shape() {
        let id = RequestId::generate();
        let v =
            serde_json::to_value(SuccessEnvelope::new(serde_json::json!({"id": 1}), &id)).unwrap();
        assert_eq!(
            v,
            serde_json::json!({"success": true, "data": {"id": 1}, "meta": {"request_id": id.as_str()}})
        );
    }

    #[test]
    fn status_mapping_roundtrips_for_every_code() {
        use ErrorCode::*;
        for c in [
            BadRequest,
            Unauthorized,
            Forbidden,
            ResourceNotFound,
            MethodNotAllowed,
            RequestTimeout,
            Conflict,
            PayloadTooLarge,
            UnsupportedMediaType,
            ValidationFailed,
            RateLimited,
            InternalError,
            ServiceUnavailable,
        ] {
            assert_eq!(ErrorCode::from_http_status(c.http_status()), c);
        }
        assert_eq!(ErrorCode::from_http_status(418), BadRequest);
        assert_eq!(ErrorCode::from_http_status(502), InternalError);
    }
}
