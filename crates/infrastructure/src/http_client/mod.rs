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

/// Header permintaan untuk [`Egress::request`]. `sensitive` menandai nilai yang tak boleh di-log.
#[derive(Debug, Clone, Copy)]
pub struct Header<'a> {
    pub name: &'a str,
    pub value: &'a str,
    pub sensitive: bool,
}

impl<'a> Header<'a> {
    pub fn plain(name: &'a str, value: &'a str) -> Self {
        Self {
            name,
            value,
            sensitive: false,
        }
    }

    pub fn sensitive(name: &'a str, value: &'a str) -> Self {
        Self {
            name,
            value,
            sensitive: true,
        }
    }
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

    /// Salinan dengan kebijakan lain (mis. operator) yang BERBAGI semaphore konkurensi.
    pub fn with_policy(&self, policy: EgressPolicy) -> Self {
        let mut c = self.clone();
        c.policy = policy;
        c
    }

    /// Salinan dengan batas waktu total lain; semaphore konkurensi tetap DIBAGI.
    pub fn with_timeout(&self, total: Duration) -> Self {
        let mut c = self.clone();
        c.policy.total_timeout = total;
        c
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
        // Header bawaan jalur AI: accept + (jika ada body) content-type JSON, lalu Authorization.
        let mut headers: Vec<Header<'_>> = vec![Header::plain("accept", "application/json")];
        if body.is_some() {
            headers.push(Header::plain("content-type", "application/json"));
        }
        let auth;
        if let Some(k) = bearer {
            auth = format!("Bearer {k}");
            headers.push(Header::sensitive("authorization", &auth));
        }
        self.request(method, url, &headers, body).await
    }

    /// Jalur umum dengan header kustom (mis. sesi GoPay/GoBiz: cookie, x-appid, origin). Semua kontrol
    /// SSRF 1–9 tetap berlaku — header tidak memengaruhi URL/IP/redirect/ukuran. Header bernama
    /// `authorization`/`cookie` ditandai sensitif (tidak di-log). Caller menyusun query di `url`.
    pub async fn request(
        &self,
        method: reqwest::Method,
        url: &str,
        headers: &[Header<'_>],
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
        let mut req = client.request(method, target.url.clone());
        for h in headers {
            let name = reqwest::header::HeaderName::from_bytes(h.name.as_bytes())
                .map_err(|_| SsrfError::InvalidUrl)?;
            let mut value = reqwest::header::HeaderValue::from_str(h.value)
                .map_err(|_| SsrfError::InvalidUrl)?;
            if h.sensitive || name == reqwest::header::AUTHORIZATION || name == "cookie" {
                value.set_sensitive(true);
            }
            req = req.header(name, value);
        }
        if let Some(b) = body {
            req = req.body(b);
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
