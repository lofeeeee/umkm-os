//! Klien HTTP keluar terkendali (P5-3, `PRD.md §6.5`, ADR-0011). SATU-SATUNYA jalur ke endpoint
//! provider AI/embedding. Sembilan kontrol:
//! 1–3 `ssrf::validate_url` (scheme, host, HTTPS, userinfo, nama/IP lokal);
//! 4 setiap IP hasil resolusi divalidasi; 5 koneksi dipatok ke IP tervalidasi (`resolve_to_addrs`),
//!   tanpa proxy sistem → tidak ada resolusi kedua (anti DNS rebinding);
//! 6 redirect dimatikan, 3xx = error; 7 timeout koneksi/total, batas ukuran respons, batas konkurensi;
//! 8 header Authorization baru dipasang setelah semua validasi lolos;
//! 9 jaringan privat/HTTP hanya untuk `EgressPolicy::operator` (endpoint yang didaftarkan platform).

pub mod ssrf;

use std::{
    collections::HashMap,
    net::{IpAddr, SocketAddr},
    sync::{Arc, Once},
    time::Duration,
};

use tokio::sync::Semaphore;

pub use ssrf::{SsrfError, Target};

#[derive(Debug, Clone)]
pub struct EgressPolicy {
    pub allow_http: bool,
    pub allow_private: bool,
    pub connect_timeout: Duration,
    pub total_timeout: Duration,
    pub max_response_bytes: usize,
}

impl EgressPolicy {
    /// Endpoint milik tenant (BYOK): HTTPS publik saja.
    pub fn tenant(total_timeout: Duration) -> Self {
        Self {
            allow_http: false,
            allow_private: false,
            connect_timeout: Duration::from_secs(5),
            total_timeout,
            max_response_bytes: 2 * 1024 * 1024,
        }
    }

    /// Endpoint provider platform yang didaftarkan operator (mis. vLLM/Ollama internal; kontrol 9).
    pub fn operator(total_timeout: Duration) -> Self {
        Self {
            allow_http: true,
            allow_private: true,
            ..Self::tenant(total_timeout)
        }
    }
}

/// Sumber resolusi DNS. `Static` dipakai uji & pemetaan host operator yang eksplisit.
#[derive(Debug, Clone, Default)]
pub enum Resolver {
    #[default]
    System,
    Static(HashMap<String, Vec<IpAddr>>),
}

impl Resolver {
    async fn resolve(&self, host: &str, port: u16) -> Result<Vec<IpAddr>, SsrfError> {
        let ips: Vec<IpAddr> = match self {
            Self::Static(m) => m.get(host).cloned().unwrap_or_default(),
            Self::System => tokio::net::lookup_host((host, port))
                .await
                .map_err(|_| SsrfError::Dns)?
                .map(|a| a.ip())
                .collect(),
        };
        if ips.is_empty() {
            Err(SsrfError::Dns)
        } else {
            Ok(ips)
        }
    }
}

#[derive(Debug)]
pub struct HttpResponse {
    pub status: u16,
    pub body: Vec<u8>,
}

/// Klien keluar. Klon murah (berbagi semaphore konkurensi).
#[derive(Debug, Clone)]
pub struct Egress {
    policy: EgressPolicy,
    resolver: Resolver,
    permits: Arc<Semaphore>,
}

static TLS_PROVIDER: Once = Once::new();

impl Egress {
    pub fn new(policy: EgressPolicy, resolver: Resolver, max_concurrency: usize) -> Self {
        // reqwest dibangun dengan `rustls-no-provider`: pasang provider `ring` sekali per proses.
        TLS_PROVIDER.call_once(|| {
            let _ = rustls::crypto::ring::default_provider().install_default();
        });
        Self {
            policy,
            resolver,
            permits: Arc::new(Semaphore::new(max_concurrency.max(1))),
        }
    }

    pub fn policy(&self) -> &EgressPolicy {
        &self.policy
    }

    /// Kontrol 1–4: validasi URL lalu SEMUA IP hasil resolusi. Satu IP terlarang → tolak seluruhnya.
    pub async fn resolve_checked(&self, url: &str) -> Result<(Target, Vec<SocketAddr>), SsrfError> {
        let t = ssrf::validate_url(url, self.policy.allow_http, self.policy.allow_private)?;
        let ips = match t.host.parse::<IpAddr>() {
            Ok(ip) => vec![ip],
            Err(_) => self.resolver.resolve(&t.host, t.port).await?,
        };
        for ip in &ips {
            ssrf::check_ip(*ip, self.policy.allow_private)?;
        }
        let addrs = ips
            .into_iter()
            .map(|ip| SocketAddr::new(ip, t.port))
            .collect();
        Ok((t, addrs))
    }

    /// POST JSON (byte sudah diserialisasi) ke `url`. `bearer` hanya dikirim bila tujuan lolos (kontrol 8).
    pub async fn post_json(
        &self,
        url: &str,
        bearer: Option<&str>,
        body: Vec<u8>,
    ) -> Result<HttpResponse, SsrfError> {
        self.send(reqwest::Method::POST, url, bearer, Some(body))
            .await
    }

    pub async fn get(&self, url: &str, bearer: Option<&str>) -> Result<HttpResponse, SsrfError> {
        self.send(reqwest::Method::GET, url, bearer, None).await
    }

    async fn send(
        &self,
        method: reqwest::Method,
        url: &str,
        bearer: Option<&str>,
        body: Option<Vec<u8>>,
    ) -> Result<HttpResponse, SsrfError> {
        let _permit = self
            .permits
            .clone()
            .try_acquire_owned()
            .map_err(|_| SsrfError::Busy)?;
        let (target, addrs) = self.resolve_checked(url).await?;
        let client = reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .no_proxy()
            .https_only(!self.policy.allow_http)
            .connect_timeout(self.policy.connect_timeout)
            .timeout(self.policy.total_timeout)
            .resolve_to_addrs(&target.host, &addrs)
            .build()
            .map_err(|_| SsrfError::Transport)?;
        let mut req = client
            .request(method, target.url.clone())
            .header("accept", "application/json");
        if let Some(b) = body {
            req = req.header("content-type", "application/json").body(b);
        }
        if let Some(k) = bearer {
            let mut v = reqwest::header::HeaderValue::from_str(&format!("Bearer {k}"))
                .map_err(|_| SsrfError::InvalidUrl)?;
            v.set_sensitive(true);
            req = req.header(reqwest::header::AUTHORIZATION, v);
        }
        let mut resp = req.send().await.map_err(map_err)?;
        let status = resp.status();
        if status.is_redirection() {
            return Err(SsrfError::Redirect);
        }
        let max = self.policy.max_response_bytes;
        if resp.content_length().is_some_and(|n| n as usize > max) {
            return Err(SsrfError::TooLarge);
        }
        let mut buf = Vec::new();
        while let Some(chunk) = resp.chunk().await.map_err(map_err)? {
            if buf.len() + chunk.len() > max {
                return Err(SsrfError::TooLarge);
            }
            buf.extend_from_slice(&chunk);
        }
        Ok(HttpResponse {
            status: status.as_u16(),
            body: buf,
        })
    }
}

fn map_err(e: reqwest::Error) -> SsrfError {
    if e.is_timeout() {
        SsrfError::Timeout
    } else {
        SsrfError::Transport
    }
}
