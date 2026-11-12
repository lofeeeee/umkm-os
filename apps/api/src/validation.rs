//! Error validasi multi-field: `422 VALIDATION_FAILED`, `details.errors = [{path, code}]`.

use umkm_shared::http::{ErrorBody, ErrorCode};

use crate::ApiError;

pub fn field_errors<'a>(errs: impl IntoIterator<Item = (&'a str, &'a str)>) -> ApiError {
    let list: Vec<_> = errs
        .into_iter()
        .map(|(path, code)| serde_json::json!({ "path": path, "code": code }))
        .collect();
    ApiError(
        ErrorBody::new(ErrorCode::ValidationFailed)
            .with_message("Validasi gagal")
            .with_details(serde_json::json!({ "errors": list })),
    )
}

pub fn one(path: &str, code: &str) -> ApiError {
    field_errors([(path, code)])
}
