//! Kontrak OpenAPI (P0-9, `PRD.md §7.4`). Sumber kebenaran: kode (utoipa).
//! `docs/api/openapi.yaml` adalah snapshot yang dijaga tes `tests/openapi.rs`:
//! tes gagal bila kode dan snapshot berbeda (drift).
//!
//! Regenerasi: `UPDATE_OPENAPI=1 cargo test -p umkm-api --test openapi`.

use umkm_shared::http::{ErrorBody, ErrorCode, ErrorEnvelope, Meta};
use utoipa::{
    Modify, OpenApi,
    openapi::{
        RefOr, ResponseBuilder,
        content::ContentBuilder,
        path::{Operation, PathItem},
    },
};

use crate::{auth, health, tenants};

#[derive(OpenApi)]
#[openapi(
    info(
        title = "UMKM Business OS API",
        description = "Kontrak HTTP. Endpoint bisnis memakai prefix `/api/v1` (PRD.md §7.4). \
            Semua error memakai ErrorEnvelope; header `x-request-id` == `meta.request_id`.",
        version = "0.1.0"
    ),
    paths(
        health::healthz,
        health::readyz,
        auth::register_handler,
        auth::login_handler,
        auth::session_handler,
        auth::logout_handler,
        tenants::list_tenants,
        tenants::get_tenant,
        tenants::update_tenant
    ),
    components(schemas(ErrorEnvelope, ErrorBody, ErrorCode, Meta)),
    servers((url = "/", description = "Origin yang sama; root domain menunggu D-01")),
    modifiers(&CommonErrors, &PublicOps, &PruneUnused),
    tags(
        (name = "ops", description = "Liveness/readiness untuk orchestrator"),
        (name = "auth", description = "Registrasi, login, sesi (cookie HttpOnly + CSRF, ADR-0015)"),
        (name = "tenants", description = "Tenant milik pemanggil; tenant_id di path diverifikasi terhadap membership")
    )
)]
pub struct ApiDoc;

/// Error yang dapat muncul di SEMUA endpoint karena middleware global
/// (timeout, body limit, panic) ditambahkan ke setiap operasi.
struct CommonErrors;

const COMMON: [(&str, &str); 4] = [
    ("405", "Method tidak diizinkan"),
    ("408", "Request melebihi `server.request_timeout_secs`"),
    ("413", "Body melebihi `server.body_limit_bytes`"),
    ("500", "Kesalahan internal (tanpa detail internal)"),
];

impl Modify for CommonErrors {
    fn modify(&self, openapi: &mut utoipa::openapi::OpenApi) {
        // Lisensi dari Cargo ("UNLICENSED") bukan SPDX valid; proyek privat.
        openapi.info.license = None;
        let error_ref = || {
            ResponseBuilder::new().content(
                "application/json",
                ContentBuilder::new()
                    .schema(Some(RefOr::Ref(utoipa::openapi::Ref::from_schema_name(
                        "ErrorEnvelope",
                    ))))
                    .build(),
            )
        };
        for item in openapi.paths.paths.values_mut() {
            for op in operations(item) {
                for (code, desc) in COMMON {
                    op.responses
                        .responses
                        .entry(code.to_owned())
                        .or_insert_with(|| error_ref().description(desc).build().into());
                }
            }
        }
    }
}

/// Endpoint ops sengaja publik (dipakai orchestrator); dinyatakan eksplisit
/// `security: []`. Skema auth tenant/admin ditambahkan di P1.
struct PublicOps;

const PUBLIC_PATHS: [&str; 4] = [
    "/healthz",
    "/readyz",
    "/api/v1/auth/register",
    "/api/v1/auth/login",
];

/// Skema `sessionCookie` (ADR-0015). Endpoint selain PUBLIC_PATHS mewajibkannya.
/// Production memakai nama `__Host-umkm_session`; development `umkm_session`.
impl Modify for PublicOps {
    fn modify(&self, openapi: &mut utoipa::openapi::OpenApi) {
        use utoipa::openapi::security::{ApiKey, ApiKeyValue, SecurityRequirement, SecurityScheme};
        let components = openapi.components.get_or_insert_with(Default::default);
        components.add_security_scheme(
            "sessionCookie",
            SecurityScheme::ApiKey(ApiKey::Cookie(ApiKeyValue::with_description(
                "__Host-umkm_session",
                "Cookie HttpOnly; request yang mengubah state wajib header X-CSRF-Token",
            ))),
        );
        for (path, item) in openapi.paths.paths.iter_mut() {
            let public = PUBLIC_PATHS.contains(&path.as_str());
            for op in operations(item) {
                op.security = Some(if public {
                    Vec::new()
                } else {
                    vec![SecurityRequirement::new(
                        "sessionCookie",
                        Vec::<String>::new(),
                    )]
                });
            }
        }
    }
}

/// Hapus komponen skema yang tidak dirujuk (utoipa menambahkan argumen generic
/// sebagai komponen walau sudah di-inline). Diulang sampai titik tetap.
struct PruneUnused;

impl Modify for PruneUnused {
    fn modify(&self, openapi: &mut utoipa::openapi::OpenApi) {
        loop {
            let Some(components) = openapi.components.as_ref() else {
                return;
            };
            let names: Vec<String> = components.schemas.keys().cloned().collect();
            let json = serde_json::to_string(&*openapi).expect("serializable");
            let unused: Vec<String> = names
                .into_iter()
                .filter(|n| !json.contains(&format!("\"#/components/schemas/{n}\"")))
                .collect();
            if unused.is_empty() {
                return;
            }
            let components = openapi.components.as_mut().expect("checked above");
            for n in unused {
                components.schemas.remove(&n);
            }
        }
    }
}

fn operations(item: &mut PathItem) -> impl Iterator<Item = &mut Operation> {
    [
        item.get.as_mut(),
        item.put.as_mut(),
        item.post.as_mut(),
        item.delete.as_mut(),
        item.options.as_mut(),
        item.head.as_mut(),
        item.patch.as_mut(),
        item.trace.as_mut(),
    ]
    .into_iter()
    .flatten()
}

/// YAML deterministik untuk snapshot.
pub fn to_yaml() -> String {
    ApiDoc::openapi()
        .to_yaml()
        .expect("OpenAPI serializable to YAML")
}
