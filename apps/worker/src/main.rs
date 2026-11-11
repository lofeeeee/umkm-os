//! umkm-worker — kerangka proses background (P0-6).
//!
//! Belum ada job terdaftar: outbox/email masuk P1-6, purge P1-9. Proses tetap
//! berjalan sampai SIGTERM agar dapat dideploy, dipantau, dan dimatikan bersih.

use std::{net::SocketAddr, path::PathBuf, sync::Arc};

use axum::{Router, http::header, routing::get};
use tracing_subscriber::{EnvFilter, fmt};
use umkm_infrastructure::metrics::{CONTENT_TYPE, Metrics};
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

    // /metrics internal (P0-8): worker_* diisi saat job nyata ada (P1-6).
    let metrics = Arc::new(Metrics::new());
    let addr: SocketAddr =
        match format!("{}:{}", settings.metrics.host, settings.metrics.port).parse() {
            Ok(a) => a,
            Err(e) => {
                tracing::error!(error = %e, "metrics address tidak valid");
                return std::process::ExitCode::FAILURE;
            }
        };
    let listener = match tokio::net::TcpListener::bind(addr).await {
        Ok(l) => l,
        Err(e) => {
            tracing::error!(error = %e, %addr, "gagal bind metrics");
            return std::process::ExitCode::FAILURE;
        }
    };
    let app = Router::new().route(
        "/metrics",
        get(move || {
            let metrics = metrics.clone();
            async move { ([(header::CONTENT_TYPE, CONTENT_TYPE)], metrics.encode()) }
        }),
    );
    tracing::info!(%addr, "worker metrics listening");
    if let Err(e) = axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await
    {
        tracing::error!(error = %e, "metrics server error");
        return std::process::ExitCode::FAILURE;
    }
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
