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

use crate::{
    ai, approvals, audit, auth, business, catalog, commerce, crm, customer_auth, gopay, health,
    inventory, notifications, team, tenants, tickets, upload, website,
};

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
        crate::account::list_sessions,
        crate::account::revoke_session,
        crate::account::revoke_others,
        crate::account::change_password,
        auth::verify_email_handler,
        crate::analytics::conversations,
        crate::billing::list_plans,
        crate::billing::subscription,
        crate::billing::list_invoices,
        crate::billing::create_invoice,
        crate::billing::upload_proof,
        crate::billing::get_proof,
        crate::billing::cancel_invoice,
        crate::platform::me,
        crate::platform::analytics,
        crate::platform::tenants,
        crate::platform::tenant_detail,
        crate::analytics::overview,
        crate::platform::invoices,
        crate::platform::proof,
        crate::platform::approve,
        crate::platform::reject,
        crate::inbox::list,
        crate::inbox::detail,
        crate::inbox::takeover,
        crate::inbox::release,
        crate::inbox::reply,
        crate::inbox::close,
        crate::inbox::send_draft,
        crate::inbox::discard_draft,
        crate::inbox::assign,
        crate::inbox::create_ticket,
        crate::chat::config,
        crate::chat::start,
        crate::chat::current,
        crate::chat::send,
        crate::chat::handover,
        crate::chat::end,
        crate::chat::chat_stream,
        crate::mfa::login_mfa,
        crate::mfa::mfa_status,
        crate::mfa::mfa_setup,
        crate::mfa::mfa_enable,
        crate::mfa::mfa_disable,
        crate::mfa::mfa_recovery_codes,
        crate::mfa::get_security,
        crate::mfa::put_security,
        tenants::list_tenants,
        tenants::get_tenant,
        tenants::update_tenant,
        crate::realtime::tenant_events,
        upload::upload,
        upload::list_uploads,
        upload::delete_upload,
        upload::serve_file,
        business::get_profile,
        business::put_profile,
        business::put_addresses,
        business::put_hours,
        business::put_exceptions,
        business::get_status,
        gopay::get_status,
        gopay::check_session,
        gopay::request_otp,
        gopay::verify_otp,
        gopay::connect_session,
        gopay::disconnect,
        website::list_templates,
        website::preview_template,
        website::get_website,
        website::select_template,
        website::get_draft,
        website::save_draft,
        website::publish,
        website::save_theme,
        website::list_versions,
        website::get_version,
        website::rollback,
        website::public_site,
        catalog::list_categories,
        catalog::create_category,
        catalog::update_category,
        catalog::delete_category,
        catalog::list_products,
        catalog::create_product,
        catalog::get_product,
        catalog::update_product,
        catalog::change_status,
        catalog::public_products,
        catalog::public_product,
        catalog::public_categories,
        inventory::list,
        inventory::list_movements,
        inventory::create_movement,
        inventory::put_settings,
        crm::list,
        crm::create,
        crm::get_one,
        crm::update,
        crm::put_addresses,
        crm::request_deletion,
        crm::export,
        customer_auth::register,
        customer_auth::login,
        customer_auth::logout,
        customer_auth::me,
        customer_auth::session,
        customer_auth::verify_email,
        commerce::get_settings,
        commerce::put_settings,
        commerce::list_orders,
        commerce::get_order,
        commerce::checkout_config,
        commerce::checkout,
        commerce::current_order,
        commerce::current_gopay_qris,
        commerce::customer_orders,
        commerce::claim_payment,
        commerce::upload_proof,
        commerce::get_proof,
        commerce::confirm_payment,
        commerce::reject_payment,
        commerce::create_refund,
        commerce::change_status,
        commerce::approve_order,
        commerce::reject_order,
        team::permissions,
        team::list_members,
        team::update_member,
        team::list_roles,
        team::create_role,
        team::update_role,
        team::delete_role,
        team::list_invitations,
        team::invite,
        team::revoke_invitation,
        team::accept,
        approvals::entity_types,
        approvals::list_policies,
        approvals::put_policy,
        approvals::list,
        approvals::get_one,
        approvals::approve,
        approvals::reject,
        approvals::cancel,
        approvals::list_delegations,
        approvals::create_delegation,
        approvals::revoke_delegation,
        tickets::list,
        tickets::create,
        tickets::get_one,
        tickets::assign,
        tickets::set_status,
        tickets::update_meta,
        tickets::add_message,
        tickets::add_attachment,
        tickets::get_attachment,
        tickets::customer_create,
        tickets::customer_list,
        tickets::customer_reply,
        tickets::customer_attach,
        tickets::customer_attachment,
        notifications::list,
        notifications::read_one,
        notifications::read_all,
        audit::list,
        ai::list_providers,
        ai::create_provider,
        ai::update_provider,
        ai::delete_provider,
        ai::get_config,
        ai::put_config,
        ai::put_tools,
        ai::test_turn,
        ai::usage,
        ai::list_knowledge,
        ai::create_knowledge,
        ai::delete_knowledge,
        ai::reindex_knowledge
    ),
    components(schemas(ErrorEnvelope, ErrorBody, ErrorCode, Meta)),
    servers((url = "/", description = "Origin yang sama; root domain menunggu D-01")),
    modifiers(&CommonErrors, &PublicOps, &UniqueOperationIds, &PruneUnused),
    tags(
        (name = "ops", description = "Liveness/readiness untuk orchestrator"),
        (name = "auth", description = "Registrasi, login, sesi (cookie HttpOnly + CSRF, ADR-0015)"),
        (name = "tenants", description = "Tenant milik pemanggil; tenant_id di path diverifikasi terhadap membership"),
        (name = "website", description = "Template, draft, publish, rollback (ADR-0007, ADR-0014)"),
        (name = "public", description = "Situs publik tenant; tenant dari Host, tanpa sesi (ADR-0014)"),
        (name = "business", description = "Profil, alamat, jam operasional tenant (PRD.md §4.4)"),
        (name = "catalog", description = "Produk, varian, kategori (PRD.md §4.5)"),
        (name = "commerce", description = "Pengaturan checkout, order, pembayaran QRIS statis (PRD.md §4.7, ADR-0004)"),
        (name = "approvals", description = "Approval engine multi-step (PRD.md §4.9, P4-2)"),
        (name = "audit", description = "Audit log append-only (PRD.md §6.9, P4-5)"),
        (name = "ai", description = "Pengaturan AI: provider platform/BYOK, kunci masked, konfigurasi chatbot (PRD.md §4.10, ADR-0011)"),
        (name = "notifications", description = "Notifikasi in-app (PRD.md §4.13, P4-4)"),
        (name = "tickets", description = "Ticket support: lifecycle, assign, balasan, SLA (PRD.md §4.8, P4-3)"),
        (name = "team", description = "Anggota, role kustom, undangan (PRD.md §3, P4-1)"),
        (name = "crm", description = "Customer, tag, catatan internal, hapus/export (PRD.md §4.6, ADR-0005)"),
        (name = "inventory", description = "Stok, movement ledger, ambang low-stock (PRD.md §4.5)"),
        (name = "files", description = "Upload gambar tenant + signed URL ber-expiry (ADR-0009)"),
        (name = "conversations", description = "Conversation viewer staf; data sensitif dimasking sesuai permission (PRD.md §4.12, P6-3)"),
        (name = "analytics", description = "Analitik percakapan dari agregat harian yang bertahan setelah purge (PRD.md §4.12, P6-6)"),
        (name = "billing", description = "Paket Starter/Pro & langganan toko; batas paket = data (PRD.md §4.15, ADR-0016)"),
        (name = "platform", description = "Admin platform (super_admin + sesi MFA): verifikasi pembayaran langganan lintas toko, diaudit (ADR-0016)"),
        (name = "chat", description = "Web chat publik toko: tenant dari Host, cookie sesi chat host-only (ADR-0006, P6-2)")
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

const PUBLIC_PATHS: [&str; 9] = [
    "/api/v1/plans",
    "/api/v1/public/site",
    "/files/{object_id}",
    "/healthz",
    "/readyz",
    "/api/v1/auth/register",
    "/api/v1/auth/login",
    "/api/v1/auth/login/mfa",
    "/api/v1/auth/verify-email",
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

/// utoipa memakai nama fungsi sebagai `operationId`; nama yang sama di modul berbeda (mis. `list`)
/// bentrok. Id ganda diberi awalan tag (`approvals_list`) — deterministik, klien hasil generate tetap stabil.
struct UniqueOperationIds;

impl Modify for UniqueOperationIds {
    fn modify(&self, openapi: &mut utoipa::openapi::OpenApi) {
        let mut seen: std::collections::BTreeMap<String, usize> = Default::default();
        for item in openapi.paths.paths.values_mut() {
            for op in operations(item) {
                if let Some(id) = &op.operation_id {
                    *seen.entry(id.clone()).or_default() += 1;
                }
            }
        }
        for item in openapi.paths.paths.values_mut() {
            for op in operations(item) {
                let Some(id) = op.operation_id.clone() else {
                    continue;
                };
                if seen.get(&id).copied().unwrap_or(0) > 1 {
                    let tag = op
                        .tags
                        .as_ref()
                        .and_then(|t| t.first())
                        .cloned()
                        .unwrap_or_default();
                    op.operation_id = Some(format!("{tag}_{id}"));
                }
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
