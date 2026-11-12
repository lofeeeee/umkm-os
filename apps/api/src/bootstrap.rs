use std::{
    io::{Read, Write},
    net::{SocketAddr, TcpStream},
    path::PathBuf,
    sync::Arc,
    time::Duration,
};

use std::future::IntoFuture;

use tokio::net::TcpListener;
use tracing_subscriber::{EnvFilter, fmt};
use umkm_application::onboarding::LegalVersions;
use umkm_domain::auth::AuthPolicy;
use umkm_infrastructure::storage::{PgObjectStorage, StorageLimits, UrlSigner};
use umkm_infrastructure::{
    auth::Argon2idHasher,
    metrics::Metrics,
    rate_limit::RateLimiter,
    redis::{KeyNamespace, RedisClient},
};
use umkm_shared::config::{Environment, LogFormat, Secrets, Settings};

use crate::{
    AppState, HttpLimits,
    auth::AuthService,
    build_router,
    metrics_server::{OnScrape, metrics_router},
    readiness,
};

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
    let role = umkm_database::app_role::ensure_rls_enforced(
        &pool,
        settings.environment == Environment::Development,
    )
    .await?;
    tracing::info!(db_role = %role.name, rls_enforced = role.rls_enforced(), "database role verified");
    let redis = RedisClient::open(&secrets.redis_url)?;
    // P4-6: fan-out realtime lintas instance lewat Redis pub/sub.
    let rt_ns = KeyNamespace::new(&settings.redis.key_prefix)?;
    let rt_publisher = umkm_infrastructure::realtime::RtPublisher::new(&redis, &rt_ns)?;
    let rt_client = Arc::new(RedisClient::open(&secrets.redis_url)?);
    let metrics = Arc::new(Metrics::new());
    let limiter = RateLimiter::new(
        redis.connection_manager_lazy()?,
        KeyNamespace::new(&settings.redis.key_prefix)?,
    );
    let auth = Arc::new(AuthService {
        pool: pool.clone(),
        limiter,
        hasher: Arc::new(Argon2idHasher::owasp()),
        policy: AuthPolicy::default(),
        legal: LegalVersions {
            privacy_policy: settings.legal.privacy_policy_version.clone(),
            terms_of_service: settings.legal.terms_of_service_version.clone(),
        },
        root_domain: settings.tenancy.root_domain.clone(),
        secure_cookies: settings.environment != Environment::Development,
        allowed_origins: settings.http.cors_allowed_origins.clone(),
        trust_forwarded_for: settings.http.trust_forwarded_for,
    });
    let mut state = AppState::new(vec![
        Arc::new(readiness::Postgres(pool.clone())),
        Arc::new(readiness::Redis(redis)),
    ])
    .with_metrics(metrics.clone())
    .with_db(pool.clone())
    .with_rt_publisher(rt_publisher)
    .with_files(Arc::new(crate::state::FileService {
        store: PgObjectStorage::new(
            pool.clone(),
            StorageLimits {
                max_object_bytes: settings.storage.max_object_bytes,
                max_objects_per_tenant: settings.storage.max_objects_per_tenant,
            },
        ),
        signer: UrlSigner::new(
            secrets.file_signing_key.clone(),
            settings.storage.signed_url_ttl_secs,
        ),
    }));
    if settings.root_domain_ready() {
        state = state.with_auth(auth);
    } else {
        // Tanpa root domain nyata, hostname tenant tidak dapat dibuat: endpoint auth 503.
        tracing::warn!(
            "tenancy.root_domain masih placeholder; endpoint auth dinonaktifkan (set UMKM_ROOT_DOMAIN)"
        );
    }

    let on_scrape: OnScrape = {
        let metrics = metrics.clone();
        Arc::new(move || {
            let in_use = pool.size().saturating_sub(pool.num_idle() as u32);
            metrics.db_connections_active.set(i64::from(in_use));
        })
    };

    let addr: SocketAddr = format!("{}:{}", settings.server.host, settings.server.port).parse()?;
    let metrics_addr: SocketAddr =
        format!("{}:{}", settings.metrics.host, settings.metrics.port).parse()?;
    let _bridge = crate::realtime::spawn_bridge(state.realtime.clone(), rt_client, rt_ns);
    let app = build_router(state, HttpLimits::from(&settings.server));
    let listener = TcpListener::bind(addr).await?;
    let metrics_listener = TcpListener::bind(metrics_addr).await?;
    tracing::info!(%addr, %metrics_addr, environment = %settings.environment, "umkm-api listening");

    let (stop_tx, _) = tokio::sync::broadcast::channel::<()>(1);
    let wait = |tx: &tokio::sync::broadcast::Sender<()>| {
        let mut rx = tx.subscribe();
        async move {
            let _ = rx.recv().await;
        }
    };
    let api = axum::serve(
        listener,
        app.into_make_service_with_connect_info::<SocketAddr>(),
    )
    .with_graceful_shutdown(wait(&stop_tx));
    let metrics_srv = axum::serve(metrics_listener, metrics_router(metrics, on_scrape))
        .with_graceful_shutdown(wait(&stop_tx));
    let signal = async move {
        shutdown_signal().await;
        let _ = stop_tx.send(());
        Ok::<(), std::io::Error>(())
    };
    tokio::try_join!(api.into_future(), metrics_srv.into_future(), signal)?;
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
