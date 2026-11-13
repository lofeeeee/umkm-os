//! Runtime AI (P5-8): merangkai konfigurasi tenant, kuota, circuit breaker, adapter provider
//! (platform/BYOK), tool backend & retriever tenant, orchestrator, lalu mencatat metering
//! (`ai_runs` + `ai_tool_calls`). Provider lambat/mati tidak menahan API: timeout per config,
//! konkurensi dibatasi, breaker membuka setelah gagal beruntun, dan kegagalan selalu berujung
//! handover ke manusia — bukan error ke pelanggan.

use std::{
    sync::Arc,
    time::{Duration, Instant},
};

use serde_json::json;
use sqlx::PgPool;
use umkm_application::ai::{
    llm::{LlmError, LlmPort, Usage},
    orchestrator::{
        NoRetrieval, ProviderUsed, Retriever, TurnConfig, TurnInput, TurnOutcome, run_turn,
    },
    usage::{Limit, effective_usage},
};
use umkm_domain::ai::{Decision, ProviderSource, Tool};
use umkm_shared::{
    config::{PlatformAiSettings, SecretString},
    ids::TenantId,
};
use uuid::Uuid;

use super::{
    circuit_breaker::CircuitBreaker,
    provider_adapter::{ApiKey, OpenAiCompatible},
    tool_backend::PgToolBackend,
};
use crate::{
    ai_repo::{self, ChatbotConfig, ProviderRow},
    crypto::Keyring,
    http_client::Egress,
    pgx::tenant_tx,
    rag::PgRetriever,
};

pub const BREAKER_THRESHOLD: u32 = 5;
pub const BREAKER_COOLDOWN: Duration = Duration::from_secs(30);
/// Model embedding default untuk BYOK OpenAI (provider lain: tanpa retrieval sampai dapat dikonfigurasi).
pub const BYOK_OPENAI_EMBEDDING: &str = "text-embedding-3-small";

#[derive(Debug, thiserror::Error)]
pub enum AiRunError {
    #[error("chatbot belum dikonfigurasi")]
    NotConfigured,
    #[error("chatbot nonaktif")]
    Disabled,
    #[error("rahasia provider tidak dapat dibuka")]
    Secret,
    #[error(transparent)]
    Db(#[from] sqlx::Error),
}

impl From<ai_repo::AiRepoError> for AiRunError {
    fn from(e: ai_repo::AiRepoError) -> Self {
        match e {
            ai_repo::AiRepoError::Db(e) => Self::Db(e),
            ai_repo::AiRepoError::Crypto => Self::Secret,
            _ => Self::NotConfigured,
        }
    }
}

pub struct AiRuntime {
    pub pool: PgPool,
    pub keyring: Keyring,
    pub platform: Option<PlatformAiSettings>,
    pub platform_key: Option<SecretString>,
    /// Klien untuk endpoint tenant (HTTPS publik) dan endpoint platform (operator).
    pub tenant_egress: Egress,
    pub operator_egress: Egress,
    pub breaker: Arc<CircuitBreaker>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct RunResult {
    pub run_id: Uuid,
    pub outcome: TurnOutcome,
}

/// Konteks pencatatan satu giliran (latensi diukur sejak giliran dimulai).
struct RecordCtx {
    conversation_id: Option<Uuid>,
    started: Instant,
}

pub fn decision_str(d: Decision) -> &'static str {
    match d {
        Decision::SendAuto => "send_auto",
        Decision::DraftForAdmin => "draft_for_admin",
        Decision::Handover => "handover",
        Decision::HumanQueue => "human_queue",
    }
}

/// Retriever opsional dengan satu tipe konkret.
enum AnyRetriever<'a, L: LlmPort> {
    Pg(PgRetriever<'a, L>),
    None(NoRetrieval),
}

impl<L: LlmPort> Retriever for AnyRetriever<'_, L> {
    async fn retrieve(&self, q: &str, k: usize) -> Result<Vec<String>, LlmError> {
        match self {
            Self::Pg(r) => r.retrieve(q, k).await,
            Self::None(r) => r.retrieve(q, k).await,
        }
    }
}

fn canary() -> String {
    let mut b = [0u8; 8];
    let _ = getrandom::fill(&mut b);
    format!(
        "ref-{}",
        b.iter().map(|x| format!("{x:02x}")).collect::<String>()
    )
}

impl AiRuntime {
    /// Runtime proses: klien tenant & operator berbagi satu batas konkurensi.
    pub fn new(
        pool: PgPool,
        keyring: Keyring,
        platform: Option<PlatformAiSettings>,
        platform_key: Option<SecretString>,
        max_concurrency: usize,
    ) -> Self {
        let tenant_egress = Egress::new(
            crate::http_client::EgressPolicy::tenant(Duration::from_secs(60)),
            crate::http_client::Resolver::System,
            max_concurrency,
        );
        let operator_egress = tenant_egress.with_policy(
            crate::http_client::EgressPolicy::operator(Duration::from_secs(60)),
        );
        Self {
            pool,
            keyring,
            platform,
            platform_key,
            tenant_egress,
            operator_egress,
            breaker: Arc::new(CircuitBreaker::new(BREAKER_THRESHOLD, BREAKER_COOLDOWN)),
        }
    }

    async fn build(
        &self,
        t: TenantId,
        p: &ProviderRow,
        timeout: Duration,
    ) -> Result<OpenAiCompatible, AiRunError> {
        if p.source == ProviderSource::Platform {
            let s = self.platform.as_ref().ok_or(AiRunError::NotConfigured)?;
            // Endpoint internal operator (kontrol 9) hanya lewat kebijakan operator.
            let egress = if s.trusted_internal {
                &self.operator_egress
            } else {
                &self.tenant_egress
            };
            let key = self
                .platform_key
                .as_ref()
                .map(|k| ApiKey::new(k.expose().to_owned()));
            return Ok(OpenAiCompatible::new(
                egress.with_timeout(timeout),
                &s.base_url,
                key,
                p.capabilities,
            ));
        }
        let key = ai_repo::load_key(&self.pool, t, p.id, &self.keyring).await?;
        let key = key
            .map(|k| k.expose_str().map(|s| ApiKey::new(s.to_owned())))
            .transpose()
            .map_err(|_| AiRunError::Secret)?;
        let url = p.base_url.as_deref().ok_or(AiRunError::NotConfigured)?;
        Ok(OpenAiCompatible::new(
            self.tenant_egress.with_timeout(timeout),
            url,
            key,
            p.capabilities,
        ))
    }

    fn embedding_model(&self, p: &ProviderRow) -> Option<String> {
        if !p.capabilities.embeddings {
            return None;
        }
        match p.source {
            ProviderSource::Platform => self
                .platform
                .as_ref()
                .filter(|s| s.embeddings)
                .map(|s| s.embedding_model.clone()),
            ProviderSource::Byok => {
                (p.provider_type == "openai").then(|| BYOK_OPENAI_EMBEDDING.to_owned())
            }
        }
    }

    /// Indeks dokumen pengetahuan tertunda (worker). Embedding memakai provider utama tenant.
    /// Kembali (berhasil, gagal).
    pub async fn index_pending(&self, batch: i32) -> Result<(usize, usize), sqlx::Error> {
        let (mut ok, mut failed) = (0, 0);
        for (t, doc) in crate::rag::pending(&self.pool, batch).await? {
            let provider = match ai_repo::get_config(&self.pool, t).await {
                Ok(cfg) => ai_repo::list_providers(&self.pool, t)
                    .await
                    .ok()
                    .and_then(|ps| {
                        ps.into_iter()
                            .find(|p| Some(p.id) == cfg.primary_provider_id && p.status == "active")
                    }),
                Err(_) => None,
            };
            let Some((p, model)) = provider.and_then(|p| self.embedding_model(&p).map(|m| (p, m)))
            else {
                crate::rag::mark_failed(&self.pool, t, doc, "no_embedding_provider").await?;
                failed += 1;
                continue;
            };
            let llm = match self.build(t, &p, Duration::from_secs(30)).await {
                Ok(l) => l,
                Err(_) => {
                    crate::rag::mark_failed(&self.pool, t, doc, "provider_unavailable").await?;
                    failed += 1;
                    continue;
                }
            };
            match crate::rag::index_document(&self.pool, t, doc, &llm, &model).await {
                Ok(_) => ok += 1,
                Err(_) => failed += 1,
            }
        }
        Ok((ok, failed))
    }

    /// Nama usaha untuk system prompt (profil bisnis; fallback nama tenant).
    pub async fn business_name(&self, t: TenantId) -> Result<String, sqlx::Error> {
        let mut tx = tenant_tx(&self.pool, t).await?;
        let n: Option<String> = sqlx::query_scalar(
            "SELECT coalesce((SELECT name FROM business_profiles), (SELECT name FROM tenants))",
        )
        .fetch_one(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(n.unwrap_or_default())
    }

    /// Pemakaian token bulan berjalan (zona Asia/Jakarta) per sumber.
    pub async fn month_usage(
        &self,
        t: TenantId,
        source: ProviderSource,
    ) -> Result<u64, sqlx::Error> {
        let mut tx = tenant_tx(&self.pool, t).await?;
        let n: i64 = sqlx::query_scalar(
            "SELECT coalesce(sum(prompt_tokens::int8 + completion_tokens), 0)::int8 FROM ai_runs
             WHERE source = $1 AND created_at >= (date_trunc('month', now() AT TIME ZONE 'Asia/Jakarta') AT TIME ZONE 'Asia/Jakarta')",
        )
        .bind(source.as_str())
        .fetch_one(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(n.max(0) as u64)
    }

    fn limit(&self, cfg: &ChatbotConfig, source: ProviderSource) -> Limit {
        match source {
            ProviderSource::Platform => {
                Limit(Some(self.platform.as_ref().map_or(0, |p| p.monthly_quota)))
            }
            ProviderSource::Byok => Limit(cfg.monthly_token_budget.map(|b| b.max(0) as u64)),
        }
    }

    /// Satu giliran. `purpose` = `chat` (percakapan, butuh chatbot aktif) | `test` (uji coba admin).
    pub async fn answer(
        &self,
        t: TenantId,
        business_name: &str,
        input: TurnInput,
        purpose: &str,
    ) -> Result<RunResult, AiRunError> {
        self.answer_in(t, business_name, input, purpose, None).await
    }

    /// `answer` untuk satu percakapan (P6-2): `ai_runs.conversation_id` terisi untuk viewer/analitik.
    pub async fn answer_in(
        &self,
        t: TenantId,
        business_name: &str,
        input: TurnInput,
        purpose: &str,
        conversation_id: Option<Uuid>,
    ) -> Result<RunResult, AiRunError> {
        let rc = RecordCtx {
            conversation_id,
            started: Instant::now(),
        };
        let cfg = ai_repo::get_config(&self.pool, t).await?;
        if purpose == "chat" && !cfg.enabled {
            return Err(AiRunError::Disabled);
        }
        let providers = ai_repo::list_providers(&self.pool, t).await?;
        let find = |id: Option<Uuid>| {
            id.and_then(|id| {
                providers
                    .iter()
                    .find(|p| p.id == id && p.status == "active")
            })
            .cloned()
        };
        let primary = find(cfg.primary_provider_id);
        let fallback = find(cfg.fallback_provider_id);
        let enabled: Vec<Tool> = ai_repo::tools(&self.pool, t)
            .await?
            .into_iter()
            .filter(|(_, e)| *e)
            .map(|(t, _)| t)
            .collect();
        let canary = canary();
        let turn = |model: &'static str| TurnConfig {
            mode: cfg.mode,
            business_name,
            params: &cfg.params,
            model,
            fallback_model: None,
            tenant_prompt: &cfg.system_prompt,
            allowed_topics: &cfg.allowed_topics,
            restricted_topics: &cfg.restricted_topics,
            fallback_message: &cfg.fallback_message,
            enabled_tools: &enabled,
            canary: &canary,
        };
        let input_chars = input.message.chars().count();

        // Human Only atau tanpa provider: orchestrator berhenti sebelum provider (tanpa adapter).
        let Some(mut primary) = primary.filter(|_| cfg.mode.calls_provider()) else {
            if cfg.mode.calls_provider() {
                return Err(AiRunError::NotConfigured);
            }
            let o = run_turn(
                &Unreachable,
                None,
                &NoBackend,
                &NoRetrieval,
                &turn("-"),
                input,
            )
            .await;
            let id = self
                .record(t, &rc, None, ProviderSource::Platform, "-", purpose, &o)
                .await?;
            return Ok(RunResult {
                run_id: id,
                outcome: o,
            });
        };
        let mut fallback = fallback;
        let now = Instant::now();
        // Breaker: provider utama terbuka → pakai fallback sebagai utama bila tersedia.
        if !self.breaker.allow(&primary.id.to_string(), now) {
            match fallback
                .take()
                .filter(|f| self.breaker.allow(&f.id.to_string(), now))
            {
                Some(f) => primary = f,
                None => {
                    return self
                        .short_circuit(
                            &rc,
                            t,
                            &cfg,
                            &primary,
                            purpose,
                            "circuit_open",
                            "circuit_open",
                        )
                        .await;
                }
            }
        }
        if !self
            .limit(&cfg, primary.source)
            .allows(self.month_usage(t, primary.source).await?)
        {
            return self
                .short_circuit(
                    &rc,
                    t,
                    &cfg,
                    &primary,
                    purpose,
                    "quota_exceeded",
                    "quota_exceeded",
                )
                .await;
        }
        let timeout = Duration::from_millis(u64::from(cfg.params.timeout_ms));
        let p_llm = self.build(t, &primary, timeout).await?;
        let f_llm = match &fallback {
            Some(f) if f.id != primary.id => Some(self.build(t, f, timeout).await?),
            _ => None,
        };
        let emb = self.embedding_model(&primary);
        let retriever = match &emb {
            Some(m) => AnyRetriever::Pg(PgRetriever {
                pool: &self.pool,
                tenant: t,
                llm: &p_llm,
                model: m,
            }),
            None => AnyRetriever::None(NoRetrieval),
        };
        let backend = PgToolBackend {
            pool: &self.pool,
            tenant: t,
            faq: Some(&retriever),
        };
        let tc = TurnConfig {
            mode: cfg.mode,
            business_name,
            params: &cfg.params,
            model: &primary.model,
            fallback_model: fallback.as_ref().map(|f| f.model.as_str()),
            tenant_prompt: &cfg.system_prompt,
            allowed_topics: &cfg.allowed_topics,
            restricted_topics: &cfg.restricted_topics,
            fallback_message: &cfg.fallback_message,
            enabled_tools: &enabled,
            canary: &canary,
        };
        let mut o = run_turn(&p_llm, f_llm.as_ref(), &backend, &retriever, &tc, input).await;
        // Breaker & metering.
        let transient = matches!(o.status, "error" | "timeout")
            && o.error_code.is_some_and(|c| {
                !matches!(
                    c,
                    "unauthorized" | "bad_request" | "unsupported" | "invalid_response"
                )
            });
        let now = Instant::now();
        match o.provider {
            ProviderUsed::Primary if transient => {
                self.breaker.failure(&primary.id.to_string(), now)
            }
            ProviderUsed::Primary => self.breaker.success(&primary.id.to_string()),
            ProviderUsed::Fallback => {
                self.breaker.failure(&primary.id.to_string(), now);
                if let Some(f) = &fallback {
                    if transient {
                        self.breaker.failure(&f.id.to_string(), now)
                    } else {
                        self.breaker.success(&f.id.to_string())
                    }
                }
            }
            ProviderUsed::None => {}
        }
        if o.llm_calls > 0 && o.usage == Usage::default() {
            o.usage = effective_usage(
                None,
                input_chars + cfg.system_prompt.chars().count() + 1500,
                o.reply.as_deref().map_or(0, |r| r.chars().count()),
            );
        }
        let (used, source) = match (o.provider, &fallback) {
            (ProviderUsed::Fallback, Some(f)) => (Some(f), f.source),
            _ => (Some(&primary), primary.source),
        };
        let model = used.map_or("-", |p| p.model.as_str()).to_owned();
        let id = self
            .record(t, &rc, used.map(|p| p.id), source, &model, purpose, &o)
            .await?;
        Ok(RunResult {
            run_id: id,
            outcome: o,
        })
    }

    #[allow(clippy::too_many_arguments)]
    async fn short_circuit(
        &self,
        rc: &RecordCtx,
        t: TenantId,
        cfg: &ChatbotConfig,
        p: &ProviderRow,
        purpose: &str,
        status: &'static str,
        code: &'static str,
    ) -> Result<RunResult, AiRunError> {
        let o = TurnOutcome {
            decision: Decision::Handover,
            reply: Some(cfg.fallback_message.clone()),
            status,
            error_code: Some(code),
            usage: Usage::default(),
            provider: ProviderUsed::None,
            tool_calls: vec![],
            llm_calls: 0,
        };
        let id = self
            .record(t, rc, Some(p.id), p.source, &p.model, purpose, &o)
            .await?;
        Ok(RunResult {
            run_id: id,
            outcome: o,
        })
    }

    #[allow(clippy::too_many_arguments)]
    async fn record(
        &self,
        t: TenantId,
        rc: &RecordCtx,
        provider: Option<Uuid>,
        source: ProviderSource,
        model: &str,
        purpose: &str,
        o: &TurnOutcome,
    ) -> Result<Uuid, sqlx::Error> {
        let mut tx = tenant_tx(&self.pool, t).await?;
        let status = match o.status {
            s @ ("ok" | "blocked" | "error" | "timeout" | "quota_exceeded" | "circuit_open") => s,
            _ => "error",
        };
        let id: Uuid = sqlx::query_scalar(
            "INSERT INTO ai_runs (tenant_id, provider_id, source, model, purpose, status, decision, prompt_tokens,
                completion_tokens, latency_ms, error_code, conversation_id)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $11, $10, $12) RETURNING id",
        )
        .bind(t.as_uuid())
        .bind(provider)
        .bind(source.as_str())
        .bind(model.chars().take(120).collect::<String>())
        .bind(if purpose == "test" { "test" } else { "chat" })
        .bind(status)
        .bind(decision_str(o.decision))
        .bind(o.usage.prompt_tokens.min(i32::MAX as u32) as i32)
        .bind(o.usage.completion_tokens.min(i32::MAX as u32) as i32)
        .bind(o.error_code)
        .bind(rc.started.elapsed().as_millis().min(i32::MAX as u128) as i32)
        .bind(rc.conversation_id)
        .fetch_one(&mut *tx)
        .await?;
        for c in &o.tool_calls {
            sqlx::query("INSERT INTO ai_tool_calls (tenant_id, ai_run_id, tool_name, result_status, args_summary) VALUES ($1, $2, $3, $4, $5)")
                .bind(t.as_uuid())
                .bind(id)
                .bind(&c.name)
                .bind(c.status)
                .bind(if c.summary.to_string().len() <= 2000 { c.summary.clone() } else { json!({}) })
                .execute(&mut *tx)
                .await?;
        }
        tx.commit().await?;
        Ok(id)
    }
}

/// Dipakai hanya pada jalur Human Only (orchestrator tidak memanggil provider/backend).
struct Unreachable;
impl LlmPort for Unreachable {
    fn capabilities(&self) -> umkm_domain::ai::Capabilities {
        umkm_domain::ai::Capabilities::default()
    }
    async fn chat(
        &self,
        _r: umkm_application::ai::llm::ChatRequest,
    ) -> Result<umkm_application::ai::llm::ChatResponse, LlmError> {
        Err(LlmError::Unsupported)
    }
    async fn embed(&self, _m: &str, _i: Vec<String>) -> Result<Vec<Vec<f32>>, LlmError> {
        Err(LlmError::Unsupported)
    }
}

struct NoBackend;
impl umkm_application::ai::tools::ToolBackend for NoBackend {
    async fn execute(
        &self,
        _a: &umkm_application::ai::tools::ToolArgs,
        _c: Option<umkm_application::ai::tools::CustomerCtx>,
    ) -> Result<serde_json::Value, umkm_application::ai::tools::ToolError> {
        Err(umkm_application::ai::tools::ToolError::Failed)
    }
}
