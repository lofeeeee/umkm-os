//! Aturan tenant murni: slug/subdomain (TEN-004, TEN-005, TEN-006, ADR-0001).

use std::fmt;

pub const SLUG_MIN: usize = 3;
pub const SLUG_MAX: usize = 63;
pub const NAME_MAX_CHARS: usize = 120;

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum TenantError {
    #[error("nama bisnis wajib 1..120 karakter")]
    InvalidName,
    #[error("alamat toko harus 3..63 karakter a-z, 0-9, '-' dan tidak diawali/diakhiri '-'")]
    InvalidSlug,
    #[error("alamat toko sudah dipakai atau dicadangkan")]
    SlugUnavailable,
    #[error("host tidak valid")]
    InvalidHost,
}

/// Slug tervalidasi; sama dengan constraint `ck_tenants_slug` di database.
#[derive(Clone, PartialEq, Eq, Hash)]
pub struct Slug(String);

impl Slug {
    pub fn parse(raw: &str) -> Result<Self, TenantError> {
        let s = raw.trim();
        let bytes = s.as_bytes();
        let valid = (SLUG_MIN..=SLUG_MAX).contains(&s.len())
            && bytes
                .iter()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || *b == b'-')
            && bytes.first() != Some(&b'-')
            && bytes.last() != Some(&b'-');
        if valid {
            Ok(Self(s.to_owned()))
        } else {
            Err(TenantError::InvalidSlug)
        }
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Hostname subdomain utama: `{slug}.{root_domain}` (BR-003).
    pub fn hostname(&self, root_domain: &str) -> String {
        format!("{}.{}", self.0, root_domain)
    }

    /// Kandidat ke-`n` (n >= 2): `{slug}-{n}`, dipotong agar tetap <= 63.
    pub fn with_suffix(&self, n: u32) -> Self {
        let suffix = format!("-{n}");
        let keep = SLUG_MAX - suffix.len();
        let base = self.0[..self.0.len().min(keep)].trim_end_matches('-');
        Self(format!("{base}{suffix}"))
    }
}

impl fmt::Debug for Slug {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Slug({})", self.0)
    }
}

impl fmt::Display for Slug {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// Normalisasi header `Host` (TEN-009): buang port & titik akhir, huruf kecil,
/// validasi label DNS. IP literal dan bentuk aneh ditolak (bukan host tenant).
pub fn normalize_host(raw: &str) -> Result<String, TenantError> {
    let raw = raw.trim();
    if raw.starts_with('[') {
        return Err(TenantError::InvalidHost); // IPv6 literal
    }
    let host = match raw.rsplit_once(':') {
        Some((h, port)) if !port.is_empty() && port.bytes().all(|b| b.is_ascii_digit()) => h,
        Some(_) => return Err(TenantError::InvalidHost),
        None => raw,
    };
    let host = host.strip_suffix('.').unwrap_or(host).to_ascii_lowercase();
    let labels: Vec<&str> = host.split('.').collect();
    let valid = !host.is_empty()
        && host.len() <= 253
        && labels.iter().all(|l| {
            !l.is_empty()
                && l.len() <= 63
                && !l.starts_with('-')
                && !l.ends_with('-')
                && l.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
        })
        // IPv4 literal (semua label numerik) bukan host tenant
        && !labels.iter().all(|l| l.bytes().all(|b| b.is_ascii_digit()));
    if valid {
        Ok(host)
    } else {
        Err(TenantError::InvalidHost)
    }
}

pub fn validate_business_name(name: &str) -> Result<String, TenantError> {
    let n = name.trim();
    let len = n.chars().count();
    if len == 0 || len > NAME_MAX_CHARS || n.chars().any(char::is_control) {
        return Err(TenantError::InvalidName);
    }
    Ok(n.to_owned())
}

/// Slug otomatis dari nama bisnis (TEN-004): huruf Latin beraksen disederhanakan,
/// selain a-z0-9 menjadi '-', dirapatkan, dipotong ke 63. Nama yang tidak menghasilkan
/// minimal 3 karakter memakai awalan `toko-`.
pub fn slugify(name: &str) -> Slug {
    let mut out = String::with_capacity(name.len());
    for c in name.chars().flat_map(char::to_lowercase) {
        let mapped = match c {
            'a'..='z' | '0'..='9' => Some(c),
            'à' | 'á' | 'â' | 'ã' | 'ä' | 'å' => Some('a'),
            'è' | 'é' | 'ê' | 'ë' => Some('e'),
            'ì' | 'í' | 'î' | 'ï' => Some('i'),
            'ò' | 'ó' | 'ô' | 'õ' | 'ö' => Some('o'),
            'ù' | 'ú' | 'û' | 'ü' => Some('u'),
            'ñ' => Some('n'),
            'ç' => Some('c'),
            _ => None,
        };
        match mapped {
            Some(m) => out.push(m),
            None if !out.ends_with('-') => out.push('-'),
            None => {}
        }
    }
    let mut s = out.trim_matches('-').to_owned();
    s.truncate(SLUG_MAX);
    let s = s.trim_end_matches('-');
    let s = if s.is_empty() {
        "toko-baru".to_owned()
    } else if s.len() < SLUG_MIN {
        format!("toko-{s}")
    } else {
        s.to_owned()
    };
    Slug::parse(&s).expect("slugify selalu menghasilkan slug valid")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slug_parse_matches_db_rules() {
        for ok in ["abc", "toko-budi", "a1b", &"a".repeat(63)] {
            assert!(Slug::parse(ok).is_ok(), "{ok}");
        }
        for bad in [
            "ab",
            "Toko",
            "-abc",
            "abc-",
            "a_b",
            "a.b",
            "toko budi",
            &"a".repeat(64),
            "",
        ] {
            assert_eq!(Slug::parse(bad), Err(TenantError::InvalidSlug), "{bad}");
        }
    }

    #[test]
    fn slugify_examples() {
        assert_eq!(slugify("Toko Budi").as_str(), "toko-budi");
        assert_eq!(
            slugify("  Kopi & Roti — Bu Siti!! ").as_str(),
            "kopi-roti-bu-siti"
        );
        assert_eq!(slugify("Café Ñoño").as_str(), "cafe-nono");
        assert_eq!(slugify("AB").as_str(), "toko-ab");
        assert_eq!(slugify("!!!").as_str(), "toko-baru");
        assert_eq!(slugify("店铺").as_str(), "toko-baru");
        assert_eq!(slugify(&"x".repeat(200)).as_str().len(), 63);
    }

    #[test]
    fn slugify_always_valid() {
        for name in [
            "",
            "-",
            "a",
            "a-",
            "---a---",
            "12",
            "Ü",
            &"é-".repeat(100),
            "a\u{0}b",
        ] {
            let s = slugify(name);
            assert!(Slug::parse(s.as_str()).is_ok(), "{name:?} -> {s:?}");
        }
    }

    #[test]
    fn suffix_candidates_stay_valid() {
        let s = Slug::parse("toko-budi").unwrap();
        assert_eq!(s.with_suffix(2).as_str(), "toko-budi-2");
        let long = Slug::parse(&"a".repeat(63)).unwrap();
        let c = long.with_suffix(12);
        assert_eq!(c.as_str().len(), 63);
        assert!(c.as_str().ends_with("-12"));
        let dashy = Slug::parse(&format!("{}-b", "a".repeat(59))).unwrap();
        assert!(Slug::parse(dashy.with_suffix(10).as_str()).is_ok());
    }

    #[test]
    fn normalize_host_cases() {
        for (raw, want) in [
            ("toko.ryasai.my.id", "toko.ryasai.my.id"),
            ("TOKO.Ryasai.MY.ID", "toko.ryasai.my.id"),
            ("toko.ryasai.my.id:443", "toko.ryasai.my.id"),
            ("toko.ryasai.my.id.", "toko.ryasai.my.id"),
            ("  toko.localhost:18082 ", "toko.localhost"),
        ] {
            assert_eq!(normalize_host(raw).as_deref(), Ok(want), "{raw}");
        }
        for bad in [
            "",
            "127.0.0.1",
            "127.0.0.1:80",
            "[::1]:80",
            "toko..id",
            "-x.id",
            "x_y.id",
            "toko.id:abc",
            "toko.id:",
            "a b.id",
            "toko.id/x",
            "t@o.id",
        ] {
            assert_eq!(
                normalize_host(bad),
                Err(TenantError::InvalidHost),
                "{bad:?}"
            );
        }
    }

    #[test]
    fn hostname_and_name_validation() {
        assert_eq!(
            Slug::parse("toko-budi").unwrap().hostname("ryasai.my.id"),
            "toko-budi.ryasai.my.id"
        );
        assert_eq!(validate_business_name("  Toko Budi ").unwrap(), "Toko Budi");
        assert!(validate_business_name("   ").is_err());
        assert!(validate_business_name(&"x".repeat(121)).is_err());
        assert!(validate_business_name("a\nb").is_err());
    }
}
