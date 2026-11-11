use std::{
    io::{Read, Write},
    net::{SocketAddr, TcpStream},
    path::PathBuf,
    sync::Arc,
    time::Duration,
};

use tokio::net::TcpListener;
use tracing_subscriber::{EnvFilter, fmt};
use umkm_infrastructure::redis::RedisClient;
use umkm_shared::config::{LogFormat, Secrets, Settings};

use crate::{AppState, HttpLimits, build_router, readiness};

pub const CONFIG_DIR_VAR: &str = "UMKM_CONFIG_DIR";

type BoxError = Box<dyn std::error::Error>;

pub fn init_tracing(format: LogFormat) {
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    let builder = fmt().with_env_filter(filter);
    match format {
        LogFormat::Json => builder.json().flatten_event(true).init(),
        LogFormat::Pretty => builder.init(),
    }
}

fn load_settings() -> Result<Settings, BoxError> {
    let config_dir = std::env::var_os(CONFIG_DIR_VAR)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("config"));
    Ok(Settings::load_from_env(&config_dir)?)
}

/// `umkm-api [serve|migrate|healthcheck]`
pub async fn main_with_args(args: &[String]) -> Result<(), BoxError> {
    match args.first().map(String::as_str) {
        None | Some("serve") => serve().await,
        Some("migrate") => migrate().await,
        Some("healthcheck") => healthcheck(),
        Some(other) => {
            Err(format!("perintah tidak dikenal: {other} (serve|migrate|healthcheck)").into())
        }
    }
}

pub async fn serve() -> Result<(), BoxError> {
    let settings = load_settings()?;
    init_tracing(settings.observability.log_format);
    let secrets = Secrets::from_env()?;

    let pool = umkm_database::connect_lazy(&secrets.database_url, &settings.database)?;
    let redis = RedisClient::open(&secrets.redis_url)?;
    let state = AppState::new(vec![
        Arc::new(readiness::Postgres(pool)),
        Arc::new(readiness::Redis(redis)),
    ]);

    let addr: SocketAddr = format!("{}:{}", settings.server.host, settings.server.port).parse()?;
    let app = build_router(state, HttpLimits::from(&settings.server));
    let listener = TcpListener::bind(addr).await?;
    tracing::info!(%addr, environment = %settings.environment, "umkm-api listening");
    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await?;
    tracing::info!("umkm-api stopped");
    Ok(())
}

/// Jalankan migration lalu keluar. Dipakai service one-shot `migrate` di Compose.
pub async fn migrate() -> Result<(), BoxError> {
    let settings = load_settings()?;
    init_tracing(settings.observability.log_format);
    let secrets = Secrets::from_env()?;
    let pool = umkm_database::connect_lazy(&secrets.database_url, &settings.database)?;
    umkm_database::MIGRATOR.run(&pool).await?;
    tracing::info!("migrations applied");
    Ok(())
}

/// Healthcheck container tanpa curl: GET /readyz ke port lokal, sukses bila HTTP 200.
pub fn healthcheck() -> Result<(), BoxError> {
    let settings = load_settings()?;
    let addr = SocketAddr::from(([127, 0, 0, 1], settings.server.port));
    let mut stream = TcpStream::connect_timeout(&addr, Duration::from_secs(2))?;
    stream.set_read_timeout(Some(Duration::from_secs(5)))?;
    stream.write_all(b"GET /readyz HTTP/1.0\r\nHost: localhost\r\n\r\n")?;
    let mut buf = [0u8; 16];
    let n = stream.read(&mut buf)?;
    let status_line = String::from_utf8_lossy(&buf[..n]);
    if status_line.starts_with("HTTP/1.0 200") || status_line.starts_with("HTTP/1.1 200") {
        Ok(())
    } else {
        Err(format!("not ready: {}", status_line.trim()).into())
    }
}

pub async fn shutdown_signal() {
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
