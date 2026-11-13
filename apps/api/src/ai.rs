//! Pengaturan AI tenant (P5-2, `PRD.md §4.10`, ADR-0011): provider platform/BYOK, kunci API
//! (hanya masuk, tidak pernah keluar — AC-AIC-003), konfigurasi chatbot (AC-AIC-001), tool.
//! Baca `ai_config.read`; konfigurasi `ai_config.manage`; provider & kunci `ai_provider.manage`.

use std::sync::Arc;

use axum::{
    Json, Router,
    extract::{Path, State},
    http::StatusCode,
    routing::{get, patch, put},
};
use serde::{Deserialize, Serialize};
use umkm_domain::ai::{
    self, AiMode, Capabilities, ConfigError, GenerationParams, PROVIDER_NAME_MAX, PROVIDER_TYPES,
    ProviderSource, Tool,
};
use umkm_infrastructure::{
    ai_repo::{self as repo, AiRepoError, ChatbotConfig, NewProvider, ProviderPatch, ProviderRow},
    http_client::ssrf,
};
use umkm_shared::http::{ErrorCode, SuccessEnvelope};
use utoipa::ToSchema;
use uuid::Uuid;

use crate::{
    ApiError, AppState,
    auth::ClientIp,
    http_util::{conflict, internal, not_found, pool},
    middleware::ReqId,
    session::Member,
    state::AiService,
    validation::{field_errors, one},
};

pub const KEY_MIN: usize = 8;
pub const KEY_MAX: usize = 512;

#[derive(Debug, Serialize, ToSchema)]
pub struct CapabilitiesView {
    pub chat: bool,
    pub streaming: bool,
    pub tool_calling: bool,
    pub usage_reporting: bool,
    pub embeddings: bool,
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct CapabilitiesInput {
    #[serde(default)]
    pub chat: bool,
    #[serde(default)]
    pub streaming: bool,
    #[serde(default)]
    pub tool_calling: bool,
    #[serde(default)]
    pub usage_reporting: bool,
    #[serde(default)]
    pub embeddings: bool,
}

impl From<CapabilitiesInput> for Capabilities {
    fn from(c: CapabilitiesInput) -> Self {
        Self {
            chat: c.chat,
            streaming: c.streaming,
            tool_calling: c.tool_calling,
            usage_reporting: c.usage_reporting,
            embeddings: c.embeddings,
        }
    }
}

/// Provider tanpa kunci: hanya `key_hint` masked (AC-AIC-003).
#[derive(Debug, Serialize, ToSchema)]
pub struct ProviderView {
    pub id: Uuid,
    pub name: String,
    /// `platform` | `byok`
    pub source: String,
    pub provider_type: String,
    /// `null` untuk provider platform (URL milik operator tidak diungkap).
    pub base_url: Option<String>,
    pub model: String,
    /// `active` | `disabled`
    pub status: String,
    pub capabilities: CapabilitiesView,
    /// Contoh `••••abcd`; `null` bila tanpa kunci.
    pub key_hint: Option<String>,
    pub has_key: bool,
    pub updated_at: String,
}

fn view(p: ProviderRow) -> ProviderView {
    let c = p.capabilities;
    ProviderView {
        id: p.id,
        name: p.name,
        source: p.source.as_str().into(),
        provider_type: p.provider_type,
        base_url: p.base_url,
        model: p.model,
        status: p.status,
        capabilities: CapabilitiesView {
            chat: c.chat,
            streaming: c.streaming,
            tool_calling: c.tool_calling,
            usage_reporting: c.usage_reporting,
            embeddings: c.embeddings,
        },
        key_hint: p.key_hint,
        has_key: p.has_key,
        updated_at: p.updated_at,
    }
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct ProviderInput {
    pub name: String,
    /// `platform` | `byok`
    pub source: String,
    /// BYOK: `openai` | `openrouter` | `groq` | `together` | `custom`
    pub provider_type: Option<String>,
    /// BYOK: endpoint OpenAI-compatible HTTPS publik
    pub base_url: Option<String>,
    pub model: String,
    /// BYOK: kunci API (tidak pernah dikembalikan)
    pub api_key: Option<String>,
    /// BYOK: override capability registry; default menurut provider_type
    pub capabilities: Option<CapabilitiesInput>,
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct ProviderPatchInput {
    pub name: Option<String>,
    pub model: Option<String>,
    pub active: Option<bool>,
    /// BYOK: rotasi kunci
    pub api_key: Option<String>,
    pub capabilities: Option<CapabilitiesInput>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct PlatformView {
    pub available: bool,
    pub models: Vec<String>,
    /// Kuota bulanan per tenant (token)
    pub monthly_quota: Option<u64>,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct ConfigBody {
    pub enabled: bool,
    /// `auto` | `assisted` | `hybrid` | `human_only` (default hybrid)
    pub mode: String,
    pub primary_provider_id: Option<Uuid>,
    pub fallback_provider_id: Option<Uuid>,
    pub temperature: f64,
    pub max_output_tokens: u32,
    pub timeout_ms: u32,
    pub system_prompt: String,
    pub welcome_message: String,
    pub fallback_message: String,
    pub allowed_topics: Vec<String>,
    pub restricted_topics: Vec<String>,
    #[schema(value_type = Object)]
    pub handover_policy: serde_json::Value,
    #[schema(value_type = Object)]
    pub operating_schedule: serde_json::Value,
    pub monthly_token_budget: Option<i64>,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct ToolState {
    pub name: String,
    pub enabled: bool,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct ConfigView {
    pub config: ConfigBody,
    pub tools: Vec<ToolState>,
    pub platform: PlatformView,
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct ToolsInput {
    pub tools: Vec<ToolState>,
}

fn svc(state: &AppState) -> Result<&Arc<AiService>, ApiError> {
    state
        .ai
        .as_ref()
        .ok_or_else(|| ApiError::new(ErrorCode::ServiceUnavailable))
}

fn repo_err(e: AiRepoError) -> ApiError {
    match e {
        AiRepoError::NotFound => not_found(),
        AiRepoError::NameTaken => conflict("name_taken", "Nama provider sudah dipakai"),
        AiRepoError::InUse => conflict("provider_in_use", "Provider dipakai chatbot yang aktif"),
        AiRepoError::Crypto => internal("ai secret crypto failure"),
        AiRepoError::Db(e) => internal(e),
    }
}

fn config_err(e: ConfigError) -> ApiError {
    let (f, code) = match e {
        ConfigError::OutOfRange(f) => (f, "out_of_range"),
        ConfigError::TooLong(f) => (f, "too_long"),
        ConfigError::Invalid(f) => (f, "invalid"),
    };
    one(f, code)
}

/// Kunci API: ASCII tercetak tanpa spasi (header Authorization), 8–512.
fn check_key(k: &str) -> Result<(), ApiError> {
    if (KEY_MIN..=KEY_MAX).contains(&k.len()) && k.bytes().all(|b| b.is_ascii_graphic()) {
        Ok(())
    } else {
        Err(one("api_key", "invalid_api_key"))
    }
}

fn check_name(n: &str) -> Result<String, ApiError> {
    let n = n.trim();
    if n.is_empty() || n.chars().count() > PROVIDER_NAME_MAX || n.chars().any(char::is_control) {
        return Err(one("name", "invalid_name"));
    }
    Ok(n.to_owned())
}

fn platform_model_ok(s: &AiService, model: &str) -> bool {
    s.platform
        .as_ref()
        .is_some_and(|p| p.models.iter().any(|m| m == model))
}

fn platform_caps(s: &AiService) -> Capabilities {
    let p = s.platform.as_ref();
    Capabilities {
        chat: true,
        streaming: false,
        tool_calling: p.is_some_and(|p| p.tool_calling),
        usage_reporting: true,
        embeddings: p.is_some_and(|p| p.embeddings),
    }
}

type Ok<T> = Result<Json<SuccessEnvelope<T>>, ApiError>;

/// Daftar provider AI tenant (API key tidak pernah dikembalikan).
#[utoipa::path(
    get, path = "/api/v1/tenants/{tenant_id}/ai/providers", tag = "ai",
    params(("tenant_id" = Uuid, Path)),
    responses((status = 200, description = "Provider (kunci tidak pernah dikembalikan)", body = SuccessEnvelope<Vec<ProviderView>>))
)]
pub async fn list_providers(
    State(state): State<AppState>,
    ReqId(id): ReqId,
    Member(actor): Member,
) -> Ok<Vec<ProviderView>> {
    actor.require("ai_config.read")?;
    let rows = repo::list_providers(pool(&state)?, actor.tenant_id)
        .await
        .map_err(repo_err)?;
    Ok(Json(SuccessEnvelope::new(
        rows.into_iter().map(view).collect(),
        &id,
    )))
}

/// Tambah provider AI tenant (BYOK); API key dienkripsi.
#[utoipa::path(
    post, path = "/api/v1/tenants/{tenant_id}/ai/providers", tag = "ai",
    params(("tenant_id" = Uuid, Path)), request_body = ProviderInput,
    responses(
        (status = 201, description = "Dibuat", body = SuccessEnvelope<ProviderView>),
        (status = 409, description = "Nama dipakai", body = umkm_shared::http::ErrorEnvelope)
    )
)]
pub async fn create_provider(
    State(state): State<AppState>,
    ReqId(id): ReqId,
    ClientIp(ip): ClientIp,
    Member(actor): Member,
    Json(req): Json<ProviderInput>,
) -> Result<(StatusCode, Json<SuccessEnvelope<ProviderView>>), ApiError> {
    actor.require("ai_provider.manage")?;
    let s = svc(&state)?;
    let name = check_name(&req.name)?;
    ai::validate_model(&req.model).map_err(config_err)?;
    let source =
        ProviderSource::parse(&req.source).ok_or_else(|| one("source", "invalid_source"))?;
    let (input, key) = match source {
        ProviderSource::Platform => {
            if req.base_url.is_some()
                || req.api_key.is_some()
                || req.provider_type.is_some()
                || req.capabilities.is_some()
            {
                return Err(field_errors([("source", "platform_fields_not_allowed")]));
            }
            if s.platform.is_none() {
                return Err(conflict(
                    "platform_unavailable",
                    "Provider AI platform belum tersedia",
                ));
            }
            if !platform_model_ok(s, &req.model) {
                return Err(one("model", "model_not_offered"));
            }
            let p = NewProvider {
                name,
                source,
                provider_type: "platform".into(),
                base_url: None,
                model: req.model,
                capabilities: platform_caps(s),
            };
            (p, None)
        }
        ProviderSource::Byok => {
            let ty = req.provider_type.unwrap_or_default();
            if !PROVIDER_TYPES.contains(&ty.as_str()) {
                return Err(one("provider_type", "invalid_provider_type"));
            }
            let url = req.base_url.unwrap_or_default();
            // Kontrol 1–3 saat simpan; kontrol 4–8 (DNS/IP, pinning, redirect, batas) di setiap panggilan.
            let t = ssrf::validate_url(url.trim(), false, false)
                .map_err(|e| one("base_url", e.code()))?;
            let key = req.api_key.ok_or_else(|| one("api_key", "required"))?;
            check_key(&key)?;
            let caps = req
                .capabilities
                .map(Capabilities::from)
                .unwrap_or_else(|| Capabilities::defaults_for(&ty));
            if !caps.chat {
                return Err(one("capabilities.chat", "required"));
            }
            let p = NewProvider {
                name,
                source,
                provider_type: ty,
                base_url: Some(t.url.as_str().trim_end_matches('/').to_owned()),
                model: req.model,
                capabilities: caps,
            };
            (p, Some(key))
        }
    };
    let row = repo::create_provider(
        pool(&state)?,
        &actor,
        input,
        key.as_deref().map(|k| (&s.keyring, k)),
        (Some(id.as_str()), Some(&ip)),
    )
    .await
    .map_err(repo_err)?;
    Ok((
        StatusCode::CREATED,
        Json(SuccessEnvelope::new(view(row), &id)),
    ))
}

/// Ubah provider AI tenant.
#[utoipa::path(
    patch, path = "/api/v1/tenants/{tenant_id}/ai/providers/{provider_id}", tag = "ai",
    params(("tenant_id" = Uuid, Path), ("provider_id" = Uuid, Path)), request_body = ProviderPatchInput,
    responses((status = 200, description = "Diperbarui (rotasi kunci / nonaktifkan)", body = SuccessEnvelope<ProviderView>))
)]
pub async fn update_provider(
    State(state): State<AppState>,
    ReqId(id): ReqId,
    ClientIp(ip): ClientIp,
    Member(actor): Member,
    Path((_, pid)): Path<(Uuid, Uuid)>,
    Json(req): Json<ProviderPatchInput>,
) -> Ok<ProviderView> {
    actor.require("ai_provider.manage")?;
    let s = svc(&state)?;
    let p = pool(&state)?;
    let cur = repo::get_provider(p, actor.tenant_id, pid)
        .await
        .map_err(repo_err)?;
    let name = req.name.as_deref().map(check_name).transpose()?;
    if let Some(m) = &req.model {
        ai::validate_model(m).map_err(config_err)?;
        if cur.source == ProviderSource::Platform && !platform_model_ok(s, m) {
            return Err(one("model", "model_not_offered"));
        }
    }
    if cur.source == ProviderSource::Platform
        && (req.api_key.is_some() || req.capabilities.is_some())
    {
        return Err(field_errors([("source", "platform_fields_not_allowed")]));
    }
    if let Some(k) = &req.api_key {
        check_key(k)?;
    }
    let caps = req.capabilities.map(Capabilities::from);
    if caps.is_some_and(|c| !c.chat) {
        return Err(one("capabilities.chat", "required"));
    }
    let row = repo::update_provider(
        p,
        &actor,
        pid,
        ProviderPatch {
            name,
            model: req.model,
            active: req.active,
            capabilities: caps,
        },
        req.api_key.as_deref().map(|k| (&s.keyring, k)),
        (Some(id.as_str()), Some(&ip)),
    )
    .await
    .map_err(repo_err)?;
    Ok(Json(SuccessEnvelope::new(view(row), &id)))
}

/// Hapus provider AI tenant.
#[utoipa::path(
    delete, path = "/api/v1/tenants/{tenant_id}/ai/providers/{provider_id}", tag = "ai",
    params(("tenant_id" = Uuid, Path), ("provider_id" = Uuid, Path)),
    responses(
        (status = 204, description = "Dihapus beserta kunci"),
        (status = 409, description = "Dipakai chatbot aktif", body = umkm_shared::http::ErrorEnvelope)
    )
)]
pub async fn delete_provider(
    State(state): State<AppState>,
    ReqId(id): ReqId,
    ClientIp(ip): ClientIp,
    Member(actor): Member,
    Path((_, pid)): Path<(Uuid, Uuid)>,
) -> Result<StatusCode, ApiError> {
    actor.require("ai_provider.manage")?;
    repo::delete_provider(pool(&state)?, &actor, pid, (Some(id.as_str()), Some(&ip)))
        .await
        .map_err(repo_err)?;
    Ok(StatusCode::NO_CONTENT)
}

fn body_of(c: ChatbotConfig) -> ConfigBody {
    ConfigBody {
        enabled: c.enabled,
        mode: c.mode.as_str().into(),
        primary_provider_id: c.primary_provider_id,
        fallback_provider_id: c.fallback_provider_id,
        temperature: c.params.temperature,
        max_output_tokens: c.params.max_output_tokens,
        timeout_ms: c.params.timeout_ms,
        system_prompt: c.system_prompt,
        welcome_message: c.welcome_message,
        fallback_message: c.fallback_message,
        allowed_topics: c.allowed_topics,
        restricted_topics: c.restricted_topics,
        handover_policy: c.handover_policy,
        operating_schedule: c.operating_schedule,
        monthly_token_budget: c.monthly_token_budget,
    }
}

async fn config_view(
    state: &AppState,
    actor: &umkm_domain::rbac::Actor,
    c: ChatbotConfig,
) -> Result<ConfigView, ApiError> {
    let s = svc(state)?;
    let tools = repo::tools(pool(state)?, actor.tenant_id)
        .await
        .map_err(repo_err)?;
    Ok(ConfigView {
        config: body_of(c),
        tools: tools
            .into_iter()
            .map(|(t, e)| ToolState {
                name: t.as_str().into(),
                enabled: e,
            })
            .collect(),
        platform: PlatformView {
            available: s.platform.is_some(),
            models: s
                .platform
                .as_ref()
                .map(|p| p.models.clone())
                .unwrap_or_default(),
            monthly_quota: s.platform.as_ref().map(|p| p.monthly_quota),
        },
    })
}

/// Konfigurasi chatbot tenant.
#[utoipa::path(
    get, path = "/api/v1/tenants/{tenant_id}/ai/config", tag = "ai",
    params(("tenant_id" = Uuid, Path)),
    responses((status = 200, description = "Konfigurasi chatbot (default bila belum diatur)", body = SuccessEnvelope<ConfigView>))
)]
pub async fn get_config(
    State(state): State<AppState>,
    ReqId(id): ReqId,
    Member(actor): Member,
) -> Ok<ConfigView> {
    actor.require("ai_config.read")?;
    let c = repo::get_config(pool(&state)?, actor.tenant_id)
        .await
        .map_err(repo_err)?;
    Ok(Json(SuccessEnvelope::new(
        config_view(&state, &actor, c).await?,
        &id,
    )))
}

const JSON_MAX: usize = 4_000;

/// Simpan konfigurasi chatbot tenant.
#[utoipa::path(
    put, path = "/api/v1/tenants/{tenant_id}/ai/config", tag = "ai",
    params(("tenant_id" = Uuid, Path)), request_body = ConfigBody,
    responses(
        (status = 200, description = "Tersimpan", body = SuccessEnvelope<ConfigView>),
        (status = 422, description = "Nilai di luar batas (AC-AIC-001)", body = umkm_shared::http::ErrorEnvelope)
    )
)]
pub async fn put_config(
    State(state): State<AppState>,
    ReqId(id): ReqId,
    ClientIp(ip): ClientIp,
    Member(actor): Member,
    Json(b): Json<ConfigBody>,
) -> Ok<ConfigView> {
    actor.require("ai_config.manage")?;
    svc(&state)?;
    let mode = AiMode::parse(&b.mode).ok_or_else(|| one("mode", "invalid_mode"))?;
    let params = GenerationParams {
        temperature: b.temperature,
        max_output_tokens: b.max_output_tokens,
        timeout_ms: b.timeout_ms,
    };
    params.validate().map_err(config_err)?;
    let topics: Vec<String> = b
        .allowed_topics
        .iter()
        .chain(&b.restricted_topics)
        .cloned()
        .collect();
    ai::validate_texts(
        &b.system_prompt,
        &b.welcome_message,
        &b.fallback_message,
        &topics,
    )
    .map_err(config_err)?;
    if b.allowed_topics.len() > ai::TOPICS_MAX || b.restricted_topics.len() > ai::TOPICS_MAX {
        return Err(one("topics", "out_of_range"));
    }
    for (f, v) in [
        ("handover_policy", &b.handover_policy),
        ("operating_schedule", &b.operating_schedule),
    ] {
        if !v.is_object() || v.to_string().len() > JSON_MAX {
            return Err(one(f, "invalid"));
        }
    }
    if b.monthly_token_budget.is_some_and(|x| x <= 0) {
        return Err(one("monthly_token_budget", "out_of_range"));
    }
    if b.fallback_provider_id.is_some() && b.fallback_provider_id == b.primary_provider_id {
        return Err(one("fallback_provider_id", "same_as_primary"));
    }
    if b.enabled && mode != AiMode::HumanOnly && b.primary_provider_id.is_none() {
        return Err(one("primary_provider_id", "required"));
    }
    let c = ChatbotConfig {
        enabled: b.enabled,
        mode,
        primary_provider_id: b.primary_provider_id,
        fallback_provider_id: b.fallback_provider_id,
        params,
        system_prompt: b.system_prompt,
        welcome_message: b.welcome_message,
        fallback_message: b.fallback_message.trim().to_owned(),
        allowed_topics: b
            .allowed_topics
            .iter()
            .map(|t| t.trim().to_owned())
            .collect(),
        restricted_topics: b
            .restricted_topics
            .iter()
            .map(|t| t.trim().to_owned())
            .collect(),
        handover_policy: b.handover_policy,
        operating_schedule: b.operating_schedule,
        monthly_token_budget: b.monthly_token_budget,
    };
    let saved = repo::put_config(pool(&state)?, &actor, &c, (Some(id.as_str()), Some(&ip)))
        .await
        .map_err(|e| match e {
            AiRepoError::NotFound => one("primary_provider_id", "provider_not_available"),
            e => repo_err(e),
        })?;
    Ok(Json(SuccessEnvelope::new(
        config_view(&state, &actor, saved).await?,
        &id,
    )))
}

/// Atur tool yang boleh dipakai chatbot.
#[utoipa::path(
    put, path = "/api/v1/tenants/{tenant_id}/ai/tools", tag = "ai",
    params(("tenant_id" = Uuid, Path)), request_body = ToolsInput,
    responses((status = 200, description = "Status tool", body = SuccessEnvelope<Vec<ToolState>>))
)]
pub async fn put_tools(
    State(state): State<AppState>,
    ReqId(id): ReqId,
    ClientIp(ip): ClientIp,
    Member(actor): Member,
    Json(req): Json<ToolsInput>,
) -> Ok<Vec<ToolState>> {
    actor.require("ai_config.manage")?;
    let mut set = Vec::with_capacity(req.tools.len());
    for t in &req.tools {
        let tool = Tool::parse(&t.name).ok_or_else(|| one("tools", "unknown_tool"))?;
        if set.iter().any(|(x, _)| *x == tool) {
            return Err(one("tools", "duplicate"));
        }
        set.push((tool, t.enabled));
    }
    let p = pool(&state)?;
    repo::set_tools(p, &actor, &set, (Some(id.as_str()), Some(&ip)))
        .await
        .map_err(repo_err)?;
    let now = repo::tools(p, actor.tenant_id).await.map_err(repo_err)?;
    Ok(Json(SuccessEnvelope::new(
        now.into_iter()
            .map(|(t, e)| ToolState {
                name: t.as_str().into(),
                enabled: e,
            })
            .collect(),
        &id,
    )))
}

pub fn routes() -> Router<AppState> {
    let t = "/api/v1/tenants/{tenant_id}/ai";
    Router::new()
        .route(
            &format!("{t}/providers"),
            get(list_providers).post(create_provider),
        )
        .route(
            &format!("{t}/providers/{{provider_id}}"),
            patch(update_provider).delete(delete_provider),
        )
        .route(&format!("{t}/config"), get(get_config).put(put_config))
        .route(&format!("{t}/tools"), put(put_tools))
}

// ------------------------------------------------------------ uji coba, pemakaian, basis pengetahuan

#[derive(Debug, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct TestInput {
    pub message: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct ToolCallView {
    pub name: String,
    pub status: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct TestResult {
    pub run_id: Uuid,
    /// `send_auto` | `draft_for_admin` | `handover` | `human_queue`
    pub decision: String,
    pub reply: Option<String>,
    /// `ok` | `blocked` | `error` | `timeout` | `quota_exceeded` | `circuit_open`
    pub status: String,
    pub error_code: Option<String>,
    pub tool_calls: Vec<ToolCallView>,
    pub prompt_tokens: u32,
    pub completion_tokens: u32,
}

/// Uji coba chatbot oleh admin (tidak butuh chatbot aktif; dicatat purpose `test`, dihitung kuota).
#[utoipa::path(
    post, path = "/api/v1/tenants/{tenant_id}/ai/test", tag = "ai",
    params(("tenant_id" = Uuid, Path)), request_body = TestInput,
    responses(
        (status = 200, description = "Hasil satu giliran", body = SuccessEnvelope<TestResult>),
        (status = 409, description = "Provider utama belum diatur", body = umkm_shared::http::ErrorEnvelope)
    )
)]
pub async fn test_turn(
    State(state): State<AppState>,
    ReqId(id): ReqId,
    Member(actor): Member,
    Json(req): Json<TestInput>,
) -> Ok<TestResult> {
    actor.require("ai_config.manage")?;
    let s = svc(&state)?;
    let rt = &s.runtime;
    let name = rt.business_name(actor.tenant_id).await.map_err(internal)?;
    let input = umkm_application::ai::orchestrator::TurnInput {
        message: req.message,
        history: vec![],
        customer: None,
    };
    let r = rt
        .answer(actor.tenant_id, &name, input, "test")
        .await
        .map_err(|e| match e {
            umkm_infrastructure::ai::runtime::AiRunError::NotConfigured => {
                conflict("ai_not_configured", "Pilih provider utama terlebih dahulu")
            }
            e => internal(e),
        })?;
    let o = r.outcome;
    Ok(Json(SuccessEnvelope::new(
        TestResult {
            run_id: r.run_id,
            decision: umkm_infrastructure::ai::runtime::decision_str(o.decision).into(),
            reply: o.reply,
            status: o.status.into(),
            error_code: o.error_code.map(str::to_owned),
            tool_calls: o
                .tool_calls
                .into_iter()
                .map(|c| ToolCallView {
                    name: c.name,
                    status: c.status.into(),
                })
                .collect(),
            prompt_tokens: o.usage.prompt_tokens,
            completion_tokens: o.usage.completion_tokens,
        },
        &id,
    )))
}

#[derive(Debug, Serialize, ToSchema)]
pub struct UsageView {
    /// Token bulan berjalan (zona Asia/Jakarta)
    pub platform_tokens: u64,
    pub byok_tokens: u64,
    pub platform_quota: Option<u64>,
    pub byok_budget: Option<i64>,
}

/// Pemakaian AI tenant (token & biaya).
#[utoipa::path(
    get, path = "/api/v1/tenants/{tenant_id}/ai/usage", tag = "ai",
    params(("tenant_id" = Uuid, Path)),
    responses((status = 200, description = "Pemakaian token bulan ini", body = SuccessEnvelope<UsageView>))
)]
pub async fn usage(
    State(state): State<AppState>,
    ReqId(id): ReqId,
    Member(actor): Member,
) -> Ok<UsageView> {
    actor.require("ai_config.read")?;
    let s = svc(&state)?;
    let t = actor.tenant_id;
    let cfg = repo::get_config(pool(&state)?, t).await.map_err(repo_err)?;
    Ok(Json(SuccessEnvelope::new(
        UsageView {
            platform_tokens: s
                .runtime
                .month_usage(t, ProviderSource::Platform)
                .await
                .map_err(internal)?,
            byok_tokens: s
                .runtime
                .month_usage(t, ProviderSource::Byok)
                .await
                .map_err(internal)?,
            // Batas EFEKTIF (paket ∩ kuota operator), sama dengan yang ditegakkan runtime.
            // 0 = tidak termasuk paket (mis. Starter: hanya kunci API sendiri).
            platform_quota: s
                .runtime
                .month_limit(t, ProviderSource::Platform)
                .await
                .map_err(internal)?,
            byok_budget: cfg.monthly_token_budget,
        },
        &id,
    )))
}

#[derive(Debug, Serialize, ToSchema)]
pub struct DocumentView {
    pub id: Uuid,
    pub title: String,
    /// `pending` | `indexed` | `failed` | `disabled`
    pub status: String,
    pub chunk_count: i32,
    pub error_code: Option<String>,
    pub updated_at: String,
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct DocumentInput {
    pub title: String,
    pub body: String,
}

fn rag_err(e: umkm_infrastructure::rag::RagError) -> ApiError {
    use umkm_infrastructure::rag::RagError as E;
    match e {
        E::NotFound => not_found(),
        E::LimitReached => conflict("knowledge_limit", "Batas dokumen pengetahuan tercapai"),
        e => internal(e),
    }
}

/// Daftar dokumen pengetahuan chatbot.
#[utoipa::path(
    get, path = "/api/v1/tenants/{tenant_id}/ai/knowledge", tag = "ai",
    params(("tenant_id" = Uuid, Path)),
    responses((status = 200, description = "Dokumen pengetahuan", body = SuccessEnvelope<Vec<DocumentView>>))
)]
pub async fn list_knowledge(
    State(state): State<AppState>,
    ReqId(id): ReqId,
    Member(actor): Member,
) -> Ok<Vec<DocumentView>> {
    actor.require("ai_config.read")?;
    let rows = umkm_infrastructure::rag::list_documents(pool(&state)?, actor.tenant_id)
        .await
        .map_err(rag_err)?;
    Ok(Json(SuccessEnvelope::new(
        rows.into_iter()
            .map(|d| DocumentView {
                id: d.id,
                title: d.title,
                status: d.status,
                chunk_count: d.chunk_count,
                error_code: d.error_code,
                updated_at: d.updated_at,
            })
            .collect(),
        &id,
    )))
}

/// Tambah dokumen; diindeks worker di latar (status `pending` → `indexed`).
#[utoipa::path(
    post, path = "/api/v1/tenants/{tenant_id}/ai/knowledge", tag = "ai",
    params(("tenant_id" = Uuid, Path)), request_body = DocumentInput,
    responses((status = 201, description = "Dibuat (menunggu indeks)", body = SuccessEnvelope<Uuid>))
)]
pub async fn create_knowledge(
    State(state): State<AppState>,
    ReqId(id): ReqId,
    ClientIp(ip): ClientIp,
    Member(actor): Member,
    Json(req): Json<DocumentInput>,
) -> Result<(StatusCode, Json<SuccessEnvelope<Uuid>>), ApiError> {
    actor.require("ai_config.manage")?;
    umkm_domain::ai::knowledge::validate(&req.title, &req.body).map_err(|e| match e {
        umkm_domain::ai::knowledge::KnowledgeError::Title => one("title", "invalid"),
        umkm_domain::ai::knowledge::KnowledgeError::Body => one("body", "invalid"),
    })?;
    let doc = umkm_infrastructure::rag::create_document(
        pool(&state)?,
        &actor,
        &req.title,
        &req.body,
        (Some(id.as_str()), Some(&ip)),
    )
    .await
    .map_err(rag_err)?;
    Ok((StatusCode::CREATED, Json(SuccessEnvelope::new(doc, &id))))
}

/// Hapus dokumen pengetahuan chatbot.
#[utoipa::path(
    delete, path = "/api/v1/tenants/{tenant_id}/ai/knowledge/{document_id}", tag = "ai",
    params(("tenant_id" = Uuid, Path), ("document_id" = Uuid, Path)),
    responses((status = 204, description = "Dihapus beserta chunk"))
)]
pub async fn delete_knowledge(
    State(state): State<AppState>,
    ReqId(id): ReqId,
    ClientIp(ip): ClientIp,
    Member(actor): Member,
    Path((_, doc)): Path<(Uuid, Uuid)>,
) -> Result<StatusCode, ApiError> {
    actor.require("ai_config.manage")?;
    umkm_infrastructure::rag::delete_document(
        pool(&state)?,
        &actor,
        doc,
        (Some(id.as_str()), Some(&ip)),
    )
    .await
    .map_err(rag_err)?;
    Ok(StatusCode::NO_CONTENT)
}

/// Indeks ulang dokumen pengetahuan chatbot.
#[utoipa::path(
    post, path = "/api/v1/tenants/{tenant_id}/ai/knowledge/{document_id}/reindex", tag = "ai",
    params(("tenant_id" = Uuid, Path), ("document_id" = Uuid, Path)),
    responses((status = 202, description = "Dijadwalkan ulang untuk diindeks"))
)]
pub async fn reindex_knowledge(
    State(state): State<AppState>,
    Member(actor): Member,
    Path((_, doc)): Path<(Uuid, Uuid)>,
) -> Result<StatusCode, ApiError> {
    actor.require("ai_config.manage")?;
    umkm_infrastructure::rag::mark_pending(pool(&state)?, actor.tenant_id, doc)
        .await
        .map_err(rag_err)?;
    Ok(StatusCode::ACCEPTED)
}

pub fn routes_runtime() -> Router<AppState> {
    let t = "/api/v1/tenants/{tenant_id}/ai";
    Router::new()
        .route(&format!("{t}/test"), axum::routing::post(test_turn))
        .route(&format!("{t}/usage"), get(usage))
        .route(
            &format!("{t}/knowledge"),
            get(list_knowledge).post(create_knowledge),
        )
        .route(
            &format!("{t}/knowledge/{{document_id}}"),
            axum::routing::delete(delete_knowledge),
        )
        .route(
            &format!("{t}/knowledge/{{document_id}}/reindex"),
            axum::routing::post(reindex_knowledge),
        )
}
