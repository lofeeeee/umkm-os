//! Konfigurasi aplikasi (P0-2, `PRD.md §14.1`).
//!
//! - Nilai non-rahasia dibaca dari `config/<env>.toml`.
//! - Secret hanya dari environment variable runtime (`PRD.md §6.7`), dibungkus
//!   [`SecretString`] yang tidak pernah tercetak di `Debug`.
//! - Environment wajib dipilih eksplisit lewat `UMKM_ENV`; tidak ada default,
//!   agar production tidak pernah diam-diam berjalan dengan config development.

use std::{fmt, path::Path, str::FromStr};

use serde::Deserialize;

/// Placeholder root domain sampai D-01 `DISETUJUI` (`OPEN_DECISIONS.md`).
pub const ROOT_DOMAIN_PLACEHOLDER: &str = "<ROOT_DOMAIN>";

pub const ENV_VAR: &str = "UMKM_ENV";
pub const DATABASE_URL_VAR: &str = "UMKM_DATABASE_URL";
pub const REDIS_URL_VAR: &str = "UMKM_REDIS_URL";
/// Kunci HMAC signed URL file (ADR-0009); minimal 32 byte.
pub const FILE_SIGNING_KEY_VAR: &str = "UMKM_FILE_SIGNING_KEY";
pub const FILE_SIGNING_KEY_MIN_LEN: usize = 32;
/// Master key envelope encryption (P5-2, ADR-0011): `v1:<base64 32 byte>[,v2:...]`.
pub const DATA_KEK_VAR: &str = "UMKM_DATA_KEK";
/// Kunci provider AI platform (ADR-0011); opsional (endpoint internal tanpa kunci).
pub const PLATFORM_AI_KEY_VAR: &str = "UMKM_PLATFORM_AI_API_KEY";
/// Override `tenancy.root_domain` (Compose meneruskan nilai yang sama dengan Caddy).
pub const ROOT_DOMAIN_VAR: &str = "UMKM_ROOT_DOMAIN";

#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("environment variable {0} wajib diisi")]
    MissingVar(&'static str),
    #[error("{ENV_VAR} tidak valid: {0:?} (harus development|staging|production)")]
    InvalidEnvironment(String),
    #[error("gagal membaca {path}: {source}")]
    Read {
        path: String,
        source: std::io::Error,
    },
    #[error("format {path} tidak valid: {source}")]
    Parse {
        path: String,
        source: Box<toml::de::Error>,
    },
    #[error("{ENV_VAR}={expected} tetapi file berisi environment = {found}")]
    EnvironmentMismatch {
        expected: Environment,
        found: Environment,
    },
    #[error("konfigurasi tidak valid: {0}")]
    Invalid(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Environment {
    Development,
    Staging,
    Production,
}

impl Environment {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Development => "development",
            Self::Staging => "staging",
            Self::Production => "production",
        }
    }
}

impl fmt::Display for Environment {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for Environment {
    type Err = ConfigError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "development" => Ok(Self::Development),
            "staging" => Ok(Self::Staging),
            "production" => Ok(Self::Production),
            other => Err(ConfigError::InvalidEnvironment(other.to_owned())),
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Settings {
    pub environment: Environment,
    pub server: ServerSettings,
    pub http: HttpSettings,
    pub legal: LegalSettings,
    pub worker: WorkerSettings,
    pub retention: RetentionSettings,
    pub metrics: MetricsSettings,
    pub database: DatabaseSettings,
    pub redis: RedisSettings,
    pub tenancy: TenancySettings,
    pub storage: StorageSettings,
    pub observability: ObservabilitySettings,
    /// P5 AI (ADR-0011). Tanpa section = AI platform nonaktif.
    #[serde(default)]
    pub ai: AiSettings,
    /// P7-3 (ADR-0016): instruksi pembayaran langganan manual. Tanpa section = belum diatur.
    #[serde(default)]
    pub billing: BillingSettings,
}

/// Pembayaran langganan manual (ADR-0016). Bukan rahasia: rekening/QRIS tujuan ditampilkan ke pemilik toko.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct BillingSettings {
    /// Teks instruksi (rekening/QRIS platform). Kosong = "hubungi admin platform".
    pub payment_instructions: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ServerSettings {
    pub host: String,
    pub port: u16,
    pub request_timeout_secs: u64,
    pub body_limit_bytes: usize,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HttpSettings {
    pub cors_allowed_origins: Vec<String>,
    pub trust_forwarded_for: bool,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LegalSettings {
    pub privacy_policy_version: String,
    pub terms_of_service_version: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WorkerSettings {
    pub poll_interval_ms: u64,
    pub batch_size: i64,
    pub lease_secs: u64,
    pub max_attempts: i32,
    pub base_backoff_secs: u64,
    pub verify_url_base: String,
}

/// Retensi ADR-0012 (hari).
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RetentionSettings {
    pub audit_days: u32,
    pub closed_tenant_grace_days: u32,
    pub conversation_days: u32,
    pub file_grace_days: u32,
    pub customer_data_grace_days: u32,
    pub auth_artifact_days: u32,
    pub outbox_days: u32,
    pub purge_interval_secs: u64,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MetricsSettings {
    pub host: String,
    pub port: u16,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DatabaseSettings {
    pub max_connections: u32,
    pub acquire_timeout_secs: u64,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RedisSettings {
    pub key_prefix: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TenancySettings {
    pub root_domain: String,
}

/// Object storage lokal (ADR-0009). Kuota sementara sampai D-10 (plan limits).
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StorageSettings {
    pub max_object_bytes: usize,
    pub max_objects_per_tenant: i64,
    /// Masa berlaku signed URL; ADR-0009: maksimal 3600.
    pub signed_url_ttl_secs: u64,
}

/// AI (ADR-0011): batas klien keluar + provider platform yang didaftarkan operator.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct AiSettings {
    /// Permintaan bersamaan maksimum ke provider per proses (SSRF guard kontrol 7).
    pub max_concurrency: usize,
    pub platform: PlatformAiSettings,
}

impl Default for AiSettings {
    fn default() -> Self {
        Self {
            max_concurrency: 8,
            platform: PlatformAiSettings::default(),
        }
    }
}

/// Provider AI milik platform. Kunci di secret `UMKM_PLATFORM_AI_API_KEY` (opsional untuk endpoint internal).
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct PlatformAiSettings {
    pub enabled: bool,
    /// Endpoint OpenAI-compatible, mis. `https://api.openai.com/v1` atau vLLM internal.
    pub base_url: String,
    /// Model yang boleh dipilih tenant; elemen pertama = default.
    pub models: Vec<String>,
    pub embedding_model: String,
    pub tool_calling: bool,
    pub embeddings: bool,
    /// Kuota token bulanan per tenant (prompt + completion). Angka final mengikuti D-10.
    pub monthly_quota: u64,
    /// Endpoint jaringan internal tepercaya (http/IP privat; `PRD.md §6.5` #9). Hanya untuk operator.
    pub trusted_internal: bool,
    /// AC-SUB-002: harga provider platform (Rupiah per 1 juta token) untuk estimasi biaya per panggilan.
    /// 0 = belum diketahui → estimasi tidak disimpan (NULL), bukan dianggap gratis.
    pub input_cost_per_million_idr: f64,
    pub output_cost_per_million_idr: f64,
}

impl Default for PlatformAiSettings {
    fn default() -> Self {
        Self {
            enabled: false,
            base_url: String::new(),
            models: vec![],
            embedding_model: String::new(),
            tool_calling: true,
            embeddings: true,
            monthly_quota: 200_000,
            trusted_internal: false,
            input_cost_per_million_idr: 0.0,
            output_cost_per_million_idr: 0.0,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LogFormat {
    Json,
    Pretty,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ObservabilitySettings {
    pub log_format: LogFormat,
}

impl Settings {
    /// Baca environment dari `UMKM_ENV`, lalu muat `<config_dir>/<env>.toml`.
    pub fn load_from_env(config_dir: &Path) -> Result<Self, ConfigError> {
        let env = std::env::var(ENV_VAR).map_err(|_| ConfigError::MissingVar(ENV_VAR))?;
        let mut settings = Self::load(config_dir, env.parse()?)?;
        if let Some(domain) = std::env::var(ROOT_DOMAIN_VAR)
            .ok()
            .filter(|d| !d.trim().is_empty())
        {
            settings.tenancy.root_domain = domain.trim().to_lowercase();
            settings.validate()?;
        }
        Ok(settings)
    }

    /// URL verifikasi email dengan `<ROOT_DOMAIN>` diganti root domain aktif.
    pub fn verify_url_base(&self) -> String {
        self.worker
            .verify_url_base
            .replace(ROOT_DOMAIN_PLACEHOLDER, &self.tenancy.root_domain)
    }

    /// Root domain siap dipakai untuk membuat hostname tenant (bukan placeholder).
    pub fn root_domain_ready(&self) -> bool {
        self.tenancy.root_domain != ROOT_DOMAIN_PLACEHOLDER
    }

    pub fn load(config_dir: &Path, env: Environment) -> Result<Self, ConfigError> {
        let path = config_dir.join(format!("{env}.toml"));
        let display = path.display().to_string();
        let raw = std::fs::read_to_string(&path).map_err(|source| ConfigError::Read {
            path: display.clone(),
            source,
        })?;
        let settings = Self::parse(&raw).map_err(|source| ConfigError::Parse {
            path: display,
            source: Box::new(source),
        })?;
        if settings.environment != env {
            return Err(ConfigError::EnvironmentMismatch {
                expected: env,
                found: settings.environment,
            });
        }
        settings.validate()?;
        Ok(settings)
    }

    pub fn parse(raw: &str) -> Result<Self, toml::de::Error> {
        toml::from_str(raw)
    }

    pub fn validate(&self) -> Result<(), ConfigError> {
        let invalid = |m: &str| Err(ConfigError::Invalid(m.to_owned()));
        if self.server.port == 0 {
            return invalid("server.port harus > 0");
        }
        if self.server.request_timeout_secs == 0 {
            return invalid("server.request_timeout_secs harus > 0");
        }
        if self.server.body_limit_bytes == 0 {
            return invalid("server.body_limit_bytes harus > 0");
        }
        for o in &self.http.cors_allowed_origins {
            let scheme_ok = o.starts_with("https://")
                || (self.environment == Environment::Development && o.starts_with("http://"));
            if !scheme_ok || o.contains('*') || o.ends_with('/') || o.matches('/').count() != 2 {
                return Err(ConfigError::Invalid(format!(
                    "http.cors_allowed_origins tidak valid: {o:?} (origin persis, https, tanpa wildcard/path)"
                )));
            }
        }
        if self.legal.privacy_policy_version.trim().is_empty()
            || self.legal.terms_of_service_version.trim().is_empty()
        {
            return invalid("legal.*_version wajib diisi");
        }
        let w = &self.worker;
        if w.poll_interval_ms == 0
            || w.batch_size <= 0
            || w.lease_secs == 0
            || w.max_attempts <= 0
            || w.base_backoff_secs == 0
        {
            return invalid("worker.* harus > 0");
        }
        let url = &w.verify_url_base;
        if !(url.starts_with("https://")
            || (self.environment == Environment::Development && url.starts_with("http://")))
        {
            return invalid("worker.verify_url_base harus https (http hanya development)");
        }
        let r = &self.retention;
        if r.audit_days < 90 {
            return invalid("retention.audit_days minimal 90 (ADR-0012: 365)");
        }
        if r.closed_tenant_grace_days < 30
            || r.file_grace_days < 30
            || r.customer_data_grace_days < 30
        {
            return invalid("retention.*_grace_days minimal 30 (ADR-0012)");
        }
        if r.conversation_days == 0
            || r.auth_artifact_days == 0
            || r.outbox_days == 0
            || r.purge_interval_secs < 60
        {
            return invalid("retention: hari >= 1, purge_interval_secs >= 60");
        }
        if self.metrics.port == 0 {
            return invalid("metrics.port harus > 0");
        }
        if self.metrics.port == self.server.port {
            return invalid("metrics.port harus berbeda dari server.port");
        }
        if self.database.max_connections == 0 {
            return invalid("database.max_connections harus > 0");
        }
        if self.database.acquire_timeout_secs == 0 {
            return invalid("database.acquire_timeout_secs harus > 0");
        }
        let st = &self.storage;
        if st.max_object_bytes == 0 || st.max_objects_per_tenant <= 0 {
            return invalid("storage.max_* harus > 0");
        }
        if st.max_object_bytes > self.server.body_limit_bytes {
            return invalid(
                "storage.max_object_bytes tidak boleh melebihi server.body_limit_bytes",
            );
        }
        if st.signed_url_ttl_secs < 60 || st.signed_url_ttl_secs > 3600 {
            return invalid("storage.signed_url_ttl_secs harus 60..=3600 (ADR-0009)");
        }
        if self.redis.key_prefix.trim().is_empty() {
            return invalid("redis.key_prefix wajib diisi");
        }
        let d = &self.tenancy.root_domain;
        if d.trim().is_empty() {
            return invalid("tenancy.root_domain wajib diisi");
        }
        let hostname_ok = d.len() <= 253
            && d.split('.').all(|l| {
                !l.is_empty()
                    && l.len() <= 63
                    && !l.starts_with('-')
                    && !l.ends_with('-')
                    && l.bytes()
                        .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
            });
        if d != ROOT_DOMAIN_PLACEHOLDER && !hostname_ok {
            return invalid("tenancy.root_domain bukan hostname huruf kecil yang valid");
        }
        let ai = &self.ai;
        if ai.max_concurrency == 0 {
            return invalid("ai.max_concurrency harus > 0");
        }
        if ai.platform.enabled {
            let p = &ai.platform;
            let https = p.base_url.starts_with("https://");
            if !(https || (p.trusted_internal && p.base_url.starts_with("http://"))) {
                return invalid(
                    "ai.platform.base_url harus https (http hanya bila trusted_internal)",
                );
            }
            if p.models.is_empty() || p.models.iter().any(|m| m.trim().is_empty()) {
                return invalid("ai.platform.models wajib berisi minimal satu model");
            }
            if p.monthly_quota == 0 {
                return invalid("ai.platform.monthly_quota harus > 0");
            }
            if p.embeddings && p.embedding_model.trim().is_empty() {
                return invalid("ai.platform.embedding_model wajib bila embeddings = true");
            }
        }
        if self.environment == Environment::Production
            && self.tenancy.root_domain == ROOT_DOMAIN_PLACEHOLDER
        {
            return invalid(
                "tenancy.root_domain masih placeholder; production menunggu D-01 DISETUJUI",
            );
        }
        Ok(())
    }
}

/// String rahasia: tidak `Display`, tidak `Serialize`, `Debug` selalu tersamar.
#[derive(Clone, PartialEq, Eq)]
pub struct SecretString(String);

impl SecretString {
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    /// Akses eksplisit; panggil hanya di titik pemakaian (mis. membuka koneksi).
    pub fn expose(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for SecretString {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("SecretString(***)")
    }
}

/// Secret runtime (`PRD.md §6.7`). Tidak pernah dibaca dari file config.
#[derive(Debug, Clone)]
pub struct Secrets {
    pub database_url: SecretString,
    pub redis_url: SecretString,
    pub file_signing_key: SecretString,
    pub data_kek: SecretString,
    pub platform_ai_api_key: Option<SecretString>,
}

impl Secrets {
    pub fn from_env() -> Result<Self, ConfigError> {
        Self::from_lookup(|k| std::env::var(k).ok())
    }

    /// Versi yang dapat diuji tanpa mengubah environment proses.
    pub fn from_lookup(lookup: impl Fn(&str) -> Option<String>) -> Result<Self, ConfigError> {
        let get = |k: &'static str| {
            lookup(k)
                .filter(|v| !v.trim().is_empty())
                .map(SecretString::new)
                .ok_or(ConfigError::MissingVar(k))
        };
        let database_url = get(DATABASE_URL_VAR)?;
        let redis_url = get(REDIS_URL_VAR)?;
        let file_signing_key = get(FILE_SIGNING_KEY_VAR)?;
        if file_signing_key.expose().len() < FILE_SIGNING_KEY_MIN_LEN {
            return Err(ConfigError::Invalid(format!(
                "{FILE_SIGNING_KEY_VAR} minimal {FILE_SIGNING_KEY_MIN_LEN} byte"
            )));
        }
        // Format & panjang divalidasi saat Keyring dibangun (infrastructure::crypto); fail-closed.
        let data_kek = get(DATA_KEK_VAR)?;
        let platform_ai_api_key = lookup(PLATFORM_AI_KEY_VAR)
            .filter(|v| !v.trim().is_empty())
            .map(SecretString::new);
        Ok(Self {
            database_url,
            redis_url,
            file_signing_key,
            data_kek,
            platform_ai_api_key,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const VALID: &str = r#"
environment = "production"
[server]
host = "0.0.0.0"
port = 8080
request_timeout_secs = 30
body_limit_bytes = 1048576
[http]
cors_allowed_origins = ["https://app.example.test"]
trust_forwarded_for = true
[legal]
privacy_policy_version = "v1"
terms_of_service_version = "v1"
[worker]
poll_interval_ms = 1000
batch_size = 20
lease_secs = 60
max_attempts = 8
base_backoff_secs = 30
verify_url_base = "https://app.<ROOT_DOMAIN>/verify-email"
[retention]
audit_days = 365
closed_tenant_grace_days = 30
conversation_days = 30
file_grace_days = 30
customer_data_grace_days = 30
auth_artifact_days = 30
outbox_days = 30
purge_interval_secs = 3600
[metrics]
host = "0.0.0.0"
port = 9090
[database]
max_connections = 20
acquire_timeout_secs = 5
[redis]
key_prefix = "umkm:production"
[tenancy]
root_domain = "example.test"
[storage]
max_object_bytes = 1048576
max_objects_per_tenant = 200
signed_url_ttl_secs = 3600
[observability]
log_format = "json"
"#;

    #[test]
    fn parses_valid_config() {
        let s = Settings::parse(VALID).unwrap();
        s.validate().unwrap();
        assert_eq!(s.environment, Environment::Production);
        assert_eq!(s.observability.log_format, LogFormat::Json);
    }

    #[test]
    fn ai_platform_is_off_by_default_and_validated_when_on() {
        let s = Settings::parse(VALID).unwrap();
        assert!(!s.ai.platform.enabled && s.ai.max_concurrency > 0);
        let on = |body: &str| {
            let s = Settings::parse(&format!("{VALID}\n[ai.platform]\nenabled = true\n{body}"))
                .unwrap();
            s.validate()
        };
        on("base_url = \"https://api.example.test/v1\"\nmodels = [\"m1\"]\nembedding_model = \"e1\"").unwrap();
        for bad in [
            "base_url = \"http://10.0.0.5/v1\"\nmodels = [\"m1\"]\nembedding_model = \"e\"",
            "base_url = \"https://api.example.test/v1\"\nmodels = []\nembedding_model = \"e\"",
            "base_url = \"https://api.example.test/v1\"\nmodels = [\"m\"]\nembedding_model = \"\"",
            "base_url = \"https://api.example.test/v1\"\nmodels = [\"m\"]\nembedding_model = \"e\"\nmonthly_quota = 0",
        ] {
            assert!(on(bad).is_err(), "{bad}");
        }
        // Endpoint internal hanya dengan trusted_internal (kontrol 9).
        on("base_url = \"http://10.0.0.5/v1\"\nmodels = [\"m1\"]\nembedding_model = \"e\"\ntrusted_internal = true").unwrap();
        // Kunci tidak boleh ditulis di file konfigurasi.
        assert!(Settings::parse(&format!("{VALID}\n[ai.platform]\napi_key = \"sk-x\"")).is_err());
    }

    #[test]
    fn rejects_unknown_field_such_as_inline_secret() {
        let raw = VALID.replace("[database]", "[database]\nurl = \"postgres://u:p@h/db\"");
        let err = Settings::parse(&raw).unwrap_err().to_string();
        assert!(err.contains("unknown field"), "{err}");
    }

    #[test]
    fn production_rejects_root_domain_placeholder() {
        let raw = VALID.replace("example.test", ROOT_DOMAIN_PLACEHOLDER);
        let err = Settings::parse(&raw).unwrap().validate().unwrap_err();
        assert!(err.to_string().contains("D-01"), "{err}");
    }

    #[test]
    fn root_domain_must_be_lowercase_hostname() {
        for bad in [
            "Example.test",
            "exa mple.test",
            "-x.test",
            "x..test",
            "x.test/",
        ] {
            let raw = VALID.replace("example.test", bad);
            assert!(Settings::parse(&raw).unwrap().validate().is_err(), "{bad}");
        }
        for ok in ["localhost", "ryasai.my.id", "a-b.example.test"] {
            let raw = VALID.replace(
                "root_domain = \"example.test\"",
                &format!("root_domain = \"{ok}\""),
            );
            Settings::parse(&raw).unwrap().validate().unwrap();
        }
    }

    #[test]
    fn staging_allows_root_domain_placeholder() {
        let raw = VALID
            .replace("\"production\"", "\"staging\"")
            .replace("example.test", ROOT_DOMAIN_PLACEHOLDER);
        Settings::parse(&raw).unwrap().validate().unwrap();
    }

    #[test]
    fn rejects_zero_values() {
        for (from, to) in [
            ("port = 8080", "port = 0"),
            ("request_timeout_secs = 30", "request_timeout_secs = 0"),
            ("port = 9090", "port = 0"),
            ("port = 9090", "port = 8080"),
            ("body_limit_bytes = 1048576", "body_limit_bytes = 0"),
            ("max_connections = 20", "max_connections = 0"),
            ("acquire_timeout_secs = 5", "acquire_timeout_secs = 0"),
            ("key_prefix = \"umkm:production\"", "key_prefix = \" \""),
        ] {
            let s = Settings::parse(&VALID.replace(from, to)).unwrap();
            assert!(s.validate().is_err(), "{to} seharusnya ditolak");
        }
    }

    #[test]
    fn cors_origins_must_be_exact_https_without_wildcard() {
        for bad in [
            "*",
            "https://*.example.test",
            "http://app.example.test",
            "https://app.example.test/",
            "https://app.example.test/x",
            "app.example.test",
        ] {
            let raw = VALID.replace("https://app.example.test\"", &format!("{bad}\""));
            let s = Settings::parse(&raw).unwrap();
            assert!(s.validate().is_err(), "{bad} harus ditolak di production");
        }
        let dev = VALID
            .replace("\"production\"", "\"development\"")
            .replace("https://app.example.test", "http://localhost:3000");
        Settings::parse(&dev).unwrap().validate().unwrap();
    }

    #[test]
    fn verify_url_substitutes_root_domain_and_requires_https() {
        let s = Settings::parse(VALID).unwrap();
        assert_eq!(s.verify_url_base(), "https://app.example.test/verify-email");
        let raw = VALID.replace(
            "https://app.<ROOT_DOMAIN>/verify-email",
            "http://app.example.test/v",
        );
        assert!(Settings::parse(&raw).unwrap().validate().is_err());
        let raw = VALID.replace("batch_size = 20", "batch_size = 0");
        assert!(Settings::parse(&raw).unwrap().validate().is_err());
    }

    #[test]
    fn retention_floors_enforced() {
        for (from, to) in [
            ("audit_days = 365", "audit_days = 30"),
            (
                "closed_tenant_grace_days = 30",
                "closed_tenant_grace_days = 7",
            ),
            ("outbox_days = 30", "outbox_days = 0"),
            ("purge_interval_secs = 3600", "purge_interval_secs = 1"),
        ] {
            assert!(
                Settings::parse(&VALID.replace(from, to))
                    .unwrap()
                    .validate()
                    .is_err(),
                "{to}"
            );
        }
    }

    #[test]
    fn environment_parse_is_strict() {
        assert_eq!(
            "staging".parse::<Environment>().unwrap(),
            Environment::Staging
        );
        assert!("prod".parse::<Environment>().is_err());
        assert!("Production".parse::<Environment>().is_err());
    }

    #[test]
    fn secrets_debug_is_redacted() {
        let s = Secrets::from_lookup(|k| match k {
            DATABASE_URL_VAR => Some("postgres://user:hunter2@db/app".into()),
            REDIS_URL_VAR => Some("redis://:topsecret@redis:6379".into()),
            FILE_SIGNING_KEY_VAR => Some("signing-key-0123456789-abcdefghijkl".into()),
            DATA_KEK_VAR => Some("v1:a2VrLXJhaGFzaWEta2VrLXJhaGFzaWEta2VrLXJhaGE=".into()),
            _ => None,
        })
        .unwrap();
        let dbg = format!("{s:?}");
        assert!(
            !dbg.contains("hunter2")
                && !dbg.contains("topsecret")
                && !dbg.contains("signing-key")
                && !dbg.contains("a2VrLXJh"),
            "{dbg}"
        );
        assert_eq!(s.database_url.expose(), "postgres://user:hunter2@db/app");
    }

    #[test]
    fn missing_or_empty_secret_is_error_without_value() {
        let err =
            Secrets::from_lookup(|k| (k == DATABASE_URL_VAR).then(|| "x".into())).unwrap_err();
        assert!(matches!(err, ConfigError::MissingVar(REDIS_URL_VAR)));
        let err = Secrets::from_lookup(|_| Some("  ".into())).unwrap_err();
        assert!(matches!(err, ConfigError::MissingVar(DATABASE_URL_VAR)));
        let err =
            Secrets::from_lookup(|k| (k != FILE_SIGNING_KEY_VAR).then(|| "x".into())).unwrap_err();
        assert!(matches!(err, ConfigError::MissingVar(FILE_SIGNING_KEY_VAR)));
        let err = Secrets::from_lookup(|k| {
            (k != DATA_KEK_VAR).then(|| {
                if k == FILE_SIGNING_KEY_VAR {
                    "signing-key-0123456789-abcdefghijkl".into()
                } else {
                    "x".into()
                }
            })
        })
        .unwrap_err();
        assert!(matches!(err, ConfigError::MissingVar(DATA_KEK_VAR)));
    }

    #[test]
    fn short_file_signing_key_is_rejected_without_echoing_it() {
        let err = Secrets::from_lookup(|k| {
            Some(if k == FILE_SIGNING_KEY_VAR {
                "pendek".into()
            } else {
                "x".into()
            })
        })
        .unwrap_err();
        assert!(matches!(err, ConfigError::Invalid(_)));
        assert!(!err.to_string().contains("pendek"));
    }
}
