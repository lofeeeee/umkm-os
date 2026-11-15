//! umkm-worker — memproses transactional outbox (P1-6, `PRD.md §4.13`).
//!
//! Loop: `process_once` sampai antrean kosong, lalu tidur `poll_interval_ms`.
//! SIGTERM menghentikan loop di antara batch (event yang sedang diklaim tetap
//! aman: lease kedaluwarsa lalu diklaim ulang).

use std::{net::SocketAddr, path::PathBuf, sync::Arc, time::Duration};

use axum::{Router, http::header, routing::get};
use tracing_subscriber::{EnvFilter, fmt};
use umkm_domain::auth::AuthPolicy;
use umkm_infrastructure::{
    metrics::{CONTENT_TYPE, Metrics},
    outbox::{LogEmailSender, WorkerConfig, process_once_with},
    retention::{RetentionPolicy, purge_job},
};
use umkm_shared::config::{LogFormat, Secrets, Settings};

type BoxError = Box<dyn std::error::Error>;

#[tokio::main]
async fn main() -> std::process::ExitCode {
    match run().await {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("umkm-worker: {e}");
            std::process::ExitCode::FAILURE
        }
    }
}

async fn run() -> Result<(), BoxError> {
    let config_dir = std::env::var_os("UMKM_CONFIG_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("config"));
    let settings = Settings::load_from_env(&config_dir)?;
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    match settings.observability.log_format {
        LogFormat::Json => fmt()
            .with_env_filter(filter)
            .json()
            .flatten_event(true)
            .init(),
        LogFormat::Pretty => fmt().with_env_filter(filter).init(),
    }
    let secrets = Secrets::from_env()?;
    // Fail-closed: KEK dibutuhkan job rotasi/penggunaan rahasia AI (P5-2).
    let keyring = umkm_infrastructure::crypto::Keyring::parse(secrets.data_kek.expose())?;
    let pool = umkm_database::connect_lazy(&secrets.database_url, &settings.database)?;
    let role = umkm_database::app_role::ensure_rls_enforced(
        &pool,
        settings.environment == umkm_shared::config::Environment::Development,
    )
    .await?;
    tracing::info!(db_role = %role.name, rls_enforced = role.rls_enforced(), "database role verified");
    let metrics = Arc::new(Metrics::new());

    let addr: SocketAddr =
        format!("{}:{}", settings.metrics.host, settings.metrics.port).parse()?;
    let listener = tokio::net::TcpListener::bind(addr).await?;
    let m = metrics.clone();
    let app = Router::new().route(
        "/metrics",
        get(move || {
            let m = m.clone();
            async move { ([(header::CONTENT_TYPE, CONTENT_TYPE)], m.encode()) }
        }),
    );
    let (stop_tx, stop_rx) = tokio::sync::watch::channel(false);
    let mut metrics_stop = stop_rx.clone();
    let metrics_server = tokio::spawn(async move {
        axum::serve(listener, app)
            .with_graceful_shutdown(async move {
                let _ = metrics_stop.changed().await;
            })
            .await
    });

    let w = &settings.worker;
    let cfg = WorkerConfig {
        worker_id: format!("{}-{}", hostname(), std::process::id()),
        batch: w.batch_size,
        lease: Duration::from_secs(w.lease_secs),
        max_attempts: w.max_attempts,
        base_backoff: Duration::from_secs(w.base_backoff_secs),
        verify_url_base: settings.verify_url_base(),
        policy: AuthPolicy::default(),
    };
    // D-08 ditunda: adapter log-only (tidak mengirim email).
    let sender = LogEmailSender;
    let poll = Duration::from_millis(w.poll_interval_ms);
    tracing::info!(%addr, worker_id = %cfg.worker_id, environment = %settings.environment, "umkm-worker started");

    // Job retensi (P1-9, ADR-0012): idempoten, dijalankan periodik.
    let retention = RetentionPolicy::from(&settings.retention);
    let purge_every = Duration::from_secs(settings.retention.purge_interval_secs);
    let mut next_purge = tokio::time::Instant::now();

    // P3-2 (AC-INV-003): reservasi kedaluwarsa dilepas periodik.
    let expire_every = Duration::from_secs(15);
    let mut next_expire = tokio::time::Instant::now();

    // P4-6: notifikasi realtime lewat Redis pub/sub (best-effort; DB tetap sumber kebenaran).
    let rt = (|| {
        let c = umkm_infrastructure::redis::RedisClient::open(&secrets.redis_url).ok()?;
        let ns = umkm_infrastructure::redis::KeyNamespace::new(&settings.redis.key_prefix).ok()?;
        umkm_infrastructure::realtime::RtPublisher::new(&c, &ns).ok()
    })();
    if rt.is_none() {
        tracing::warn!("realtime publisher disabled");
    }

    // P10: poller pembayaran GoPay (auto-konfirmasi QRIS). Klien GoBiz lewat Egress (SSRF guard,
    // HTTPS publik). Poll hanya menyentuh toko yang punya charge pending → nol biaya bila tak dipakai.
    let gopay_keyring = keyring.clone();
    let gobiz = umkm_infrastructure::gopay::GobizClient::new(
        umkm_infrastructure::http_client::Egress::new(
            umkm_infrastructure::http_client::EgressPolicy::tenant(Duration::from_secs(15)),
            umkm_infrastructure::http_client::Resolver::default(),
            4,
        ),
    );

    // P5-7: pengindeksan basis pengetahuan (embedding lewat provider tenant, SSRF guard).
    let ai = umkm_infrastructure::ai::runtime::AiRuntime::new(
        pool.clone(),
        keyring,
        settings
            .ai
            .platform
            .enabled
            .then(|| settings.ai.platform.clone()),
        secrets.platform_ai_api_key.clone(),
        settings.ai.max_concurrency,
    );

    let mut stop = stop_rx;
    let shutdown = shutdown_signal();
    tokio::pin!(shutdown);
    loop {
        if tokio::time::Instant::now() >= next_purge {
            purge_job(&pool, &retention, &metrics).await;
            next_purge = tokio::time::Instant::now() + purge_every;
        }
        if tokio::time::Instant::now() >= next_expire {
            match umkm_infrastructure::commerce_repo::expire_orders(&pool, 500).await {
                Ok(n) if n > 0 => tracing::info!(expired = n, "orders expired"),
                Ok(_) => {}
                Err(e) => tracing::error!(error = %e, "order expiry failed"),
            }
            match umkm_infrastructure::approval_repo::expire(&pool, 500).await {
                Ok(n) if n > 0 => tracing::info!(expired = n, "approvals expired"),
                Ok(_) => {}
                Err(e) => tracing::error!(error = %e, "approval expiry failed"),
            }
            match umkm_infrastructure::inventory_repo::expire_reservations(&pool, 500).await {
                Ok(n) if n > 0 => tracing::info!(released = n, "reservations expired"),
                Ok(_) => {}
                Err(e) => tracing::error!(error = %e, "reservation expiry failed"),
            }
            match ai.index_pending(10).await {
                Ok((ok, failed)) if ok + failed > 0 => {
                    tracing::info!(indexed = ok, failed, "knowledge indexed")
                }
                Ok(_) => {}
                Err(e) => tracing::error!(error = %e, "knowledge indexing failed"),
            }
            // P10: edaluwarsakan charge GoPay lewat + poll pembayaran masuk (auto-konfirmasi).
            match umkm_infrastructure::gopay_repo::expire_charges_global(&pool, 500).await {
                Ok(n) if n > 0 => tracing::info!(expired = n, "gopay charges expired"),
                Ok(_) => {}
                Err(e) => tracing::error!(error = %e, "gopay charge expiry failed"),
            }
            let gp = umkm_infrastructure::gopay::poll_due(&pool, &gopay_keyring, &gobiz, 200).await;
            if gp.confirmed > 0 || gp.errors > 0 {
                tracing::info!(
                    tenants = gp.tenants,
                    confirmed = gp.confirmed,
                    errors = gp.errors,
                    "gopay poll"
                );
            }
            next_expire = tokio::time::Instant::now() + expire_every;
        }
        match process_once_with(&pool, &sender, &cfg, &metrics, rt.as_ref()).await {
            Ok(t) if t.claimed > 0 => {
                tracing::info!(
                    claimed = t.claimed,
                    sent = t.sent,
                    retried = t.retried,
                    dead = t.dead,
                    "outbox batch"
                );
                continue; // antrean mungkin masih berisi
            }
            Ok(_) => {}
            Err(e) => tracing::error!(error = %e, "outbox tick failed"),
        }
        tokio::select! {
            _ = &mut shutdown => break,
            _ = stop.changed() => break,
            _ = tokio::time::sleep(poll) => {}
        }
    }
    let _ = stop_tx.send(true);
    let _ = metrics_server.await;
    tracing::info!("umkm-worker stopped");
    Ok(())
}

fn hostname() -> String {
    std::env::var("HOSTNAME").unwrap_or_else(|_| "worker".to_owned())
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
