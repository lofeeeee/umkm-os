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
}

impl AppState {
    pub fn new(readiness: Vec<Arc<dyn ReadinessCheck>>) -> Self {
        Self {
            readiness: Arc::new(readiness),
            metrics: Arc::default(),
            auth: None,
        }
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
