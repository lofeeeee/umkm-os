//! P0-9: kontrak OpenAPI.
//! 1. Snapshot `docs/api/openapi.yaml` == spec dari kode (drift -> gagal).
//!    Regenerasi: `UPDATE_OPENAPI=1 cargo test -p umkm-api --test openapi`.
//! 2. Setiap path di spec benar-benar di-route.
//! 3. Response nyata cocok dengan skema (field wajib + enum ErrorCode).

use std::{path::PathBuf, time::Duration};

use axum::{body::Body, http::Request};
use http_body_util::BodyExt;
use serde_json::Value;
use tower::ServiceExt;
use umkm_api::{AppState, HttpLimits, build_router, openapi};
use utoipa::OpenApi;

const LIMITS: HttpLimits = HttpLimits {
    request_timeout: Duration::from_secs(5),
    body_limit_bytes: 1024,
};

fn snapshot_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../docs/api/openapi.yaml")
}

fn spec() -> Value {
    serde_json::to_value(openapi::ApiDoc::openapi()).unwrap()
}

async fn get_json(uri: &str) -> (u16, Value) {
    let res = build_router(AppState::default(), LIMITS)
        .oneshot(Request::get(uri).body(Body::empty()).unwrap())
        .await
        .unwrap();
    let status = res.status().as_u16();
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    (status, serde_json::from_slice(&bytes).unwrap())
}

fn required(spec: &Value, schema: &str) -> Vec<String> {
    let mut r: Vec<String> = spec["components"]["schemas"][schema]["required"]
        .as_array()
        .unwrap_or_else(|| panic!("schema {schema} tanpa required"))
        .iter()
        .map(|v| v.as_str().unwrap().to_owned())
        .collect();
    r.sort();
    r
}

fn keys(v: &Value) -> Vec<String> {
    let mut k: Vec<String> = v.as_object().unwrap().keys().cloned().collect();
    k.sort();
    k
}

#[test]
fn snapshot_matches_code() {
    let generated = openapi::to_yaml();
    let path = snapshot_path();
    if std::env::var_os("UPDATE_OPENAPI").is_some() {
        std::fs::write(&path, &generated).unwrap();
        return;
    }
    let committed = std::fs::read_to_string(&path).unwrap_or_default();
    assert!(
        committed == generated,
        "docs/api/openapi.yaml drift dari kode. Jalankan: UPDATE_OPENAPI=1 cargo test -p umkm-api --test openapi"
    );
}

#[tokio::test]
async fn every_documented_path_is_routed() {
    let spec = spec();
    let paths = spec["paths"].as_object().unwrap();
    assert!(!paths.is_empty());
    for (path, item) in paths {
        for method in item.as_object().unwrap().keys() {
            let m = axum::http::Method::from_bytes(method.to_uppercase().as_bytes()).unwrap();
            let res = build_router(AppState::default(), LIMITS)
                .oneshot(
                    Request::builder()
                        .method(m)
                        .uri(path.as_str())
                        .body(Body::empty())
                        .unwrap(),
                )
                .await
                .unwrap();
            let status = res.status().as_u16();
            assert!(
                status != 404 && status != 405,
                "{method} {path} terdokumentasi tetapi status {status}"
            );
        }
    }
}

#[tokio::test]
async fn real_responses_match_schema() {
    let spec = spec();
    let (status, ok) = get_json("/healthz").await;
    assert_eq!(status, 200);
    assert_eq!(keys(&ok), required(&spec, "SuccessEnvelope_Status"));
    assert_eq!(keys(&ok["meta"]), required(&spec, "Meta"));

    let (status, err) = get_json("/does-not-exist").await;
    assert_eq!(status, 404);
    assert_eq!(keys(&err), required(&spec, "ErrorEnvelope"));
    assert_eq!(keys(&err["error"]), required(&spec, "ErrorBody"));
    let codes = spec["components"]["schemas"]["ErrorCode"]["enum"]
        .as_array()
        .unwrap();
    assert!(
        codes.contains(&err["error"]["code"]),
        "{}",
        err["error"]["code"]
    );
}

/// Aturan Redocly (CI) yang juga dikunci di sini: operationId unik, setiap operasi punya summary,
/// dan parameter `in: path` benar-benar ada di path (struct query wajib `parameter_in = Query`).
#[test]
fn operations_have_unique_ids_summaries_and_real_path_params() {
    let spec = spec();
    let mut ids = std::collections::BTreeSet::new();
    let mut problems = Vec::new();
    for (path, item) in spec["paths"].as_object().unwrap() {
        for (method, op) in item.as_object().unwrap() {
            let at = format!("{method} {path}");
            match op["operationId"].as_str() {
                Some(id) if !ids.insert(id.to_owned()) => {
                    problems.push(format!("{at}: operationId ganda `{id}`"))
                }
                None => problems.push(format!("{at}: tanpa operationId")),
                _ => {}
            }
            if op["summary"].as_str().is_none_or(str::is_empty) {
                problems.push(format!("{at}: tanpa summary (tambahkan doc comment ///)"));
            }
            for p in op["parameters"].as_array().into_iter().flatten() {
                let name = p["name"].as_str().unwrap_or_default();
                if p["in"] == "path" && !path.contains(&format!("{{{name}}}")) {
                    problems.push(format!(
                        "{at}: `{name}` ditandai path padahal tidak ada di path"
                    ));
                }
            }
        }
    }
    assert!(problems.is_empty(), "{}", problems.join("\n"));
}
