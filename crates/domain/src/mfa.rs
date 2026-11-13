//! MFA TOTP (AUTH-011/012, keputusan Gate F 2026-11-12). Aturan murni: parameter RFC 6238,
//! base32 RFC 4648, format kode, jendela langkah waktu + anti-replay, dan kebijakan siapa
//! yang wajib MFA. Perhitungan HMAC ada di infrastructure.

use std::time::Duration;

/// RFC 6238: SHA-1, 6 digit, langkah 30 detik — kompatibel dengan aplikasi authenticator umum.
pub const DIGITS: u32 = 6;
pub const STEP_SECS: u64 = 30;
/// Toleransi jam: ±1 langkah.
pub const SKEW_STEPS: i64 = 1;
/// Panjang rahasia (160 bit, RFC 4226 §4 merekomendasikan ≥ 128, disarankan 160).
pub const SECRET_LEN: usize = 20;
pub const RECOVERY_CODES: usize = 10;
/// Kode pemulihan: 10 karakter base32 (50 bit), ditampilkan `xxxxx-xxxxx`.
pub const RECOVERY_LEN: usize = 10;
/// Tantangan login langkah kedua.
pub const CHALLENGE_TTL: Duration = Duration::from_secs(5 * 60);
pub const CHALLENGE_MAX_ATTEMPTS: i16 = 5;
pub const ISSUER: &str = "UMKM Business OS";

/// Role tenant yang dianggap admin untuk AUTH-012.
pub const ADMIN_ROLES: [&str; 3] = ["Owner", "Manager", "Admin"];
/// Role platform yang wajib MFA (AUTH-011).
pub const PLATFORM_MFA_ROLES: [&str; 1] = ["super_admin"];

const B32: &[u8; 32] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ234567";

/// Base32 RFC 4648 tanpa padding (format kunci manual authenticator).
pub fn base32_encode(data: &[u8]) -> String {
    let mut out = String::with_capacity(data.len().div_ceil(5) * 8);
    let (mut buf, mut bits) = (0u32, 0u32);
    for &b in data {
        buf = (buf << 8) | u32::from(b);
        bits += 8;
        while bits >= 5 {
            bits -= 5;
            out.push(B32[((buf >> bits) & 31) as usize] as char);
        }
    }
    if bits > 0 {
        out.push(B32[((buf << (5 - bits)) & 31) as usize] as char);
    }
    out
}

/// Kebalikan `base32_encode`; spasi/`=` diabaikan, huruf kecil diterima.
pub fn base32_decode(s: &str) -> Option<Vec<u8>> {
    let mut out = Vec::with_capacity(s.len() * 5 / 8);
    let (mut buf, mut bits) = (0u32, 0u32);
    for c in s.chars().filter(|c| !c.is_whitespace() && *c != '=') {
        let v = B32
            .iter()
            .position(|&b| b as char == c.to_ascii_uppercase())? as u32;
        buf = (buf << 5) | v;
        bits += 5;
        if bits >= 8 {
            bits -= 8;
            out.push((buf >> bits) as u8);
        }
    }
    Some(out)
}

/// Kode yang diketik pengguna.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Code {
    Totp(u32),
    /// Kode pemulihan ternormalisasi (huruf kecil, tanpa pemisah).
    Recovery(String),
}

/// Spasi dan `-` diabaikan. 6 digit = TOTP; 10 karakter base32 = kode pemulihan.
pub fn parse_code(input: &str) -> Option<Code> {
    let s: String = input
        .chars()
        .filter(|c| !c.is_whitespace() && *c != '-')
        .collect();
    if s.len() == DIGITS as usize && s.bytes().all(|b| b.is_ascii_digit()) {
        return s.parse().ok().map(Code::Totp);
    }
    let lower = s.to_ascii_lowercase();
    if lower.len() == RECOVERY_LEN && lower.bytes().all(|b| B32.contains(&b.to_ascii_uppercase())) {
        return Some(Code::Recovery(lower));
    }
    None
}

/// Bentuk tampilan kode pemulihan dari 10 karakter ternormalisasi.
pub fn format_recovery(normalized: &str) -> String {
    let (a, b) = normalized.split_at(normalized.len().min(5));
    format!("{a}-{b}")
}

pub fn step_at(unix_secs: u64) -> i64 {
    (unix_secs / STEP_SECS) as i64
}

/// Langkah yang boleh dicoba: jendela ±SKEW, hanya yang lebih baru dari langkah terakhir
/// yang diterima (kode yang sama tidak bisa dipakai dua kali — RFC 6238 §5.2).
pub fn candidate_steps(now_step: i64, last_used: Option<i64>) -> Vec<i64> {
    (now_step - SKEW_STEPS..=now_step + SKEW_STEPS)
        .filter(|s| last_used.is_none_or(|l| *s > l))
        .collect()
}

fn pct(s: &str) -> String {
    s.bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => {
                (b as char).to_string()
            }
            _ => format!("%{b:02X}"),
        })
        .collect()
}

/// URI `otpauth://` (Key Uri Format) untuk aplikasi authenticator.
pub fn otpauth_uri(account: &str, secret_b32: &str) -> String {
    format!(
        "otpauth://totp/{}:{}?secret={secret_b32}&issuer={}&algorithm=SHA1&digits={DIGITS}&period={STEP_SECS}",
        pct(ISSUER),
        pct(account),
        pct(ISSUER)
    )
}

/// AUTH-011: role platform yang wajib MFA sebelum sesi boleh dipakai.
pub fn platform_requires_mfa(platform_role: Option<&str>) -> bool {
    platform_role.is_some_and(|r| PLATFORM_MFA_ROLES.contains(&r))
}

/// AUTH-012: tenant mewajibkan MFA dan role anggota termasuk admin.
pub fn tenant_requires_mfa(require_admin_mfa: bool, role_name: &str) -> bool {
    require_admin_mfa && ADMIN_ROLES.contains(&role_name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base32_rfc4648_vectors() {
        for (raw, enc) in [
            ("", ""),
            ("f", "MY"),
            ("fo", "MZXQ"),
            ("foo", "MZXW6"),
            ("foob", "MZXW6YQ"),
            ("fooba", "MZXW6YTB"),
            ("foobar", "MZXW6YTBOI"),
        ] {
            assert_eq!(base32_encode(raw.as_bytes()), enc);
            assert_eq!(base32_decode(enc).unwrap(), raw.as_bytes());
        }
        assert_eq!(base32_decode("mzxw 6ytb oi==").unwrap(), b"foobar");
        assert!(base32_decode("MZ1").is_none());
    }

    #[test]
    fn parse_code_forms() {
        assert_eq!(parse_code(" 123 456 "), Some(Code::Totp(123_456)));
        assert_eq!(parse_code("012345"), Some(Code::Totp(12_345)));
        assert_eq!(
            parse_code("ABCDE-FGH23"),
            Some(Code::Recovery("abcdefgh23".into()))
        );
        assert_eq!(parse_code("12345"), None);
        assert_eq!(parse_code("1234567"), None);
        assert_eq!(parse_code("abcde-fgh21"), None, "1 bukan alfabet base32");
        assert_eq!(parse_code(""), None);
        assert_eq!(format_recovery("abcdefgh23"), "abcde-fgh23");
    }

    #[test]
    fn steps_window_and_replay() {
        assert_eq!(step_at(59), 1);
        assert_eq!(step_at(60), 2);
        assert_eq!(candidate_steps(100, None), vec![99, 100, 101]);
        assert_eq!(candidate_steps(100, Some(99)), vec![100, 101]);
        assert_eq!(candidate_steps(100, Some(101)), Vec::<i64>::new());
    }

    #[test]
    fn otpauth_uri_is_encoded() {
        let u = otpauth_uri("a+b@x.id", "JBSWY3DP");
        assert_eq!(
            u,
            "otpauth://totp/UMKM%20Business%20OS:a%2Bb%40x.id?secret=JBSWY3DP&issuer=UMKM%20Business%20OS&algorithm=SHA1&digits=6&period=30"
        );
    }

    #[test]
    fn policy() {
        assert!(platform_requires_mfa(Some("super_admin")));
        assert!(!platform_requires_mfa(Some("support")));
        assert!(!platform_requires_mfa(None));
        for r in ["Owner", "Manager", "Admin"] {
            assert!(tenant_requires_mfa(true, r));
            assert!(!tenant_requires_mfa(false, r));
        }
        for r in ["Support Agent", "Catalog Manager", "Viewer", "owner"] {
            assert!(!tenant_requires_mfa(true, r));
        }
    }
}
