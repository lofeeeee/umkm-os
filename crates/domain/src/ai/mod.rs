//! AI platform (P5, `PRD.md §4.10–4.11`, ADR-0011). Murni: konfigurasi chatbot + batasnya
//! (AC-AIC-001), sumber provider platform/BYOK, capability registry, daftar 10 tool.

pub mod consent;
pub mod knowledge;
pub mod mode;

pub use mode::{AiMode, Decision, Outcome};

/// Batas parameter (AC-AIC-001). Nilai di luar batas ditolak, tidak di-clamp.
pub const TEMPERATURE_MAX: f64 = 2.0;
pub const MAX_OUTPUT_TOKENS: (u32, u32) = (16, 4096);
pub const TIMEOUT_MS: (u32, u32) = (1_000, 60_000);
pub const SYSTEM_PROMPT_MAX: usize = 4_000;
pub const MESSAGE_MAX: usize = 500;
pub const TOPICS_MAX: usize = 30;
pub const TOPIC_MAX_LEN: usize = 60;
pub const MODEL_MAX_LEN: usize = 120;
pub const PROVIDER_NAME_MAX: usize = 60;

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProviderSource {
    /// Provider milik platform (kunci & URL tidak pernah terlihat tenant; kuota token).
    Platform,
    /// Endpoint OpenAI-compatible + kunci milik tenant (SSRF guard, enkripsi envelope).
    Byok,
}

impl ProviderSource {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Platform => "platform",
            Self::Byok => "byok",
        }
    }
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "platform" => Some(Self::Platform),
            "byok" => Some(Self::Byok),
            _ => None,
        }
    }
}

/// Jenis provider target (`PRD.md §4.10`). Ollama/vLLM internal hanya lewat provider platform.
pub const PROVIDER_TYPES: [&str; 5] = ["openai", "openrouter", "groq", "together", "custom"];

/// Capability registry (`PRD.md §8.1`). Fitur tanpa capability tidak dikirim ke provider (AC-AIC-002).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct Capabilities {
    pub chat: bool,
    pub streaming: bool,
    pub tool_calling: bool,
    pub usage_reporting: bool,
    pub embeddings: bool,
}

impl Capabilities {
    /// Default konservatif per jenis provider; tenant BYOK `custom` mulai dari chat saja.
    pub fn defaults_for(provider_type: &str) -> Self {
        match provider_type {
            "openai" | "openrouter" | "together" => Self {
                chat: true,
                streaming: true,
                tool_calling: true,
                usage_reporting: true,
                embeddings: true,
            },
            "groq" => Self {
                chat: true,
                streaming: true,
                tool_calling: true,
                usage_reporting: true,
                embeddings: false,
            },
            _ => Self {
                chat: true,
                ..Self::default()
            },
        }
    }
}

/// 10 tool awal (`PRD.md §4.11`) dan level izinnya.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Tool {
    SearchProducts,
    GetProductDetail,
    CheckStock,
    GetBusinessHours,
    GetBusinessLocation,
    SearchFaq,
    GetCustomerOrders,
    GetOrderStatus,
    CreateOrderDraft,
    RequestHumanSupport,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolLevel {
    Read,
    /// Hanya data milik pelanggan yang terverifikasi dalam percakapan.
    ScopedRead,
    /// Menulis, dibatasi: tidak pernah membayar/merefund/mengubah data admin.
    RestrictedWrite,
}

pub const ALL_TOOLS: [Tool; 10] = [
    Tool::SearchProducts,
    Tool::GetProductDetail,
    Tool::CheckStock,
    Tool::GetBusinessHours,
    Tool::GetBusinessLocation,
    Tool::SearchFaq,
    Tool::GetCustomerOrders,
    Tool::GetOrderStatus,
    Tool::CreateOrderDraft,
    Tool::RequestHumanSupport,
];

impl Tool {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::SearchProducts => "search_products",
            Self::GetProductDetail => "get_product_detail",
            Self::CheckStock => "check_stock",
            Self::GetBusinessHours => "get_business_hours",
            Self::GetBusinessLocation => "get_business_location",
            Self::SearchFaq => "search_faq",
            Self::GetCustomerOrders => "get_customer_orders",
            Self::GetOrderStatus => "get_order_status",
            Self::CreateOrderDraft => "create_order_draft",
            Self::RequestHumanSupport => "request_human_support",
        }
    }
    pub fn parse(s: &str) -> Option<Self> {
        ALL_TOOLS.into_iter().find(|t| t.as_str() == s)
    }
    pub fn level(self) -> ToolLevel {
        match self {
            Self::GetCustomerOrders | Self::GetOrderStatus => ToolLevel::ScopedRead,
            Self::CreateOrderDraft | Self::RequestHumanSupport => ToolLevel::RestrictedWrite,
            _ => ToolLevel::Read,
        }
    }
    /// Default aktif: tool baca + handover. Tool tulis lain harus diaktifkan eksplisit.
    pub fn enabled_by_default(self) -> bool {
        self != Self::CreateOrderDraft
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ConfigError {
    #[error("{0} di luar batas")]
    OutOfRange(&'static str),
    #[error("{0} terlalu panjang")]
    TooLong(&'static str),
    #[error("{0} tidak valid")]
    Invalid(&'static str),
}

/// Parameter generatif yang dapat diatur tenant (bagian numerik dari 16 parameter §4.10).
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct GenerationParams {
    pub temperature: f64,
    pub max_output_tokens: u32,
    pub timeout_ms: u32,
}

impl Default for GenerationParams {
    fn default() -> Self {
        Self {
            temperature: 0.3,
            max_output_tokens: 512,
            timeout_ms: 20_000,
        }
    }
}

impl GenerationParams {
    pub fn validate(&self) -> Result<(), ConfigError> {
        if !self.temperature.is_finite() || !(0.0..=TEMPERATURE_MAX).contains(&self.temperature) {
            return Err(ConfigError::OutOfRange("temperature"));
        }
        if !(MAX_OUTPUT_TOKENS.0..=MAX_OUTPUT_TOKENS.1).contains(&self.max_output_tokens) {
            return Err(ConfigError::OutOfRange("max_output_tokens"));
        }
        if !(TIMEOUT_MS.0..=TIMEOUT_MS.1).contains(&self.timeout_ms) {
            return Err(ConfigError::OutOfRange("timeout_ms"));
        }
        Ok(())
    }
}

/// Validasi teks konfigurasi (prompt, pesan, topik). Teks tenant tetap diperlakukan sebagai data
/// di prompt; batas panjang mencegah konsumsi token berlebih.
pub fn validate_texts(
    system_prompt: &str,
    welcome: &str,
    fallback: &str,
    topics: &[String],
) -> Result<(), ConfigError> {
    if system_prompt.chars().count() > SYSTEM_PROMPT_MAX {
        return Err(ConfigError::TooLong("system_prompt"));
    }
    if welcome.chars().count() > MESSAGE_MAX {
        return Err(ConfigError::TooLong("welcome_message"));
    }
    if fallback.trim().is_empty() {
        return Err(ConfigError::Invalid("fallback_message"));
    }
    if fallback.chars().count() > MESSAGE_MAX {
        return Err(ConfigError::TooLong("fallback_message"));
    }
    if topics.len() > TOPICS_MAX {
        return Err(ConfigError::OutOfRange("topics"));
    }
    if topics
        .iter()
        .any(|t| t.trim().is_empty() || t.chars().count() > TOPIC_MAX_LEN)
    {
        return Err(ConfigError::Invalid("topics"));
    }
    Ok(())
}

pub fn validate_model(model: &str) -> Result<(), ConfigError> {
    let ok = !model.is_empty()
        && model.len() <= MODEL_MAX_LEN
        && model
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || "-_./:@".contains(c));
    if ok {
        Ok(())
    } else {
        Err(ConfigError::Invalid("model"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generation_params_bounds() {
        assert!(GenerationParams::default().validate().is_ok());
        let p = |t: f64, m: u32, ms: u32| {
            GenerationParams {
                temperature: t,
                max_output_tokens: m,
                timeout_ms: ms,
            }
            .validate()
        };
        assert!(p(0.0, 16, 1000).is_ok() && p(2.0, 4096, 60_000).is_ok());
        assert_eq!(
            p(2.01, 512, 20_000),
            Err(ConfigError::OutOfRange("temperature"))
        );
        assert_eq!(
            p(-0.1, 512, 20_000),
            Err(ConfigError::OutOfRange("temperature"))
        );
        assert_eq!(
            p(f64::NAN, 512, 20_000),
            Err(ConfigError::OutOfRange("temperature"))
        );
        assert_eq!(
            p(0.3, 15, 20_000),
            Err(ConfigError::OutOfRange("max_output_tokens"))
        );
        assert_eq!(
            p(0.3, 4097, 20_000),
            Err(ConfigError::OutOfRange("max_output_tokens"))
        );
        assert_eq!(p(0.3, 512, 999), Err(ConfigError::OutOfRange("timeout_ms")));
        assert_eq!(
            p(0.3, 512, 60_001),
            Err(ConfigError::OutOfRange("timeout_ms"))
        );
    }

    #[test]
    fn texts_and_model() {
        let t = |n: usize| "a".repeat(n);
        assert!(validate_texts(&t(4000), &t(500), "Maaf", &[]).is_ok());
        assert!(validate_texts(&t(4001), "", "Maaf", &[]).is_err());
        assert!(validate_texts("", "", "  ", &[]).is_err());
        assert!(validate_texts("", "", "Maaf", &vec!["x".into(); 31]).is_err());
        assert!(validate_texts("", "", "Maaf", &["  ".into()]).is_err());
        assert!(
            validate_model("gpt-4o-mini").is_ok()
                && validate_model("meta-llama/Llama-3.1-8B:free").is_ok()
        );
        assert!(
            validate_model("").is_err()
                && validate_model("a b").is_err()
                && validate_model(&t(121)).is_err()
        );
    }

    #[test]
    fn tools_registry() {
        assert_eq!(ALL_TOOLS.len(), 10);
        for t in ALL_TOOLS {
            assert_eq!(Tool::parse(t.as_str()), Some(t));
        }
        assert_eq!(Tool::parse("run_sql"), None);
        assert_eq!(Tool::CreateOrderDraft.level(), ToolLevel::RestrictedWrite);
        assert_eq!(Tool::GetOrderStatus.level(), ToolLevel::ScopedRead);
        assert!(
            !Tool::CreateOrderDraft.enabled_by_default()
                && Tool::RequestHumanSupport.enabled_by_default()
        );
    }

    #[test]
    fn capabilities_and_sources() {
        assert!(!Capabilities::defaults_for("custom").tool_calling);
        assert!(Capabilities::defaults_for("openai").tool_calling);
        assert_eq!(ProviderSource::parse("byok"), Some(ProviderSource::Byok));
        assert_eq!(ProviderSource::parse("x"), None);
    }
}
