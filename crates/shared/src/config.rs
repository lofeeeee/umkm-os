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
    pub database: DatabaseSettings,
    pub redis: RedisSettings,
    pub tenancy: TenancySettings,
    pub observability: ObservabilitySettings,
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
        Self::load(config_dir, env.parse()?)
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
        if self.database.max_connections == 0 {
            return invalid("database.max_connections harus > 0");
        }
        if self.database.acquire_timeout_secs == 0 {
            return invalid("database.acquire_timeout_secs harus > 0");
        }
        if self.redis.key_prefix.trim().is_empty() {
            return invalid("redis.key_prefix wajib diisi");
        }
        if self.tenancy.root_domain.trim().is_empty() {
            return invalid("tenancy.root_domain wajib diisi");
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
        Ok(Self {
            database_url: get(DATABASE_URL_VAR)?,
            redis_url: get(REDIS_URL_VAR)?,
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
[database]
max_connections = 20
acquire_timeout_secs = 5
[redis]
key_prefix = "umkm:production"
[tenancy]
root_domain = "example.test"
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
            _ => None,
        })
        .unwrap();
        let dbg = format!("{s:?}");
        assert!(
            !dbg.contains("hunter2") && !dbg.contains("topsecret"),
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
    }
}
