use std::{future::Future, pin::Pin, sync::Arc};

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
}

impl AppState {
    pub fn new(readiness: Vec<Arc<dyn ReadinessCheck>>) -> Self {
        Self {
            readiness: Arc::new(readiness),
        }
    }
}
