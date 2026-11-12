//! Validasi tujuan HTTP keluar (P5-3, `PRD.md §6.5` kontrol 1–4, 9). Murni (tanpa I/O):
//! URL dari tenant → scheme/host/port/userinfo; setiap IP hasil resolusi → bukan alamat terlarang.

use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};

use reqwest::Url;

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum SsrfError {
    #[error("URL tidak valid")]
    InvalidUrl,
    #[error("scheme tidak diizinkan (HTTPS wajib)")]
    SchemeNotAllowed,
    #[error("URL tidak boleh memuat kredensial")]
    UserInfo,
    #[error("host tidak diizinkan")]
    ForbiddenHost,
    #[error("alamat tujuan tidak diizinkan")]
    ForbiddenAddress,
    #[error("resolusi DNS gagal")]
    Dns,
    #[error("batas waktu habis")]
    Timeout,
    #[error("respons melebihi batas ukuran")]
    TooLarge,
    #[error("redirect tidak diikuti")]
    Redirect,
    #[error("terlalu banyak permintaan bersamaan")]
    Busy,
    #[error("koneksi gagal")]
    Transport,
}

impl SsrfError {
    /// Kode stabil untuk metering/log (`ai_runs.error_code`), tanpa detail tujuan.
    pub fn code(&self) -> &'static str {
        match self {
            Self::InvalidUrl => "invalid_url",
            Self::SchemeNotAllowed => "scheme_not_allowed",
            Self::UserInfo => "userinfo_in_url",
            Self::ForbiddenHost => "forbidden_host",
            Self::ForbiddenAddress => "forbidden_address",
            Self::Dns => "dns_failure",
            Self::Timeout => "timeout",
            Self::TooLarge => "response_too_large",
            Self::Redirect => "redirect_blocked",
            Self::Busy => "busy",
            Self::Transport => "transport",
        }
    }
}

/// Tujuan yang lolos validasi sintaksis (belum resolusi DNS).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Target {
    pub url: Url,
    pub host: String,
    pub port: u16,
}

/// Kontrol 1–3: scheme, hostname, userinfo, nama host lokal, IP literal terlarang.
/// `allow_http`/`allow_private` hanya untuk endpoint yang didaftarkan operator (kontrol 9), tidak pernah tenant.
pub fn validate_url(raw: &str, allow_http: bool, allow_private: bool) -> Result<Target, SsrfError> {
    if raw.len() > 300 || raw.chars().any(|c| c.is_whitespace() || c.is_control()) {
        return Err(SsrfError::InvalidUrl);
    }
    let url = Url::parse(raw).map_err(|_| SsrfError::InvalidUrl)?;
    match url.scheme() {
        "https" => {}
        "http" if allow_http => {}
        _ => return Err(SsrfError::SchemeNotAllowed),
    }
    if !url.username().is_empty() || url.password().is_some() {
        return Err(SsrfError::UserInfo);
    }
    if url.fragment().is_some() {
        return Err(SsrfError::InvalidUrl);
    }
    let raw_host = url.host_str().ok_or(SsrfError::InvalidUrl)?;
    let host = if let Some(v6) = raw_host.strip_prefix('[').and_then(|h| h.strip_suffix(']')) {
        let ip: Ipv6Addr = v6.parse().map_err(|_| SsrfError::InvalidUrl)?;
        check_ip(IpAddr::V6(ip), allow_private)?;
        ip.to_string()
    } else if let Ok(ip) = raw_host.parse::<Ipv4Addr>() {
        // Parser URL sudah menormalkan bentuk numerik (2130706433, 0x7f.0.0.1) ke IPv4 bertitik.
        check_ip(IpAddr::V4(ip), allow_private)?;
        ip.to_string()
    } else {
        let d = raw_host.trim_end_matches('.').to_ascii_lowercase();
        if !allow_private && forbidden_name(&d) {
            return Err(SsrfError::ForbiddenHost);
        }
        d
    };
    let port = url.port_or_known_default().ok_or(SsrfError::InvalidUrl)?;
    Ok(Target { url, host, port })
}

/// Nama yang selalu menunjuk ke jaringan lokal/internal, atau bukan FQDN publik.
fn forbidden_name(d: &str) -> bool {
    const SUFFIXES: [&str; 7] = [
        ".localhost",
        ".local",
        ".internal",
        ".lan",
        ".home.arpa",
        ".intranet",
        ".corp",
    ];
    d.is_empty()
        || !d.contains('.')
        || d == "localhost"
        || d == "metadata.google.internal"
        || SUFFIXES.iter().any(|s| d.ends_with(s))
        // Bentuk numerik yang dinormalisasi parser sebagai domain (mis. 0x7f.1) tidak diterima.
        || d.split('.').all(|l| l.chars().all(|c| c.is_ascii_hexdigit() || c == 'x'))
}

/// Kontrol 3–4: IP terlarang (diterapkan pada IP literal DAN setiap hasil resolusi DNS).
pub fn check_ip(ip: IpAddr, allow_private: bool) -> Result<(), SsrfError> {
    if forbidden_ip(ip, allow_private) {
        Err(SsrfError::ForbiddenAddress)
    } else {
        Ok(())
    }
}

pub fn forbidden_ip(ip: IpAddr, allow_private: bool) -> bool {
    match ip {
        IpAddr::V4(v4) => forbidden_v4(v4, allow_private),
        IpAddr::V6(v6) => forbidden_v6(v6, allow_private),
    }
}

fn in_v4(ip: Ipv4Addr, net: [u8; 4], prefix: u8) -> bool {
    let mask = if prefix == 0 {
        0
    } else {
        u32::MAX << (32 - prefix)
    };
    (u32::from(ip) & mask) == (u32::from(Ipv4Addr::from(net)) & mask)
}

fn forbidden_v4(ip: Ipv4Addr, allow_private: bool) -> bool {
    // Selalu terlarang, bahkan untuk endpoint operator: unspecified, link-local (metadata 169.254.169.254),
    // multicast, broadcast, reserved, dokumentasi, benchmarking.
    const ALWAYS: [([u8; 4], u8); 10] = [
        ([0, 0, 0, 0], 8),
        ([169, 254, 0, 0], 16),
        ([192, 0, 0, 0], 24),
        ([192, 0, 2, 0], 24),
        ([192, 88, 99, 0], 24),
        ([198, 18, 0, 0], 15),
        ([198, 51, 100, 0], 24),
        ([203, 0, 113, 0], 24),
        ([224, 0, 0, 0], 4),
        ([240, 0, 0, 0], 4),
    ];
    // Jaringan privat/lokal: terlarang untuk tenant; boleh hanya untuk endpoint operator (kontrol 9).
    const PRIVATE: [([u8; 4], u8); 5] = [
        ([10, 0, 0, 0], 8),
        ([100, 64, 0, 0], 10),
        ([127, 0, 0, 0], 8),
        ([172, 16, 0, 0], 12),
        ([192, 168, 0, 0], 16),
    ];
    ALWAYS.iter().any(|(n, p)| in_v4(ip, *n, *p))
        || (!allow_private && PRIVATE.iter().any(|(n, p)| in_v4(ip, *n, *p)))
}

fn forbidden_v6(ip: Ipv6Addr, allow_private: bool) -> bool {
    let s = ip.segments();
    // Alamat yang menyematkan IPv4: periksa IPv4 di dalamnya (mapped ::ffff:0:0/96, compatible ::/96,
    // NAT64 64:ff9b::/96). 6to4 (2002::/16) & Teredo (2001::/32) ditolak: tujuan IPv4 sebenarnya tidak pasti.
    if let Some(v4) = ip.to_ipv4_mapped() {
        return forbidden_v4(v4, allow_private);
    }
    if s[0..6] == [0, 0, 0, 0, 0, 0] && !ip.is_unspecified() && !ip.is_loopback() {
        return forbidden_v4(
            Ipv4Addr::new((s[6] >> 8) as u8, s[6] as u8, (s[7] >> 8) as u8, s[7] as u8),
            allow_private,
        );
    }
    if s[0] == 0x64 && s[1] == 0xff9b {
        return forbidden_v4(
            Ipv4Addr::new((s[6] >> 8) as u8, s[6] as u8, (s[7] >> 8) as u8, s[7] as u8),
            allow_private,
        );
    }
    let always = ip.is_unspecified()
        || (s[0] & 0xffc0) == 0xfe80 // link-local
        || (s[0] & 0xffc0) == 0xfec0 // site-local (usang)
        || (s[0] & 0xff00) == 0xff00 // multicast
        || s[0] == 0x2002 // 6to4
        || (s[0] == 0x2001 && s[1] == 0) // Teredo
        || (s[0] == 0x2001 && s[1] == 0x0db8) // dokumentasi
        || (s[0] == 0x0100 && s[1..4] == [0, 0, 0]); // discard
    let private = ip.is_loopback() || (s[0] & 0xfe00) == 0xfc00; // ULA fc00::/7 (termasuk metadata fd00:ec2::254)
    always || (!allow_private && private)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ip(s: &str) -> IpAddr {
        s.parse().unwrap()
    }

    #[test]
    fn forbidden_addresses_for_tenants() {
        for a in [
            "127.0.0.1",
            "127.255.0.9",
            "10.1.2.3",
            "172.16.0.1",
            "172.31.255.255",
            "192.168.1.1",
            "100.64.0.1",
            "169.254.169.254",
            "0.0.0.0",
            "224.0.0.1",
            "255.255.255.255",
            "192.0.2.5",
            "198.18.0.1",
            "::1",
            "::",
            "fe80::1",
            "fc00::1",
            "fd00:ec2::254",
            "ff02::1",
            "::ffff:127.0.0.1",
            "::ffff:169.254.169.254",
            "::127.0.0.1",
            "64:ff9b::a9fe:a9fe",
            "2002:7f00:1::",
            "2001:0:4136:e378::",
            "2001:db8::1",
            "fec0::1",
        ] {
            assert!(forbidden_ip(ip(a), false), "{a} harus ditolak");
        }
        for a in [
            "8.8.8.8",
            "1.1.1.1",
            "104.18.0.1",
            "172.32.0.1",
            "2606:4700::1111",
            "::ffff:8.8.8.8",
            "64:ff9b::808:808",
        ] {
            assert!(!forbidden_ip(ip(a), false), "{a} harus diizinkan");
        }
    }

    #[test]
    fn operator_endpoints_may_be_private_but_never_metadata() {
        for a in ["10.0.0.5", "127.0.0.1", "fd12::1", "::1"] {
            assert!(!forbidden_ip(ip(a), true), "{a}");
        }
        for a in [
            "169.254.169.254",
            "0.0.0.0",
            "fe80::1",
            "::ffff:169.254.169.254",
            "224.0.0.1",
        ] {
            assert!(forbidden_ip(ip(a), true), "{a}");
        }
    }

    #[test]
    fn url_validation() {
        let ok = validate_url("https://api.openai.com/v1", false, false).unwrap();
        assert_eq!((ok.host.as_str(), ok.port), ("api.openai.com", 443));
        assert_eq!(
            validate_url("https://API.Example.com.:8443/v1", false, false)
                .unwrap()
                .host,
            "api.example.com"
        );
        let cases = [
            ("http://api.example.com/v1", SsrfError::SchemeNotAllowed),
            ("ftp://api.example.com", SsrfError::SchemeNotAllowed),
            ("file:///etc/passwd", SsrfError::SchemeNotAllowed),
            ("https://user:pw@api.example.com", SsrfError::UserInfo),
            ("https://localhost/v1", SsrfError::ForbiddenHost),
            ("https://ollama.local/v1", SsrfError::ForbiddenHost),
            ("https://vllm.internal/v1", SsrfError::ForbiddenHost),
            (
                "https://metadata.google.internal/",
                SsrfError::ForbiddenHost,
            ),
            ("https://intranet/v1", SsrfError::ForbiddenHost),
            ("https://127.0.0.1/v1", SsrfError::ForbiddenAddress),
            ("https://2130706433/v1", SsrfError::ForbiddenAddress),
            ("https://0x7f.0.0.1/v1", SsrfError::ForbiddenAddress),
            ("https://[::ffff:a9fe:a9fe]/", SsrfError::ForbiddenAddress),
            (
                "https://169.254.169.254/latest/meta-data",
                SsrfError::ForbiddenAddress,
            ),
            ("https://api.example.com/v1#x", SsrfError::InvalidUrl),
            ("https://api.example.com/ v1", SsrfError::InvalidUrl),
            ("not a url", SsrfError::InvalidUrl),
        ];
        for (u, want) in cases {
            assert_eq!(validate_url(u, false, false).unwrap_err(), want, "{u}");
        }
        // Endpoint operator (kontrol 9): http & privat boleh, metadata tetap tidak.
        assert!(validate_url("http://10.0.0.5:8000/v1", true, true).is_ok());
        assert_eq!(
            validate_url("http://169.254.169.254/", true, true).unwrap_err(),
            SsrfError::ForbiddenAddress
        );
    }
}
