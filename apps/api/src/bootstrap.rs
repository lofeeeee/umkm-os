use std::{net::SocketAddr, path::PathBuf};

use tokio::net::TcpListener;
use tracing_subscriber::{EnvFilter, fmt};
use umkm_shared::config::{LogFormat, Settings};

use crate::{AppState, HttpLimits, build_router};

pub const CONFIG_DIR_VAR: &str = "UMKM_CONFIG_DIR";

pub fn init_tracing(format: LogFormat) {
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    let builder = fmt().with_env_filter(filter);
    match format {
        LogFormat::Json => builder.json().flatten_event(true).init(),
        LogFormat::Pretty => builder.init(),
    }
}

pub async fn run() -> Result<(), Box<dyn std::error::Error>> {
    let config_dir = std::env::var_os(CONFIG_DIR_VAR)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("config"));
    let settings = Settings::load_from_env(&config_dir)?;
    init_tracing(settings.observability.log_format);

    let addr: SocketAddr = format!("{}:{}", settings.server.host, settings.server.port).parse()?;
    let app = build_router(AppState::default(), HttpLimits::from(&settings.server));
    let listener = TcpListener::bind(addr).await?;
    tracing::info!(%addr, environment = %settings.environment, "umkm-api listening");
    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await?;
    tracing::info!("umkm-api stopped");
    Ok(())
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
