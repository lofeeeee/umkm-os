//! Port LLM yang dinormalkan (P5-4, `PRD.md §8.1`). Adapter (infrastructure) menerjemahkan dari/ke
//! format provider; orchestrator hanya melihat tipe ini. Semua output provider = untrusted input.

use std::future::Future;

use umkm_domain::ai::Capabilities;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Role {
    System,
    User,
    Assistant,
    Tool,
}

impl Role {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::System => "system",
            Self::User => "user",
            Self::Assistant => "assistant",
            Self::Tool => "tool",
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct ChatMessage {
    pub role: Role,
    pub content: String,
    /// Untuk `Role::Tool`: id panggilan yang dijawab.
    pub tool_call_id: Option<String>,
    /// Untuk `Role::Assistant`: panggilan tool yang diminta model pada giliran itu.
    pub tool_calls: Vec<ToolCall>,
}

impl ChatMessage {
    pub fn new(role: Role, content: impl Into<String>) -> Self {
        Self {
            role,
            content: content.into(),
            tool_call_id: None,
            tool_calls: Vec::new(),
        }
    }
}

/// Definisi tool yang ditawarkan ke model (skema JSON parameter).
#[derive(Debug, Clone, PartialEq)]
pub struct ToolSpec {
    pub name: &'static str,
    pub description: &'static str,
    pub parameters: serde_json::Value,
}

/// Panggilan tool dari model. `name`/`arguments` belum tervalidasi (untrusted).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolCall {
    pub id: String,
    pub name: String,
    pub arguments: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ChatRequest {
    pub model: String,
    pub messages: Vec<ChatMessage>,
    pub temperature: f64,
    pub max_output_tokens: u32,
    /// Ditawarkan HANYA bila provider punya capability tool_calling (AC-AIC-002); adapter menegakkan.
    pub tools: Vec<ToolSpec>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Usage {
    pub prompt_tokens: u32,
    pub completion_tokens: u32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ChatResponse {
    pub content: Option<String>,
    pub tool_calls: Vec<ToolCall>,
    /// `None` bila provider tidak melaporkan (usage_reporting = false) → metering memakai estimasi.
    pub usage: Option<Usage>,
    pub model: String,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum LlmError {
    #[error("provider tidak mendukung fitur ini")]
    Unsupported,
    #[error("kredensial provider ditolak")]
    Unauthorized,
    #[error("provider membatasi laju")]
    RateLimited,
    #[error("permintaan ditolak provider")]
    BadRequest,
    #[error("provider bermasalah")]
    Upstream,
    #[error("respons provider tidak valid")]
    InvalidResponse,
    #[error("batas waktu provider habis")]
    Timeout,
    /// Tujuan diblokir SSRF guard / koneksi gagal. `code` = kode stabil tanpa detail tujuan.
    #[error("jaringan: {0}")]
    Network(&'static str),
}

impl LlmError {
    /// Kode stabil untuk `ai_runs.error_code`.
    pub fn code(&self) -> &'static str {
        match self {
            Self::Unsupported => "unsupported",
            Self::Unauthorized => "unauthorized",
            Self::RateLimited => "rate_limited",
            Self::BadRequest => "bad_request",
            Self::Upstream => "upstream",
            Self::InvalidResponse => "invalid_response",
            Self::Timeout => "timeout",
            Self::Network(c) => c,
        }
    }
    /// Gagal yang layak dicoba di provider fallback / dihitung circuit breaker.
    pub fn is_transient(&self) -> bool {
        matches!(
            self,
            Self::RateLimited | Self::Upstream | Self::Timeout | Self::Network(_)
        )
    }
}

pub trait LlmPort: Send + Sync {
    fn capabilities(&self) -> Capabilities;
    fn chat(&self, req: ChatRequest)
    -> impl Future<Output = Result<ChatResponse, LlmError>> + Send;
    fn embed(
        &self,
        model: &str,
        inputs: Vec<String>,
    ) -> impl Future<Output = Result<Vec<Vec<f32>>, LlmError>> + Send;
}
