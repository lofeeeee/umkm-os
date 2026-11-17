//! Klien GoBiz / GoPay Merchant (tidak resmi): login OTP, refresh token, ambil transaksi. P10.
//!
//! Dibangun ulang dari gateway Node (`login.js`, `sessionManager.js`, `server.js`) yang di-deobfuscate
//! lalu diaudit (hanya menghubungi `api.gobiz.co.id` + `api.gojekapi.com`). Semua panggilan keluar
//! lewat [`Egress`] (kontrol SSRF). Fungsi pembangun-request & parser dipisah agar bisa diuji tanpa
//! jaringan (pola sama seperti `ai::provider_adapter`).
//!
//! Catatan: ini API privat milik aplikasi pihak ketiga; dapat berubah/diblokir sewaktu-waktu dan
//! tunduk pada ToS GoBiz. Dipakai hanya atas kehendak pemilik toko (ADR-0020).

use serde_json::{Value, json};
use umkm_domain::commerce::gopay::{GopayTxn, TxnStatus};

use super::session::{GopaySession, TokenPair, token_expiry_ms};
use super::time::{iso8601_utc, parse_iso8601_ms};
use crate::http_client::{Egress, Header, SsrfError};

const GOBIZ: &str = "https://api.gobiz.co.id";
const TXN_URL: &str = "https://api.gojekapi.com/merchant-analytics/v2/merchants/transactions";
const CLIENT_ID: &str = "go-biz-web-new";
const UA: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 \
     (KHTML, like Gecko) Chrome/150.0.0.0 Safari/537.36";
const ORIGIN: &str = "https://portal.gofoodmerchant.co.id";
const REFERER: &str = "https://portal.gofoodmerchant.co.id/";

#[derive(Debug, thiserror::Error)]
pub enum GobizError {
    #[error("jaringan ke GoBiz gagal")]
    Egress(#[from] SsrfError),
    #[error("sesi GoBiz kedaluwarsa (401)")]
    Unauthorized,
    #[error("GoBiz membalas status {0}")]
    Status(u16),
    #[error("respons GoBiz tidak dapat diurai: {0}")]
    Parse(&'static str),
    #[error("nomor HP tidak valid")]
    BadPhone,
}

impl GobizError {
    /// GoBiz menolak kredensial secara definitif (token dicabut/kedaluwarsa, refresh token tidak sah):
    /// hanya ini yang membuat sesi perlu login ulang. Gangguan jaringan, 5xx, 429, dan respons yang
    /// tak terurai dianggap sementara — sesi tetap aktif dan dicoba lagi siklus berikutnya.
    pub fn is_auth_rejected(&self) -> bool {
        matches!(self, Self::Unauthorized | Self::Status(400 | 403))
    }

    /// Kode singkat untuk dicatat & ditampilkan (tanpa isi token).
    pub fn code(&self) -> String {
        match self {
            Self::Egress(_) => "network".into(),
            Self::Unauthorized => "unauthorized".into(),
            Self::Status(s) => format!("status_{s}"),
            Self::Parse(_) => "parse".into(),
            Self::BadPhone => "bad_phone".into(),
        }
    }
}

/// Tantangan OTP yang dikembalikan GoBiz.
#[derive(Debug, Clone)]
pub struct OtpChallenge {
    pub otp_token: String,
    pub otp_length: u8,
    pub expires_in_secs: i64,
}

// ---- Fungsi murni: nomor HP ----

/// Normalisasi input nomor jadi (lokal tanpa kode negara, E.164). `08xx`/`62xx`/`+62xx` → `("8xx","+628xx")`.
pub fn normalize_phone(input: &str) -> Result<(String, String), GobizError> {
    let digits: String = input.chars().filter(char::is_ascii_digit).collect();
    let local = digits
        .strip_prefix("62")
        .map(str::to_string)
        .or_else(|| digits.strip_prefix('0').map(str::to_string))
        .unwrap_or(digits);
    if local.len() < 8 || local.len() > 13 {
        return Err(GobizError::BadPhone);
    }
    let e164 = format!("+62{local}");
    Ok((local, e164))
}

/// Ekstrak (access_token, refresh_token) dari teks bebas yang ditempel pemilik: string cookie
/// (`access_token=..; refresh_token=..`), potongan JSON, atau dua baris. P10: jalur "sesi via
/// browser" karena login OTP GoBiz kini terkunci reCAPTCHA.
pub fn parse_session_blob(blob: &str) -> Option<(String, String)> {
    fn grab(b: &str, key: &str) -> Option<String> {
        let idx = b.find(key)?;
        let rest = b[idx + key.len()..]
            .trim_start_matches(|c: char| c == '"' || c == ':' || c == '=' || c.is_whitespace());
        let tok: String = rest
            .chars()
            .take_while(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-'))
            .collect();
        (tok.len() >= 10).then_some(tok)
    }
    Some((grab(blob, "access_token")?, grab(blob, "refresh_token")?))
}

// ---- Fungsi murni: header ----

fn web_headers<'a>(uniqueid: &'a str) -> Vec<Header<'a>> {
    vec![
        Header::plain("accept", "application/json, text/plain, */*"),
        Header::plain("accept-language", "id"),
        Header::plain("authentication-type", "go-id"),
        Header::plain("content-type", "application/json"),
        Header::plain("gojek-country-code", "ID"),
        Header::plain("gojek-timezone", "Asia/Jakarta"),
        Header::plain("origin", ORIGIN),
        Header::plain("referer", REFERER),
        Header::plain("user-agent", UA),
        Header::plain("x-appid", "go-biz-web-dashboard"),
        Header::plain("x-appversion", "platform-v3.111.0-1708bc9a"),
        Header::plain("x-deviceos", "Web"),
        Header::plain("x-platform", "Web"),
        Header::plain("x-uniqueid", uniqueid),
        Header::plain("x-user-locale", "en-GB"),
        Header::plain("x-user-type", "merchant"),
    ]
}

// ---- Fungsi murni: parser ----

/// Kode galat singkat dari badan respons GoBiz (`errors[0].code`, `error`, atau `code`), disaring
/// ke `[A-Za-z0-9_.-]` maks 40 karakter — aman dicatat ke log.
fn error_code(body: &[u8]) -> Option<String> {
    let v: Value = serde_json::from_slice(body).ok()?;
    let raw = v
        .get("errors")
        .and_then(|e| e.get(0))
        .and_then(|e| e.get("code"))
        .or_else(|| v.get("error"))
        .or_else(|| v.get("code"))?;
    let s = match raw {
        Value::String(s) => s.clone(),
        Value::Number(n) => n.to_string(),
        _ => return None,
    };
    let clean: String = s
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '.' | '-'))
        .take(40)
        .collect();
    (!clean.is_empty()).then_some(clean)
}

fn deep<'a>(v: &'a Value, key: &str) -> Option<&'a Value> {
    v.get(key)
        .or_else(|| v.get("data").and_then(|d| d.get(key)))
}

fn parse_token(v: &Value) -> Result<TokenPair, GobizError> {
    let access = deep(v, "access_token")
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
        .ok_or(GobizError::Parse("access_token hilang"))?;
    let refresh = deep(v, "refresh_token")
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
        .map(str::to_string);
    let expires_in_secs = deep(v, "expires_in").and_then(|x| {
        x.as_i64()
            .or_else(|| x.as_str().and_then(|s| s.trim().parse().ok()))
    });
    Ok(TokenPair {
        access_token: access.to_string(),
        refresh_token: refresh,
        expires_in_secs,
    })
}

fn parse_otp(v: &Value) -> Result<OtpChallenge, GobizError> {
    let token = deep(v, "otp_token")
        .and_then(Value::as_str)
        .ok_or(GobizError::Parse("otp_token hilang"))?;
    let data = v.get("data").unwrap_or(v);
    Ok(OtpChallenge {
        otp_token: token.to_string(),
        otp_length: data
            .get("otp_length")
            .and_then(Value::as_u64)
            .unwrap_or(4)
            .clamp(4, 8) as u8,
        expires_in_secs: data
            .get("otp_expires_in")
            .and_then(Value::as_i64)
            .unwrap_or(720),
    })
}

/// (merchant_id, outlet_name) dari `/goresto/v5/public/users/config`.
fn parse_merchant(v: &Value) -> (Option<String>, Option<String>) {
    let m = deep(v, "merchant");
    let id = m
        .and_then(|m| m.get("id"))
        .and_then(Value::as_str)
        .map(str::to_string);
    let name = m
        .and_then(|m| m.get("outlet_name"))
        .and_then(Value::as_str)
        .map(str::to_string);
    (id, name)
}

fn as_rupiah(v: &Value) -> Option<u64> {
    match v {
        Value::Number(n) => {
            if let Some(u) = n.as_u64() {
                // GoBiz API mengembalikan nominal dalam sen (dikali 100, mis. 156900 sen = Rp 1.569).
                if u >= 100 && u % 100 == 0 {
                    return Some(u / 100);
                }
                return Some(u);
            }
            if let Some(f) = n.as_f64() {
                if f >= 100.0 && (f % 100.0).abs() < 1e-4 {
                    return Some((f / 100.0).round() as u64);
                }
                return Some(f.round() as u64);
            }
            None
        }
        Value::String(s) => {
            let s = s.trim();
            if let Ok(u) = s.parse::<u64>() {
                if u >= 100 && u % 100 == 0 {
                    return Some(u / 100);
                }
                return Some(u);
            }
            if let Ok(f) = s.parse::<f64>() {
                if f >= 100.0 && (f % 100.0).abs() < 1e-4 {
                    return Some((f / 100.0).round() as u64);
                }
                return Some(f.round() as u64);
            }
            None
        }
        _ => None,
    }
}

/// Normalisasi daftar transaksi dari respons merchant-analytics ke [`GopayTxn`].
pub fn parse_txns(v: &Value) -> Vec<GopayTxn> {
    let arr = v
        .get("transactions")
        .or_else(|| v.get("data").and_then(|d| d.get("transactions")))
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();

    arr.iter()
        .filter_map(|t| {
            let amount = t
                .get("gross_amount")
                .or_else(|| t.get("real_gross_amount"))
                .and_then(as_rupiah)?;
            let status = t
                .get("transaction_status")
                .and_then(Value::as_str)
                .map(TxnStatus::parse)
                .unwrap_or(TxnStatus::Other);
            let time = t
                .get("transaction_time")
                .or_else(|| t.get("settlement_time"))
                .and_then(Value::as_str)
                .and_then(parse_iso8601_ms)
                .unwrap_or(0);
            let id = t
                .get("id")
                .or_else(|| t.get("order_id"))
                .and_then(Value::as_str)?
                .to_string();
            let issuer = t
                .get("qris_provider_aspi_issuer")
                .and_then(Value::as_str)
                .unwrap_or("GoPay / Bank")
                .to_string();
            let order_id = t
                .get("order_id")
                .and_then(Value::as_str)
                .map(str::to_string);
            Some(GopayTxn {
                id,
                amount_rupiah: amount,
                status,
                time_ms: time,
                issuer,
                order_id,
            })
        })
        .collect()
}

fn txn_url(merchant_id: &str, start_ms: i64, end_ms: i64, size: u32) -> Result<String, GobizError> {
    reqwest::Url::parse_with_params(
        TXN_URL,
        &[
            ("from", "0".to_string()),
            ("size", size.to_string()),
            (
                "statuses",
                "SETTLEMENT,CAPTURE,REFUND,PARTIAL_REFUND".into(),
            ),
            ("payment_types", "QRIS,GOPAY".into()),
            ("start_time", iso8601_utc(start_ms)),
            ("end_time", iso8601_utc(end_ms)),
            ("merchant_ids", merchant_id.to_string()),
        ],
    )
    .map(|u| u.to_string())
    .map_err(|_| GobizError::Parse("url transaksi"))
}

// ---- IO ----

/// Klien ber-sesi. Klon murah (`Egress` berbagi semaphore).
#[derive(Debug, Clone)]
pub struct GobizClient {
    egress: Egress,
}

impl GobizClient {
    pub fn new(egress: Egress) -> Self {
        Self { egress }
    }

    fn uniqueid() -> String {
        uuid::Uuid::new_v4().to_string()
    }

    async fn status_check(resp_status: u16, body: &[u8]) -> Result<Value, GobizError> {
        match resp_status {
            200..=299 => serde_json::from_slice(body).map_err(|_| GobizError::Parse("bukan JSON")),
            401 => Err(GobizError::Unauthorized),
            s => {
                // Kode galat GoBiz (mis. `invalid_grant`) hanya ke log, untuk diagnosis; isi respons
                // & token tidak pernah dicatat.
                tracing::warn!(
                    status = s,
                    gobiz_code = error_code(body).as_deref().unwrap_or("-"),
                    "gopay: GoBiz menolak permintaan"
                );
                Err(GobizError::Status(s))
            }
        }
    }

    /// Uji apakah access token tersimpan masih diterima GoBiz (GET transaksi 1 menit terakhir, 1 baris).
    /// Tidak memakai refresh token, jadi aman dipanggil kapan saja.
    pub async fn probe(&self, session: &GopaySession, now_ms: i64) -> Result<(), GobizError> {
        self.fetch_transactions(session, now_ms - 60_000, now_ms, 1)
            .await
            .map(|_| ())
    }

    /// Minta OTP ke nomor HP merchant (lokal tanpa kode negara).
    pub async fn request_otp(&self, phone_local: &str) -> Result<OtpChallenge, GobizError> {
        let uid = Self::uniqueid();
        let body =
            json!({"client_id": CLIENT_ID, "phone_number": phone_local, "country_code": "62"});
        let resp = self
            .egress
            .request(
                reqwest::Method::POST,
                &format!("{GOBIZ}/goid/login/request"),
                &web_headers(&uid),
                Some(serde_json::to_vec(&body).unwrap()),
            )
            .await?;
        let v = Self::status_check(resp.status, &resp.body).await?;
        parse_otp(&v)
    }

    /// Verifikasi OTP → sesi (termasuk ambil merchant_id/outlet). `now_ms` untuk hitung kedaluwarsa.
    pub async fn verify_otp(
        &self,
        phone_e164: &str,
        otp: &str,
        otp_token: &str,
        now_ms: i64,
    ) -> Result<GopaySession, GobizError> {
        let uid = Self::uniqueid();
        let body = json!({
            "client_id": CLIENT_ID,
            "grant_type": "otp",
            "data": {"otp": otp, "otp_token": otp_token}
        });
        let resp = self
            .egress
            .request(
                reqwest::Method::POST,
                &format!("{GOBIZ}/goid/token"),
                &web_headers(&uid),
                Some(serde_json::to_vec(&body).unwrap()),
            )
            .await?;
        let v = Self::status_check(resp.status, &resp.body).await?;
        let token = parse_token(&v)?;
        let refresh_token = token
            .refresh_token
            .ok_or(GobizError::Parse("refresh_token hilang"))?;

        // Ambil info merchant/outlet (best-effort; gagal tidak membatalkan login).
        let (merchant_id, outlet_name) = self.fetch_merchant(&token.access_token).await;

        Ok(GopaySession {
            phone_e164: phone_e164.to_string(),
            merchant_id,
            outlet_name,
            expires_at_ms: token_expiry_ms(&token.access_token, token.expires_in_secs, now_ms),
            access_token: token.access_token,
            refresh_token,
        })
    }

    /// Bangun sesi dari token yang ditangkap di browser pemilik (login OTP terkunci captcha).
    /// Best-effort ambil merchant/outlet. Refresh + poll berikutnya memakai token ini.
    pub async fn session_from_tokens(
        &self,
        access_token: String,
        refresh_token: String,
        phone_e164: &str,
        now_ms: i64,
    ) -> GopaySession {
        let (merchant_id, outlet_name) = self.fetch_merchant(&access_token).await;
        GopaySession {
            phone_e164: phone_e164.to_string(),
            merchant_id,
            outlet_name,
            // Token tempelan bisa sudah berumur: pakai `exp` JWT-nya bila ada, jadi refresh pertama
            // terjadi tepat waktu alih-alih menunggu 401.
            expires_at_ms: token_expiry_ms(&access_token, None, now_ms),
            access_token,
            refresh_token,
        }
    }

    async fn fetch_merchant(&self, access_token: &str) -> (Option<String>, Option<String>) {
        let bearer = format!("Bearer {access_token}");
        let headers = vec![
            Header::sensitive("authorization", &bearer),
            Header::plain("authentication-type", "go-id"),
            Header::plain("origin", ORIGIN),
            Header::plain("referer", REFERER),
            Header::plain("user-agent", UA),
        ];
        match self
            .egress
            .request(
                reqwest::Method::GET,
                &format!("{GOBIZ}/goresto/v5/public/users/config"),
                &headers,
                None,
            )
            .await
        {
            Ok(r) if (200..=299).contains(&r.status) => serde_json::from_slice::<Value>(&r.body)
                .map(|v| parse_merchant(&v))
                .unwrap_or((None, None)),
            _ => (None, None),
        }
    }

    /// Perbarui token dengan refresh_token. `now_ms` untuk kedaluwarsa baru.
    pub async fn refresh(
        &self,
        session: &GopaySession,
        now_ms: i64,
    ) -> Result<GopaySession, GobizError> {
        let uid = Self::uniqueid();
        let (local, _) = normalize_phone(&session.phone_e164)?;
        let body = json!({
            "client_id": CLIENT_ID,
            "grant_type": "refresh_token",
            "data": {"refresh_token": session.refresh_token, "phone_number": local, "country_code": "62"}
        });
        let resp = self
            .egress
            .request(
                reqwest::Method::POST,
                &format!("{GOBIZ}/goid/token"),
                &web_headers(&uid),
                Some(serde_json::to_vec(&body).unwrap()),
            )
            .await?;
        let v = Self::status_check(resp.status, &resp.body).await?;
        let token = parse_token(&v)?;
        Ok(GopaySession {
            expires_at_ms: token_expiry_ms(&token.access_token, token.expires_in_secs, now_ms),
            access_token: token.access_token,
            // Tanpa rotasi: refresh token lama tetap berlaku.
            refresh_token: token
                .refresh_token
                .unwrap_or_else(|| session.refresh_token.clone()),
            ..session.clone()
        })
    }

    /// Ambil transaksi masuk dalam jendela [start_ms, end_ms]. 401 → [`GobizError::Unauthorized`].
    pub async fn fetch_transactions(
        &self,
        session: &GopaySession,
        start_ms: i64,
        end_ms: i64,
        size: u32,
    ) -> Result<Vec<GopayTxn>, GobizError> {
        let merchant_id = session.merchant_id.as_deref().unwrap_or("");
        let url = txn_url(merchant_id, start_ms, end_ms, size)?;
        let bearer = format!("Bearer {}", session.access_token);
        let cookie = session.cookie();
        let headers = vec![
            Header::sensitive("authorization", &bearer),
            Header::sensitive("cookie", &cookie),
            Header::plain("authentication-type", "go-id"),
            Header::plain("accept", "application/json, text/plain, */*"),
            Header::plain("origin", ORIGIN),
            Header::plain("referer", REFERER),
            Header::plain("user-agent", UA),
        ];
        let resp = self
            .egress
            .request(reqwest::Method::GET, &url, &headers, None)
            .await?;
        let v = Self::status_check(resp.status, &resp.body).await?;
        Ok(parse_txns(&v))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn phone_normalization() {
        assert_eq!(
            normalize_phone("081234567890").unwrap(),
            ("81234567890".into(), "+6281234567890".into())
        );
        assert_eq!(
            normalize_phone("+62 812-3456-7890").unwrap(),
            ("81234567890".into(), "+6281234567890".into())
        );
        assert_eq!(
            normalize_phone("6281234567890").unwrap(),
            ("81234567890".into(), "+6281234567890".into())
        );
        assert!(matches!(normalize_phone("123"), Err(GobizError::BadPhone)));
    }

    #[test]
    fn token_parse_flat_and_nested() {
        assert_eq!(
            parse_token(&json!({"access_token": "A", "refresh_token": "R"}))
                .unwrap()
                .access_token,
            "A"
        );
        let nested = json!({"data": {"access_token": "A2", "refresh_token": "R2"}});
        assert_eq!(
            parse_token(&nested).unwrap().refresh_token.as_deref(),
            Some("R2")
        );
        // Refresh tanpa rotasi: refresh_token boleh tidak ada; expires_in angka atau string.
        let no_rotate = parse_token(&json!({"access_token": "A", "expires_in": "3600"})).unwrap();
        assert_eq!(no_rotate.refresh_token, None);
        assert_eq!(no_rotate.expires_in_secs, Some(3600));
        assert!(parse_token(&json!({"refresh_token": "R"})).is_err());
        assert!(parse_token(&json!({"access_token": ""})).is_err());
    }

    #[test]
    fn error_classification() {
        assert!(GobizError::Unauthorized.is_auth_rejected());
        assert!(GobizError::Status(400).is_auth_rejected());
        assert!(GobizError::Status(403).is_auth_rejected());
        assert!(!GobizError::Status(429).is_auth_rejected());
        assert!(!GobizError::Status(503).is_auth_rejected());
        assert!(!GobizError::Parse("x").is_auth_rejected());
        assert_eq!(GobizError::Status(503).code(), "status_503");
    }

    #[test]
    fn otp_and_merchant_parse() {
        let otp = parse_otp(&json!({"data": {"otp_token": "T", "otp_length": 6}})).unwrap();
        assert_eq!(otp.otp_token, "T");
        assert_eq!(otp.otp_length, 6);

        let (id, name) =
            parse_merchant(&json!({"merchant": {"id": "M1", "outlet_name": "Warung"}}));
        assert_eq!(id.as_deref(), Some("M1"));
        assert_eq!(name.as_deref(), Some("Warung"));
    }

    #[test]
    fn txns_parse_string_or_number_amounts() {
        let v = json!({"transactions": [
            {"id": "t1", "gross_amount": "30217", "transaction_status": "SETTLEMENT",
             "transaction_time": "2023-11-14T22:13:20Z", "qris_provider_aspi_issuer": "BCA", "order_id": "o1"},
            {"id": "t2", "real_gross_amount": 1500000, "transaction_status": "capture",
             "settlement_time": "2023-11-14T22:14:00Z"},
            {"gross_amount": 1, "transaction_status": "settlement"} // tanpa id → di-skip
        ]});
        let txns = parse_txns(&v);
        assert_eq!(txns.len(), 2);
        assert_eq!(txns[0].amount_rupiah, 30_217);
        assert_eq!(txns[0].issuer, "BCA");
        assert_eq!(txns[0].time_ms, 1_700_000_000_000);
        assert_eq!(txns[1].amount_rupiah, 15_000);
        assert_eq!(txns[1].issuer, "GoPay / Bank");
    }

    #[test]
    fn error_code_is_short_and_sanitized() {
        assert_eq!(
            error_code(br#"{"errors":[{"code":"GoId-400 invalid_grant"}]}"#).as_deref(),
            Some("GoId-400invalid_grant")
        );
        assert_eq!(
            error_code(br#"{"error":"invalid_grant","error_description":"x"}"#).as_deref(),
            Some("invalid_grant")
        );
        assert_eq!(error_code(br#"{"code":400}"#).as_deref(), Some("400"));
        assert_eq!(error_code(b"<html>"), None);
        assert_eq!(
            error_code(br#"{"error":"<script>"}"#).as_deref(),
            Some("script")
        );
    }

    #[test]
    fn txn_url_encodes_params() {
        let u = txn_url("M1", 0, 1_000, 20).unwrap();
        assert!(u.contains("merchant_ids=M1"));
        assert!(u.contains("start_time=1970-01-01T00%3A00%3A00Z"));
        assert!(u.contains("size=20"));
    }

    #[test]
    fn parse_blob_from_cookie_json_lines() {
        let (a, r) = parse_session_blob(
            "access_token=AAAbbbCCC111; refresh_token=RRRdddEEE222; auth_method=goid",
        )
        .unwrap();
        assert_eq!(a, "AAAbbbCCC111");
        assert_eq!(r, "RRRdddEEE222");
        let (a2, r2) =
            parse_session_blob(r#"{"access_token":"tok.en-A_1","refresh_token":"ref.tok-B_2"}"#)
                .unwrap();
        assert_eq!(a2, "tok.en-A_1");
        assert_eq!(r2, "ref.tok-B_2");
        assert!(parse_session_blob("tidak ada token di sini").is_none());
        assert!(parse_session_blob("access_token=short").is_none());
    }

    #[test]
    fn as_rupiah_handles_sen_and_decimal() {
        // GoBiz API nyata mengembalikan real_gross_amount dalam sen (x100)
        assert_eq!(as_rupiah(&json!(156900)), Some(1569));
        assert_eq!(as_rupiah(&json!(3032700)), Some(30327));
        assert_eq!(as_rupiah(&json!("156900")), Some(1569));
        assert_eq!(as_rupiah(&json!("1569.00")), Some(1569));
        // Fallback bila format angka sudah rupiah bulat (mis. fixture tes lawas)
        assert_eq!(as_rupiah(&json!(30217)), Some(30217));
    }
}
