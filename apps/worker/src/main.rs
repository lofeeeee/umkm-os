//! umkm-worker — kerangka proses background (P0-6).
//!
//! Belum ada job terdaftar: outbox/email masuk P1-6, purge P1-9. Proses tetap
//! berjalan sampai SIGTERM agar dapat dideploy, dipantau, dan dimatikan bersih.

use std::path::PathBuf;

use tracing_subscriber::{EnvFilter, fmt};
use umkm_shared::config::{LogFormat, Settings};

#[tokio::main]
async fn main() -> std::process::ExitCode {
    let config_dir = std::env::var_os("UMKM_CONFIG_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("config"));
    let settings = match Settings::load_from_env(&config_dir) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("umkm-worker: {e}");
            return std::process::ExitCode::FAILURE;
        }
    };
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    match settings.observability.log_format {
        LogFormat::Json => fmt()
            .with_env_filter(filter)
            .json()
            .flatten_event(true)
            .init(),
        LogFormat::Pretty => fmt().with_env_filter(filter).init(),
    }
    tracing::info!(
        environment = %settings.environment,
        layers = ?[umkm_application::CRATE, umkm_infrastructure::CRATE, umkm_database::CRATE],
        "umkm-worker started; no jobs registered yet"
    );
    shutdown_signal().await;
    tracing::info!("umkm-worker stopped");
    std::process::ExitCode::SUCCESS
}

async fn shutdown_signal() {
    let ctrl_c = async {
        let _ = tokio::signal::ctrl_c().await;
    };
    #[cfg(unix)]
    let terminate = async {
        if let Ok(mut s) = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
        {
            s.recv().await;
        }
    };
    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();
    tokio::select! { _ = ctrl_c => {}, _ = terminate => {} }
}
