//! Aturan autentikasi murni (P1-2, `PRD.md §4.1`, `PRD.md §6.1`).
//!
//! Tidak ada I/O di sini. Hashing password, pembangkitan token, dan penyimpanan
//! adalah adapter di `infrastructure` yang mengimplementasikan port di bawah.

use std::{fmt, time::Duration};

/// Panjang password (karakter Unicode). Batas atas mencegah DoS pada Argon2id.
/// Mengikuti NIST SP 800-63B: minimal 8, tanpa aturan komposisi paksa.
pub const PASSWORD_MIN_CHARS: usize = 8;
pub const PASSWORD_MAX_CHARS: usize = 128;

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum AuthError {
    #[error("email tidak valid")]
    InvalidEmail,
    #[error("password tidak memenuhi kebijakan: {0}")]
    WeakPassword(&'static str),
    /// Satu pesan untuk email tidak ada / password salah / akun non-aktif,
    /// agar tidak membocorkan keberadaan akun (user enumeration).
    #[error("email atau password salah")]
    InvalidCredentials,
    #[error("terlalu banyak percobaan; coba lagi dalam {retry_after_secs} detik")]
    RateLimited { retry_after_secs: u64 },
    #[error("kesalahan internal autentikasi")]
    Internal,
}

/// Email ternormalisasi: di-trim; perbandingan keunikan case-insensitive (AUTH-002)
/// dilakukan oleh index `lower(email)` di database.
#[derive(Clone, PartialEq, Eq)]
pub struct Email(String);

impl Email {
    pub fn parse(raw: &str) -> Result<Self, AuthError> {
        let s = raw.trim();
        let (local, domain) = s.split_once('@').ok_or(AuthError::InvalidEmail)?;
        let valid = (3..=254).contains(&s.len())
            && !local.is_empty()
            && local.len() <= 64
            && domain.contains('.')
            && !domain.starts_with('.')
            && !domain.ends_with('.')
            && !domain.contains('@')
            && !s.chars().any(|c| c.is_whitespace() || c.is_control());
        if valid {
            Ok(Self(s.to_owned()))
        } else {
            Err(AuthError::InvalidEmail)
        }
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Kunci perbandingan/rate-limit (lowercase).
    pub fn canonical(&self) -> String {
        self.0.to_lowercase()
    }
}

impl fmt::Debug for Email {
    // Email adalah data pribadi; jangan tercetak utuh di log debug.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let domain = self.0.split_once('@').map(|(_, d)| d).unwrap_or("");
        write!(f, "Email(***@{domain})")
    }
}

/// Validasi kebijakan password (bukan kekuatan kriptografis; itu tugas Argon2id).
pub fn validate_password(password: &str, email: &Email) -> Result<(), AuthError> {
    let n = password.chars().count();
    if n < PASSWORD_MIN_CHARS {
        return Err(AuthError::WeakPassword("minimal 8 karakter"));
    }
    if n > PASSWORD_MAX_CHARS {
        return Err(AuthError::WeakPassword("maksimal 128 karakter"));
    }
    if password.trim().is_empty() {
        return Err(AuthError::WeakPassword("tidak boleh hanya spasi"));
    }
    if password.to_lowercase() == email.canonical() {
        return Err(AuthError::WeakPassword("tidak boleh sama dengan email"));
    }
    Ok(())
}

/// SHA-256 dari token opak. Hanya hash ini yang disimpan (`PRD.md §4.1`).
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct TokenHash(pub [u8; 32]);

impl fmt::Debug for TokenHash {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("TokenHash(***)")
    }
}

/// Port hashing password (adapter: Argon2id di infrastructure).
pub trait PasswordHasher: Send + Sync {
    fn hash(&self, password: &str) -> Result<String, AuthError>;
    /// `Ok(false)` untuk password salah; `Err` hanya untuk hash rusak/internal.
    fn verify(&self, password: &str, hash: &str) -> Result<bool, AuthError>;
    /// True bila hash dibuat dengan parameter lebih lemah dari saat ini
    /// (parameter "dapat ditingkatkan", `PRD.md §6.1`).
    fn needs_rehash(&self, hash: &str) -> bool;
}

/// Masa berlaku kredensial. Nilai default dapat ditimpa konfigurasi.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AuthPolicy {
    pub session_ttl: Duration,
    pub password_reset_ttl: Duration,
    pub email_verification_ttl: Duration,
    pub invitation_ttl: Duration,
}

impl Default for AuthPolicy {
    fn default() -> Self {
        Self {
            session_ttl: Duration::from_secs(14 * 24 * 3600),
            // "short expiry" untuk reset (PRD.md §6.1)
            password_reset_ttl: Duration::from_secs(30 * 60),
            email_verification_ttl: Duration::from_secs(24 * 3600),
            invitation_ttl: Duration::from_secs(7 * 24 * 3600),
        }
    }
}

/// Aturan rate limit (AUTH-007, `PRD.md §6.1`): `limit` percobaan per `window`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RateRule {
    /// Segmen key Redis: `[a-z_]` saja.
    pub name: &'static str,
    pub limit: u32,
    pub window: Duration,
}

pub mod rate_rules {
    use std::time::Duration;

    use super::RateRule;

    const MIN15: Duration = Duration::from_secs(15 * 60);
    const HOUR: Duration = Duration::from_secs(3600);

    pub const LOGIN_PER_EMAIL: RateRule = RateRule {
        name: "login_email",
        limit: 5,
        window: MIN15,
    };
    pub const LOGIN_PER_IP: RateRule = RateRule {
        name: "login_ip",
        limit: 30,
        window: MIN15,
    };
    pub const RESET_PER_EMAIL: RateRule = RateRule {
        name: "reset_email",
        limit: 3,
        window: HOUR,
    };
    pub const RESET_PER_IP: RateRule = RateRule {
        name: "reset_ip",
        limit: 10,
        window: HOUR,
    };
    pub const REGISTER_PER_IP: RateRule = RateRule {
        name: "register_ip",
        limit: 5,
        window: HOUR,
    };
    pub const VERIFY_TOKEN_PER_IP: RateRule = RateRule {
        name: "verify_token_ip",
        limit: 20,
        window: HOUR,
    };
    pub const VERIFY_PER_EMAIL: RateRule = RateRule {
        name: "verify_email",
        limit: 5,
        window: HOUR,
    };

    /// Checkout publik per IP (P3-5): membatasi pembuatan order/reservasi palsu.
    pub const CHECKOUT_PER_IP: RateRule = RateRule {
        name: "checkout_ip",
        limit: 20,
        window: MIN15,
    };

    /// Langkah kedua MFA per IP (percobaan per tantangan juga dibatasi di DB).
    pub const MFA_PER_IP: RateRule = RateRule {
        name: "mfa_ip",
        limit: 30,
        window: MIN15,
    };
    /// Operasi MFA per akun (setup/aktifkan/matikan/kode pemulihan).
    pub const MFA_PER_USER: RateRule = RateRule {
        name: "mfa_user",
        limit: 10,
        window: MIN15,
    };

    /// Web chat publik (P6-2): percakapan baru per IP.
    pub const CHAT_START_PER_IP: RateRule = RateRule {
        name: "chat_start_ip",
        limit: 10,
        window: MIN15,
    };
    /// Web chat publik: pesan pelanggan per IP (tiap pesan dapat memicu panggilan AI berbayar).
    pub const CHAT_MESSAGE_PER_IP: RateRule = RateRule {
        name: "chat_msg_ip",
        limit: 40,
        window: MIN15,
    };

    /// Ubah kata sandi dari halaman Keamanan (verifikasi kata sandi lama = tebakan password).
    pub const PASSWORD_CHANGE_PER_USER: RateRule = RateRule {
        name: "password_change_user",
        limit: 5,
        window: MIN15,
    };
    pub const ALL: [RateRule; 13] = [
        PASSWORD_CHANGE_PER_USER,
        CHAT_START_PER_IP,
        CHAT_MESSAGE_PER_IP,
        CHECKOUT_PER_IP,
        MFA_PER_IP,
        MFA_PER_USER,
        LOGIN_PER_EMAIL,
        LOGIN_PER_IP,
        RESET_PER_EMAIL,
        RESET_PER_IP,
        REGISTER_PER_IP,
        VERIFY_PER_EMAIL,
        VERIFY_TOKEN_PER_IP,
    ];
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn email_parse_and_canonical() {
        let e = Email::parse("  Budi@Example.COM ").unwrap();
        assert_eq!(e.as_str(), "Budi@Example.COM");
        assert_eq!(e.canonical(), "budi@example.com");
        for bad in [
            "",
            "budi",
            "@x.id",
            "budi@",
            "budi@x",
            "budi@.x.id",
            "budi@x.id.",
            "a b@x.id",
            "a@b@x.id",
            "a\n@x.id",
            "a\u{0}b@x.id",
        ] {
            assert_eq!(Email::parse(bad), Err(AuthError::InvalidEmail), "{bad:?}");
        }
        // Whitespace di tepi (mis. newline dari form) di-trim, bukan ditolak.
        assert_eq!(Email::parse("a@x.id\n").unwrap().as_str(), "a@x.id");
        assert!(Email::parse(&format!("{}@x.id", "a".repeat(65))).is_err());
    }

    #[test]
    fn email_debug_hides_local_part() {
        let dbg = format!("{:?}", Email::parse("rahasia@x.id").unwrap());
        assert!(!dbg.contains("rahasia"), "{dbg}");
    }

    #[test]
    fn password_policy() {
        let e = Email::parse("budi@x.id").unwrap();
        assert!(validate_password("1234567", &e).is_err());
        assert!(validate_password("12345678", &e).is_ok());
        assert!(validate_password("        ", &e).is_err());
        assert!(validate_password("BUDI@X.ID", &e).is_err());
        assert!(validate_password(&"a".repeat(128), &e).is_ok());
        assert!(validate_password(&"a".repeat(129), &e).is_err());
        // Dihitung per karakter, bukan byte
        assert!(validate_password("ééééééé", &e).is_err());
        assert!(validate_password("éééééééé", &e).is_ok());
    }

    #[test]
    fn token_hash_debug_redacted() {
        assert_eq!(format!("{:?}", TokenHash([7; 32])), "TokenHash(***)");
    }

    #[test]
    fn rate_rule_names_are_redis_safe_and_unique() {
        let mut names: Vec<_> = rate_rules::ALL.iter().map(|r| r.name).collect();
        assert!(
            names
                .iter()
                .all(|n| n.bytes().all(|b| b.is_ascii_lowercase() || b == b'_'))
        );
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), rate_rules::ALL.len());
        assert!(
            rate_rules::ALL
                .iter()
                .all(|r| r.limit > 0 && !r.window.is_zero())
        );
    }

    #[test]
    fn reset_expiry_is_short() {
        let p = AuthPolicy::default();
        assert!(p.password_reset_ttl <= Duration::from_secs(3600));
        assert!(p.password_reset_ttl < p.email_verification_ttl);
    }
}
