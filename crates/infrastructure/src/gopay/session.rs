//! Sesi GoPay Merchant (GoBiz) per toko: token akses + refresh. Token TIDAK pernah di-`Debug`/log;
//! disimpan terenkripsi di repo (lihat crypto envelope). P10 integrasi gateway.

use serde::{Deserialize, Serialize};

/// Ambang refresh: token diperbarui bila sisa umur < 5 menit (sesuai gateway asal).
pub const REFRESH_SKEW_MS: i64 = 5 * 60 * 1000;

/// Masa berlaku token GoBiz yang diasumsikan (gateway asal memakai 24 jam).
pub const TOKEN_TTL_MS: i64 = 24 * 60 * 60 * 1000;

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

/// Pasangan token hasil token-exchange/refresh.
#[derive(Debug, Clone)]
pub struct TokenPair {
    pub access_token: String,
    pub refresh_token: String,
}
