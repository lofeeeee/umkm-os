//! Penyimpanan provider AI, rahasia BYOK, konfigurasi chatbot & tool (P5-2, ADR-0011).
//! Kunci API hanya masuk sebagai plaintext ke fungsi di sini, langsung disegel envelope di transaksi
//! yang sama; yang keluar hanya `key_hint` (masked). Setiap perubahan diaudit (kategori AI §6.9).

use serde_json::json;
use sqlx::{PgPool, Row};
use umkm_domain::{
    ai::{ALL_TOOLS, AiMode, Capabilities, GenerationParams, ProviderSource, Tool},
    audit::AuditEvent,
    rbac::Actor,
};
use umkm_shared::ids::TenantId;
use uuid::Uuid;

use crate::{
    crypto::{self, Keyring, Sealed, Secret},
    pgx::{Tx, tenant_tx, violation},
};

pub mod actions {
    pub const PROVIDER_CREATED: &str = "ai.provider_created";
    pub const PROVIDER_UPDATED: &str = "ai.provider_updated";
    pub const KEY_ROTATED: &str = "ai.key_rotated";
    pub const PROVIDER_DELETED: &str = "ai.provider_deleted";
    pub const CONFIG_UPDATED: &str = "ai.config_updated";
    pub const TOOLS_UPDATED: &str = "ai.tools_updated";
}

#[derive(Debug, thiserror::Error)]
pub enum AiRepoError {
    #[error("tidak ditemukan")]
    NotFound,
    #[error("nama provider sudah dipakai")]
    NameTaken,
    #[error("provider masih dipakai konfigurasi aktif")]
    InUse,
    #[error("rahasia tidak dapat dibuka")]
    Crypto,
    #[error(transparent)]
    Db(#[from] sqlx::Error),
}

#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct ProviderRow {
    pub id: Uuid,
    pub name: String,
    pub source: ProviderSource,
    pub provider_type: String,
    pub base_url: Option<String>,
    pub model: String,
    pub status: String,
    pub capabilities: Capabilities,
    pub key_hint: Option<String>,
    pub has_key: bool,
    pub updated_at: String,
}

#[derive(Debug, Clone)]
pub struct NewProvider {
    pub name: String,
    pub source: ProviderSource,
    pub provider_type: String,
    pub base_url: Option<String>,
    pub model: String,
    pub capabilities: Capabilities,
}

#[derive(Debug, Clone, Default)]
pub struct ProviderPatch {
    pub name: Option<String>,
    pub model: Option<String>,
    pub active: Option<bool>,
    pub capabilities: Option<Capabilities>,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ChatbotConfig {
    pub enabled: bool,
    pub mode: AiMode,
    pub primary_provider_id: Option<Uuid>,
    pub fallback_provider_id: Option<Uuid>,
    pub params: GenerationParams,
    pub system_prompt: String,
    pub welcome_message: String,
    pub fallback_message: String,
    pub allowed_topics: Vec<String>,
    pub restricted_topics: Vec<String>,
    pub handover_policy: serde_json::Value,
    pub operating_schedule: serde_json::Value,
    pub monthly_token_budget: Option<i64>,
}

impl Default for ChatbotConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            mode: AiMode::Hybrid,
            primary_provider_id: None,
            fallback_provider_id: None,
            params: GenerationParams::default(),
            system_prompt: String::new(),
            welcome_message: String::new(),
            fallback_message: "Maaf, pertanyaan Anda akan kami teruskan ke tim kami.".into(),
            allowed_topics: vec![],
            restricted_topics: vec![],
            handover_policy: json!({}),
            operating_schedule: json!({}),
            monthly_token_budget: None,
        }
    }
}

type Meta<'a> = (Option<&'a str>, Option<&'a str>);

/// AAD rahasia provider: mengikat ciphertext ke tenant + baris (dipindah → gagal dibuka).
pub fn secret_aad(t: TenantId, provider: Uuid) -> String {
    format!("ai_provider|{}|{provider}", t.as_uuid())
}

/// Satu query statis (sqlx menolak SQL dinamis); `$1` NULL = semua provider tenant.
const SELECT_PROVIDERS: &str = "SELECT p.id, p.name, p.source, p.provider_type, p.base_url, p.model, p.status, \
    p.capabilities, p.key_hint, (s.provider_id IS NOT NULL) AS has_key, \
    to_char(p.updated_at AT TIME ZONE 'UTC', 'YYYY-MM-DD\"T\"HH24:MI:SS\"Z\"') AS updated_at \
    FROM ai_providers p LEFT JOIN ai_provider_secrets s ON s.tenant_id = p.tenant_id AND s.provider_id = p.id \
    WHERE ($1::uuid IS NULL OR p.id = $1) ORDER BY p.created_at";

fn provider_row(r: &sqlx::postgres::PgRow) -> Result<ProviderRow, sqlx::Error> {
    let source: String = r.try_get("source")?;
    let caps: serde_json::Value = r.try_get("capabilities")?;
    Ok(ProviderRow {
        id: r.try_get("id")?,
        name: r.try_get("name")?,
        source: ProviderSource::parse(&source).unwrap_or(ProviderSource::Byok),
        provider_type: r.try_get("provider_type")?,
        base_url: r.try_get("base_url")?,
        model: r.try_get("model")?,
        status: r.try_get("status")?,
        capabilities: serde_json::from_value(caps).unwrap_or_default(),
        key_hint: r.try_get("key_hint")?,
        has_key: r.try_get("has_key")?,
        updated_at: r.try_get("updated_at")?,
    })
}

pub async fn list_providers(pool: &PgPool, t: TenantId) -> Result<Vec<ProviderRow>, AiRepoError> {
    let mut tx = tenant_tx(pool, t).await?;
    let rows = sqlx::query(SELECT_PROVIDERS)
        .bind(None::<Uuid>)
        .fetch_all(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(rows.iter().map(provider_row).collect::<Result<_, _>>()?)
}

async fn get_in(tx: &mut Tx, id: Uuid) -> Result<ProviderRow, AiRepoError> {
    let r = sqlx::query(SELECT_PROVIDERS)
        .bind(Some(id))
        .fetch_optional(&mut **tx)
        .await?
        .ok_or(AiRepoError::NotFound)?;
    Ok(provider_row(&r)?)
}

pub async fn get_provider(
    pool: &PgPool,
    t: TenantId,
    id: Uuid,
) -> Result<ProviderRow, AiRepoError> {
    let mut tx = tenant_tx(pool, t).await?;
    let p = get_in(&mut tx, id).await?;
    tx.commit().await?;
    Ok(p)
}

fn name_err(e: sqlx::Error) -> AiRepoError {
    match violation(&e) {
        Some((s, c)) if s == "23505" && c == "uq_ai_providers_name" => AiRepoError::NameTaken,
        _ => AiRepoError::Db(e),
    }
}

async fn store_secret(
    tx: &mut Tx,
    t: TenantId,
    id: Uuid,
    k: &Keyring,
    key: &str,
) -> Result<String, AiRepoError> {
    let sealed = crypto::seal(k, key.as_bytes(), secret_aad(t, id).as_bytes())
        .map_err(|_| AiRepoError::Crypto)?;
    sqlx::query(
        "INSERT INTO ai_provider_secrets (tenant_id, provider_id, kek_version, wrapped_dek, ciphertext)
         VALUES ($1, $2, $3, $4, $5)
         ON CONFLICT (tenant_id, provider_id) DO UPDATE SET kek_version = EXCLUDED.kek_version,
           wrapped_dek = EXCLUDED.wrapped_dek, ciphertext = EXCLUDED.ciphertext, rotated_at = now()",
    )
    .bind(t.as_uuid())
    .bind(id)
    .bind(sealed.kek_version)
    .bind(&sealed.wrapped_dek)
    .bind(&sealed.ciphertext)
    .execute(&mut **tx)
    .await?;
    let hint = crypto::mask(key);
    sqlx::query("UPDATE ai_providers SET key_hint = $2, updated_at = now() WHERE id = $1")
        .bind(id)
        .bind(&hint)
        .execute(&mut **tx)
        .await?;
    Ok(hint)
}

pub async fn create_provider(
    pool: &PgPool,
    actor: &Actor,
    p: NewProvider,
    key: Option<(&Keyring, &str)>,
    (rid, ip): Meta<'_>,
) -> Result<ProviderRow, AiRepoError> {
    let t = actor.tenant_id;
    let mut tx = tenant_tx(pool, t).await?;
    let id: Uuid = sqlx::query_scalar(
        "INSERT INTO ai_providers (tenant_id, name, source, provider_type, base_url, model, capabilities)
         VALUES ($1, $2, $3, $4, $5, $6, $7) RETURNING id",
    )
    .bind(t.as_uuid())
    .bind(&p.name)
    .bind(p.source.as_str())
    .bind(&p.provider_type)
    .bind(&p.base_url)
    .bind(&p.model)
    .bind(serde_json::to_value(p.capabilities).unwrap_or_default())
    .fetch_one(&mut *tx)
    .await
    .map_err(name_err)?;
    if let Some((k, key)) = key {
        store_secret(&mut tx, t, id, k, key).await?;
    }
    let row = get_in(&mut tx, id).await?;
    crate::audit_repo::record(
        &mut *tx,
        &AuditEvent::new(actions::PROVIDER_CREATED, "ai_provider")
            .by_user(actor.user_id)
            .in_tenant(t)
            .resource(id)
            .request(rid, ip)
            .with_summary(json!({
                "name": row.name, "source": row.source, "provider_type": row.provider_type,
                "base_url": row.base_url, "model": row.model, "key_set": row.has_key,
            })),
    )
    .await?;
    tx.commit().await?;
    Ok(row)
}

pub async fn update_provider(
    pool: &PgPool,
    actor: &Actor,
    id: Uuid,
    patch: ProviderPatch,
    new_key: Option<(&Keyring, &str)>,
    (rid, ip): Meta<'_>,
) -> Result<ProviderRow, AiRepoError> {
    let t = actor.tenant_id;
    let mut tx = tenant_tx(pool, t).await?;
    let before = get_in(&mut tx, id).await?;
    sqlx::query(
        "UPDATE ai_providers SET name = COALESCE($2, name), model = COALESCE($3, model),
           status = COALESCE($4, status), capabilities = COALESCE($5, capabilities), updated_at = now()
         WHERE id = $1",
    )
    .bind(id)
    .bind(&patch.name)
    .bind(&patch.model)
    .bind(patch.active.map(|a| if a { "active" } else { "disabled" }))
    .bind(patch.capabilities.map(|c| serde_json::to_value(c).unwrap_or_default()))
    .execute(&mut *tx)
    .await
    .map_err(name_err)?;
    if let Some((k, key)) = new_key {
        store_secret(&mut tx, t, id, k, key).await?;
        crate::audit_repo::record(
            &mut *tx,
            &AuditEvent::new(actions::KEY_ROTATED, "ai_provider")
                .by_user(actor.user_id)
                .in_tenant(t)
                .resource(id)
                .request(rid, ip)
                .with_summary(json!({ "key_was_set": before.has_key })),
        )
        .await?;
    }
    let after = get_in(&mut tx, id).await?;
    if after != before
        && (patch.name.is_some()
            || patch.model.is_some()
            || patch.active.is_some()
            || patch.capabilities.is_some())
    {
        crate::audit_repo::record(
            &mut *tx,
            &AuditEvent::new(actions::PROVIDER_UPDATED, "ai_provider")
                .by_user(actor.user_id)
                .in_tenant(t)
                .resource(id)
                .request(rid, ip)
                .with_summary(json!({
                    "before": { "name": before.name, "model": before.model, "status": before.status, "capabilities": before.capabilities },
                    "after": { "name": after.name, "model": after.model, "status": after.status, "capabilities": after.capabilities },
                })),
        )
        .await?;
    }
    tx.commit().await?;
    Ok(after)
}

pub async fn delete_provider(
    pool: &PgPool,
    actor: &Actor,
    id: Uuid,
    (rid, ip): Meta<'_>,
) -> Result<(), AiRepoError> {
    let t = actor.tenant_id;
    let mut tx = tenant_tx(pool, t).await?;
    let before = get_in(&mut tx, id).await?;
    // Provider utama dari chatbot yang aktif tidak boleh hilang diam-diam.
    let in_use: bool = sqlx::query_scalar(
        "SELECT EXISTS (SELECT 1 FROM chatbot_configs WHERE enabled AND mode <> 'human_only' AND primary_provider_id = $1)",
    )
    .bind(id)
    .fetch_one(&mut *tx)
    .await?;
    if in_use {
        return Err(AiRepoError::InUse);
    }
    sqlx::query("DELETE FROM ai_providers WHERE id = $1")
        .bind(id)
        .execute(&mut *tx)
        .await?;
    crate::audit_repo::record(
        &mut *tx,
        &AuditEvent::new(actions::PROVIDER_DELETED, "ai_provider")
            .by_user(actor.user_id)
            .in_tenant(t)
            .resource(id)
            .request(rid, ip)
            .with_summary(json!({ "name": before.name, "source": before.source })),
    )
    .await?;
    tx.commit().await?;
    Ok(())
}

/// Buka kunci BYOK untuk satu panggilan provider. `None` = provider tanpa kunci.
pub async fn load_key(
    pool: &PgPool,
    t: TenantId,
    id: Uuid,
    k: &Keyring,
) -> Result<Option<Secret>, AiRepoError> {
    let mut tx = tenant_tx(pool, t).await?;
    let r: Option<(i16, Vec<u8>, Vec<u8>)> = sqlx::query_as(
        "SELECT kek_version, wrapped_dek, ciphertext FROM ai_provider_secrets WHERE provider_id = $1",
    )
    .bind(id)
    .fetch_optional(&mut *tx)
    .await?;
    tx.commit().await?;
    r.map(|(kek_version, wrapped_dek, ciphertext)| {
        crypto::open(
            k,
            &Sealed {
                kek_version,
                wrapped_dek,
                ciphertext,
            },
            secret_aad(t, id).as_bytes(),
        )
        .map_err(|_| AiRepoError::Crypto)
    })
    .transpose()
}

pub async fn get_config(pool: &PgPool, t: TenantId) -> Result<ChatbotConfig, AiRepoError> {
    let mut tx = tenant_tx(pool, t).await?;
    let c = config_in(&mut tx).await?;
    tx.commit().await?;
    Ok(c)
}

async fn config_in(tx: &mut Tx) -> Result<ChatbotConfig, AiRepoError> {
    let Some(r) = sqlx::query(
        "SELECT enabled, mode, primary_provider_id, fallback_provider_id, temperature::float8 AS temperature,
                max_output_tokens, timeout_ms, system_prompt, welcome_message, fallback_message, allowed_topics,
                restricted_topics, handover_policy, operating_schedule, monthly_token_budget
         FROM chatbot_configs",
    )
    .fetch_optional(&mut **tx)
    .await?
    else {
        return Ok(ChatbotConfig::default());
    };
    let mode: String = r.try_get("mode")?;
    let max: i32 = r.try_get("max_output_tokens")?;
    let timeout: i32 = r.try_get("timeout_ms")?;
    Ok(ChatbotConfig {
        enabled: r.try_get("enabled")?,
        mode: AiMode::parse(&mode).unwrap_or_default(),
        primary_provider_id: r.try_get("primary_provider_id")?,
        fallback_provider_id: r.try_get("fallback_provider_id")?,
        params: GenerationParams {
            temperature: r.try_get("temperature")?,
            max_output_tokens: max as u32,
            timeout_ms: timeout as u32,
        },
        system_prompt: r.try_get("system_prompt")?,
        welcome_message: r.try_get("welcome_message")?,
        fallback_message: r.try_get("fallback_message")?,
        allowed_topics: r.try_get("allowed_topics")?,
        restricted_topics: r.try_get("restricted_topics")?,
        handover_policy: r.try_get("handover_policy")?,
        operating_schedule: r.try_get("operating_schedule")?,
        monthly_token_budget: r.try_get("monthly_token_budget")?,
    })
}

/// Simpan konfigurasi (sudah divalidasi domain di pemanggil; DB menegakkan ulang batasnya).
/// Provider yang dirujuk harus milik tenant (FK komposit) dan aktif.
pub async fn put_config(
    pool: &PgPool,
    actor: &Actor,
    c: &ChatbotConfig,
    (rid, ip): Meta<'_>,
) -> Result<ChatbotConfig, AiRepoError> {
    let t = actor.tenant_id;
    let mut tx = tenant_tx(pool, t).await?;
    for p in [c.primary_provider_id, c.fallback_provider_id]
        .into_iter()
        .flatten()
    {
        let ok: bool = sqlx::query_scalar(
            "SELECT EXISTS (SELECT 1 FROM ai_providers WHERE id = $1 AND status = 'active')",
        )
        .bind(p)
        .fetch_one(&mut *tx)
        .await?;
        if !ok {
            return Err(AiRepoError::NotFound);
        }
    }
    let before = config_in(&mut tx).await?;
    sqlx::query(
        "INSERT INTO chatbot_configs (tenant_id, enabled, mode, primary_provider_id, fallback_provider_id, temperature,
            max_output_tokens, timeout_ms, system_prompt, welcome_message, fallback_message, allowed_topics,
            restricted_topics, handover_policy, operating_schedule, monthly_token_budget, updated_by_user_id)
         VALUES ($1, $2, $3, $4, $5, $6::numeric(3,2), $7, $8, $9, $10, $11, $12, $13, $14, $15, $16, $17)
         ON CONFLICT (tenant_id) DO UPDATE SET enabled = EXCLUDED.enabled, mode = EXCLUDED.mode,
            primary_provider_id = EXCLUDED.primary_provider_id, fallback_provider_id = EXCLUDED.fallback_provider_id,
            temperature = EXCLUDED.temperature, max_output_tokens = EXCLUDED.max_output_tokens,
            timeout_ms = EXCLUDED.timeout_ms, system_prompt = EXCLUDED.system_prompt,
            welcome_message = EXCLUDED.welcome_message, fallback_message = EXCLUDED.fallback_message,
            allowed_topics = EXCLUDED.allowed_topics, restricted_topics = EXCLUDED.restricted_topics,
            handover_policy = EXCLUDED.handover_policy, operating_schedule = EXCLUDED.operating_schedule,
            monthly_token_budget = EXCLUDED.monthly_token_budget, updated_by_user_id = EXCLUDED.updated_by_user_id,
            updated_at = now()",
    )
    .bind(t.as_uuid())
    .bind(c.enabled)
    .bind(c.mode.as_str())
    .bind(c.primary_provider_id)
    .bind(c.fallback_provider_id)
    .bind(c.params.temperature)
    .bind(c.params.max_output_tokens as i32)
    .bind(c.params.timeout_ms as i32)
    .bind(&c.system_prompt)
    .bind(&c.welcome_message)
    .bind(&c.fallback_message)
    .bind(&c.allowed_topics)
    .bind(&c.restricted_topics)
    .bind(&c.handover_policy)
    .bind(&c.operating_schedule)
    .bind(c.monthly_token_budget)
    .bind(actor.user_id)
    .execute(&mut *tx)
    .await?;
    let after = config_in(&mut tx).await?;
    // Ringkasan tanpa isi prompt (bisa memuat info internal toko): hanya panjangnya.
    let brief = |c: &ChatbotConfig| {
        json!({
            "enabled": c.enabled, "mode": c.mode, "primary_provider_id": c.primary_provider_id,
            "fallback_provider_id": c.fallback_provider_id, "params": c.params,
            "system_prompt_len": c.system_prompt.chars().count(), "monthly_token_budget": c.monthly_token_budget,
            "allowed_topics": c.allowed_topics, "restricted_topics": c.restricted_topics,
        })
    };
    crate::audit_repo::record(
        &mut *tx,
        &AuditEvent::new(actions::CONFIG_UPDATED, "chatbot_config")
            .by_user(actor.user_id)
            .in_tenant(t)
            .request(rid, ip)
            .with_summary(json!({ "before": brief(&before), "after": brief(&after) })),
    )
    .await?;
    tx.commit().await?;
    Ok(after)
}

/// Status 10 tool (default bila belum pernah diatur).
pub async fn tools(pool: &PgPool, t: TenantId) -> Result<Vec<(Tool, bool)>, AiRepoError> {
    let mut tx = tenant_tx(pool, t).await?;
    let rows: Vec<(String, bool)> = sqlx::query_as("SELECT tool_name, enabled FROM chatbot_tools")
        .fetch_all(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(ALL_TOOLS
        .into_iter()
        .map(|tool| {
            let set = rows
                .iter()
                .find(|(n, _)| n == tool.as_str())
                .map(|(_, e)| *e);
            (tool, set.unwrap_or(tool.enabled_by_default()))
        })
        .collect())
}

pub async fn set_tools(
    pool: &PgPool,
    actor: &Actor,
    set: &[(Tool, bool)],
    (rid, ip): Meta<'_>,
) -> Result<(), AiRepoError> {
    let t = actor.tenant_id;
    let mut tx = tenant_tx(pool, t).await?;
    for (tool, enabled) in set {
        sqlx::query(
            "INSERT INTO chatbot_tools (tenant_id, tool_name, enabled) VALUES ($1, $2, $3)
             ON CONFLICT (tenant_id, tool_name) DO UPDATE SET enabled = EXCLUDED.enabled, updated_at = now()",
        )
        .bind(t.as_uuid())
        .bind(tool.as_str())
        .bind(enabled)
        .execute(&mut *tx)
        .await?;
    }
    crate::audit_repo::record(
        &mut *tx,
        &AuditEvent::new(actions::TOOLS_UPDATED, "chatbot_tools")
            .by_user(actor.user_id)
            .in_tenant(t)
            .request(rid, ip)
            .with_summary(json!({ "changes": set.iter().map(|(t, e)| json!({ "tool": t.as_str(), "enabled": e })).collect::<Vec<_>>() })),
    )
    .await?;
    tx.commit().await?;
    Ok(())
}
