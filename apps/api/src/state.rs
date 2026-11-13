use std::{future::Future, pin::Pin, sync::Arc};

use umkm_infrastructure::metrics::Metrics;

use crate::auth::AuthService;

pub type CheckFuture<'a> = Pin<Box<dyn Future<Output = Result<(), String>> + Send + 'a>>;

/// Dependensi yang harus siap sebelum instance menerima trafik (`/readyz`).
/// Pesan error hanya masuk log, tidak ke response (`PRD.md §6.4`).
pub trait ReadinessCheck: Send + Sync {
    fn name(&self) -> &'static str;
    fn check(&self) -> CheckFuture<'_>;
}

#[derive(Clone, Default)]
pub struct AppState {
    pub readiness: Arc<Vec<Arc<dyn ReadinessCheck>>>,
    pub metrics: Arc<Metrics>,
    /// `None` = endpoint auth menjawab 503 (mis. router uji tanpa DB/Redis).
    pub auth: Option<Arc<AuthService>>,
    /// Pool untuk extractor yang butuh DB (mis. `HostTenant`). `None` -> 503.
    pub db: Option<sqlx::PgPool>,
    /// Hub SSE per tenant (P1-10).
    pub realtime: crate::realtime::RealtimeHub,
    /// Object storage + signed URL (P2-6, ADR-0009). `None` -> endpoint file 503.
    pub files: Option<Arc<FileService>>,
    /// Fan-out realtime lintas instance (P4-6). `None` = hub lokal saja.
    pub rt_publisher: Option<umkm_infrastructure::realtime::RtPublisher>,
    /// AI (P5, ADR-0011): master key + provider platform. `None` -> endpoint AI 503.
    pub ai: Option<Arc<AiService>>,
    /// Instruksi pembayaran langganan manual (P7-3, ADR-0016). Kosong = belum diatur operator.
    pub billing_instructions: Arc<str>,
}

/// Rahasia & konfigurasi AI tingkat proses.
pub struct AiService {
    pub keyring: umkm_infrastructure::crypto::Keyring,
    /// `Some` hanya bila provider platform diaktifkan operator.
    pub platform: Option<umkm_shared::config::PlatformAiSettings>,
    pub platform_key: Option<umkm_shared::config::SecretString>,
    pub runtime: Arc<umkm_infrastructure::ai::runtime::AiRuntime>,
}

impl AiService {
    pub fn new(
        pool: sqlx::PgPool,
        keyring: umkm_infrastructure::crypto::Keyring,
        platform: Option<umkm_shared::config::PlatformAiSettings>,
        platform_key: Option<umkm_shared::config::SecretString>,
        max_concurrency: usize,
    ) -> Self {
        let runtime = Arc::new(umkm_infrastructure::ai::runtime::AiRuntime::new(
            pool,
            keyring.clone(),
            platform.clone(),
            platform_key.clone(),
            max_concurrency,
        ));
        Self {
            keyring,
            platform,
            platform_key,
            runtime,
        }
    }
}

/// Storage lokal + penanda URL (ADR-0009).
pub struct FileService {
    pub store: umkm_infrastructure::storage::PgObjectStorage,
    pub signer: umkm_infrastructure::storage::UrlSigner,
}

impl AppState {
    pub fn new(readiness: Vec<Arc<dyn ReadinessCheck>>) -> Self {
        Self {
            readiness: Arc::new(readiness),
            metrics: Arc::default(),
            auth: None,
            db: None,
            realtime: Default::default(),
            files: None,
            rt_publisher: None,
            ai: None,
            billing_instructions: Arc::from(""),
        }
    }

    pub fn with_files(mut self, files: Arc<FileService>) -> Self {
        self.files = Some(files);
        self
    }

    pub fn with_rt_publisher(mut self, p: umkm_infrastructure::realtime::RtPublisher) -> Self {
        self.rt_publisher = Some(p);
        self
    }

    pub fn with_billing_instructions(mut self, text: &str) -> Self {
        self.billing_instructions = Arc::from(text);
        self
    }

    pub fn with_ai(mut self, ai: Arc<AiService>) -> Self {
        self.ai = Some(ai);
        self
    }

    pub fn with_db(mut self, pool: sqlx::PgPool) -> Self {
        self.db = Some(pool);
        self
    }

    pub fn with_auth(mut self, auth: Arc<AuthService>) -> Self {
        self.auth = Some(auth);
        self
    }

    pub fn with_metrics(mut self, metrics: Arc<Metrics>) -> Self {
        self.metrics = metrics;
        self
    }
}
