//! Sesi GoPay Merchant (GoBiz) per toko: token akses + refresh. Token TIDAK pernah di-`Debug`/log;
//! disimpan terenkripsi di repo (lihat crypto envelope). P10 integrasi gateway.

use base64::Engine;
use serde::{Deserialize, Serialize};

/// Ambang refresh: token diperbarui bila sisa umur < 30 menit. Lebih awal dari gateway asal (5 menit)
/// supaya satu-dua siklus worker yang gagal sementara tidak langsung membuat token kedaluwarsa.
pub const REFRESH_SKEW_MS: i64 = 30 * 60 * 1000;

/// Masa berlaku cadangan bila GoBiz tidak memberi `expires_in` dan token bukan JWT ber-`exp`
/// (gateway asal mengasumsikan 24 jam).
pub const TOKEN_TTL_MS: i64 = 24 * 60 * 60 * 1000;

/// Refresh penjaga (keep-alive): sesi diperbarui minimal tiap 6 jam walau token belum dekat
/// kedaluwarsa dan toko sepi pesanan — sama seperti gateway asal — agar refresh token tetap "hangat"
/// dan tidak kedaluwarsa karena lama tidak dipakai.
pub const KEEPALIVE_MS: i64 = 6 * 60 * 60 * 1000;

/// Klaim `exp` (epoch detik) dari JWT tanpa memverifikasi tanda tangan — hanya untuk menjadwalkan
/// refresh, tidak untuk keputusan keamanan. `None` bila token bukan JWT.
pub fn jwt_exp_ms(token: &str) -> Option<i64> {
    let payload = token.split('.').nth(1)?;
    let bytes = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(payload.trim_end_matches('='))
        .ok()?;
    let v: serde_json::Value = serde_json::from_slice(&bytes).ok()?;
    let exp = v.get("exp")?.as_i64()?;
    (exp > 0).then_some(exp.saturating_mul(1000))
}

/// Kapan token akses kedaluwarsa: `expires_in` dari GoBiz → klaim `exp` JWT → cadangan 24 jam.
/// Nilai yang sudah lewat tetap dikembalikan apa adanya (token tempelan yang basi langsung di-refresh).
pub fn token_expiry_ms(access_token: &str, expires_in_secs: Option<i64>, now_ms: i64) -> i64 {
    if let Some(s) = expires_in_secs.filter(|s| *s > 0) {
        return now_ms + s.saturating_mul(1000);
    }
    jwt_exp_ms(access_token).unwrap_or(now_ms + TOKEN_TTL_MS)
}

#[derive(Clone, Serialize, Deserialize)]
pub struct GopaySession {
    /// Nomor E.164, mis. `+62812...`.
    pub phone_e164: String,
    pub merchant_id: Option<String>,
    pub outlet_name: Option<String>,
    pub access_token: String,
    pub refresh_token: String,
    /// Epoch-ms kapan token kedaluwarsa.
    pub expires_at_ms: i64,
}

impl std::fmt::Debug for GopaySession {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("GopaySession")
            .field("phone_e164", &self.phone_e164)
            .field("merchant_id", &self.merchant_id)
            .field("outlet_name", &self.outlet_name)
            .field("access_token", &"<redacted>")
            .field("refresh_token", &"<redacted>")
            .field("expires_at_ms", &self.expires_at_ms)
            .finish()
    }
}

impl GopaySession {
    /// Token dianggap perlu diperbarui bila sisa umur < [`REFRESH_SKEW_MS`].
    pub fn needs_refresh(&self, now_ms: i64) -> bool {
        self.expires_at_ms - now_ms < REFRESH_SKEW_MS
    }

    /// Cookie gaya web GoBiz untuk header `Cookie`.
    pub fn cookie(&self) -> String {
        format!(
            "access_token={}; refresh_token={}; auth_method=goid",
            self.access_token, self.refresh_token
        )
    }
}

/// Hasil token-exchange/refresh. GoBiz biasanya merotasi refresh token, tetapi bila respons tidak
/// memuatnya refresh token lama tetap dipakai (bukan dianggap gagal).
#[derive(Debug, Clone)]
pub struct TokenPair {
    pub access_token: String,
    pub refresh_token: Option<String>,
    pub expires_in_secs: Option<i64>,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn jwt(payload: &str) -> String {
        let enc = base64::engine::general_purpose::URL_SAFE_NO_PAD;
        format!(
            "{}.{}.sig",
            enc.encode(r#"{"alg":"none"}"#),
            enc.encode(payload)
        )
    }

    #[test]
    fn expiry_prefers_expires_in_then_jwt_then_fallback() {
        let now = 1_000_000;
        assert_eq!(token_expiry_ms("opaque", Some(3600), now), now + 3_600_000);
        assert_eq!(
            token_expiry_ms(&jwt(r#"{"exp":2000}"#), None, now),
            2_000_000
        );
        assert_eq!(
            token_expiry_ms(&jwt(r#"{"exp":2000}"#), Some(0), now),
            2_000_000
        );
        assert_eq!(token_expiry_ms("opaque", None, now), now + TOKEN_TTL_MS);
        assert_eq!(
            token_expiry_ms(&jwt(r#"{"sub":"x"}"#), None, now),
            now + TOKEN_TTL_MS
        );
        assert_eq!(jwt_exp_ms("a.!!!.c"), None);
    }

    #[test]
    fn refresh_window() {
        let s = GopaySession {
            phone_e164: "+628123456789".into(),
            merchant_id: None,
            outlet_name: None,
            access_token: "a".into(),
            refresh_token: "r".into(),
            expires_at_ms: 10 * 60 * 60 * 1000,
        };
        assert!(!s.needs_refresh(0));
        assert!(s.needs_refresh(s.expires_at_ms - REFRESH_SKEW_MS + 1));
        assert!(s.needs_refresh(s.expires_at_ms + 1));
    }
}
