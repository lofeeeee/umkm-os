//! MFA TOTP (AUTH-011/012): HOTP/TOTP RFC 4226/6238 (HMAC-SHA1) dan penyimpanan.
//! Rahasia TOTP dienkripsi envelope (ADR-0011, AAD terikat user); kode pemulihan dan token
//! tantangan hanya disimpan hash SHA-256-nya. Tabel global per user (bukan milik tenant).

use hmac::{Hmac, KeyInit, Mac};
use sha1::Sha1;
use sha2::{Digest, Sha256};
use sqlx::{PgPool, Postgres, Row, Transaction};
use umkm_domain::{
    auth::TokenHash,
    mfa::{self, Code},
};
use uuid::Uuid;

use crate::{
    auth::{hash_token, new_token},
    crypto::{self, Keyring, Sealed},
};

#[derive(Debug, thiserror::Error)]
pub enum MfaError {
    #[error("MFA sudah aktif")]
    AlreadyEnabled,
    #[error("MFA belum disiapkan")]
    NotSetUp,
    #[error("MFA belum aktif")]
    NotEnabled,
    #[error("kode tidak valid")]
    InvalidCode,
    #[error("tantangan tidak valid atau kedaluwarsa")]
    InvalidChallenge,
    #[error("kriptografi: {0}")]
    Crypto(#[from] crypto::CryptoError),
    #[error("acak tidak tersedia")]
    Random,
    #[error("database: {0}")]
    Db(#[from] sqlx::Error),
}

/// HOTP (RFC 4226 §5.3) dengan dynamic truncation, `DIGITS` digit.
pub fn hotp(key: &[u8], counter: u64) -> u32 {
    let mut m = <Hmac<Sha1> as KeyInit>::new_from_slice(key).expect("HMAC menerima kunci apa pun");
    m.update(&counter.to_be_bytes());
    let h = m.finalize().into_bytes();
    let off = (h[h.len() - 1] & 0x0f) as usize;
    let bin = u32::from_be_bytes([h[off] & 0x7f, h[off + 1], h[off + 2], h[off + 3]]);
    bin % 10u32.pow(mfa::DIGITS)
}

/// Langkah yang cocok di jendela (lebih baru dari `last_used`), atau `None`.
/// Semua kandidat dihitung (tanpa keluar lebih awal) agar waktu tidak membocorkan posisi.
pub fn match_totp(key: &[u8], code: u32, now_unix: u64, last_used: Option<i64>) -> Option<i64> {
    let mut found = None;
    for s in mfa::candidate_steps(mfa::step_at(now_unix), last_used) {
        let ok = u64::try_from(s).is_ok_and(|c| hotp(key, c) == code);
        if ok && found.is_none() {
            found = Some(s);
        }
    }
    found
}

fn random<const N: usize>() -> Result<[u8; N], MfaError> {
    let mut b = [0u8; N];
    getrandom::fill(&mut b).map_err(|_| MfaError::Random)?;
    Ok(b)
}

fn secret_aad(user_id: Uuid) -> String {
    format!("mfa-totp|{user_id}")
}

fn recovery_hash(user_id: Uuid, normalized: &str) -> [u8; 32] {
    Sha256::digest(format!("mfa-recovery|{user_id}|{normalized}").as_bytes()).into()
}

fn new_recovery_codes() -> Result<Vec<String>, MfaError> {
    (0..mfa::RECOVERY_CODES)
        .map(|_| {
            let raw: [u8; 7] = random()?;
            Ok(mfa::base32_encode(&raw)[..mfa::RECOVERY_LEN].to_ascii_lowercase())
        })
        .collect()
}

/// Email (label akun authenticator), hash password (konfirmasi saat mematikan MFA), dan
/// apakah role platform user wajib MFA.
pub struct Account {
    pub email: String,
    pub password_hash: String,
    pub platform_mfa_required: bool,
}

pub async fn account(pool: &PgPool, user_id: Uuid) -> Result<Option<Account>, MfaError> {
    let r = sqlx::query(
        "SELECT email, password_hash, platform_role FROM users WHERE id = $1 AND status = 'active'",
    )
    .bind(user_id)
    .fetch_optional(pool)
    .await?;
    Ok(r.map(|r| Account {
        email: r.get("email"),
        password_hash: r.get("password_hash"),
        platform_mfa_required: mfa::platform_requires_mfa(
            r.get::<Option<String>, _>("platform_role").as_deref(),
        ),
    }))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MfaStatus {
    pub enabled: bool,
    pub recovery_codes_remaining: i64,
}

pub async fn status(pool: &PgPool, user_id: Uuid) -> Result<MfaStatus, MfaError> {
    let r = sqlx::query(
        "SELECT EXISTS (SELECT 1 FROM mfa_credentials WHERE user_id = $1 AND kind = 'totp' AND confirmed_at IS NOT NULL) AS enabled,
                (SELECT count(*) FROM mfa_recovery_codes WHERE user_id = $1 AND used_at IS NULL) AS remaining",
    )
    .bind(user_id)
    .fetch_one(pool)
    .await?;
    Ok(MfaStatus {
        enabled: r.get("enabled"),
        recovery_codes_remaining: r.get("remaining"),
    })
}

/// Rahasia baru (belum aktif). Mengganti rahasia tertunda sebelumnya; ditolak bila MFA aktif.
/// Mengembalikan rahasia base32 untuk ditampilkan SEKALI kepada pengguna.
pub async fn begin_setup(pool: &PgPool, k: &Keyring, user_id: Uuid) -> Result<String, MfaError> {
    let secret: [u8; mfa::SECRET_LEN] = random()?;
    let sealed = crypto::seal(k, &secret, secret_aad(user_id).as_bytes())?;
    let n = sqlx::query(
        "INSERT INTO mfa_credentials (user_id, kind, encrypted_secret, kek_version, wrapped_dek)
         VALUES ($1, 'totp', $2, $3, $4)
         ON CONFLICT (user_id, kind) DO UPDATE
            SET encrypted_secret = EXCLUDED.encrypted_secret, kek_version = EXCLUDED.kek_version,
                wrapped_dek = EXCLUDED.wrapped_dek, last_used_step = NULL, created_at = now()
          WHERE mfa_credentials.confirmed_at IS NULL",
    )
    .bind(user_id)
    .bind(&sealed.ciphertext)
    .bind(sealed.kek_version)
    .bind(&sealed.wrapped_dek)
    .execute(pool)
    .await?
    .rows_affected();
    if n == 0 {
        return Err(MfaError::AlreadyEnabled);
    }
    Ok(mfa::base32_encode(&secret))
}

struct Cred {
    id: Uuid,
    sealed: Sealed,
    last_used: Option<i64>,
    confirmed: bool,
}

async fn lock_cred(
    tx: &mut Transaction<'_, Postgres>,
    user_id: Uuid,
) -> Result<Option<Cred>, sqlx::Error> {
    let r = sqlx::query(
        "SELECT id, encrypted_secret, kek_version, wrapped_dek, last_used_step, confirmed_at IS NOT NULL AS confirmed
         FROM mfa_credentials WHERE user_id = $1 AND kind = 'totp' FOR UPDATE",
    )
    .bind(user_id)
    .fetch_optional(&mut **tx)
    .await?;
    Ok(r.map(|r| Cred {
        id: r.get("id"),
        sealed: Sealed {
            kek_version: r.get("kek_version"),
            wrapped_dek: r.get("wrapped_dek"),
            ciphertext: r.get("encrypted_secret"),
        },
        last_used: r.get("last_used_step"),
        confirmed: r.get("confirmed"),
    }))
}

/// Cocokkan TOTP terhadap kredensial terkunci dan catat langkahnya (anti-replay).
async fn accept_totp(
    tx: &mut Transaction<'_, Postgres>,
    k: &Keyring,
    user_id: Uuid,
    cred: &Cred,
    code: u32,
    now_unix: u64,
) -> Result<bool, MfaError> {
    let secret = crypto::open(k, &cred.sealed, secret_aad(user_id).as_bytes())?;
    let Some(step) = match_totp(secret.expose(), code, now_unix, cred.last_used) else {
        return Ok(false);
    };
    sqlx::query("UPDATE mfa_credentials SET last_used_step = $2 WHERE id = $1")
        .bind(cred.id)
        .bind(step)
        .execute(&mut **tx)
        .await?;
    Ok(true)
}

async fn insert_recovery_codes(
    tx: &mut Transaction<'_, Postgres>,
    user_id: Uuid,
) -> Result<Vec<String>, MfaError> {
    sqlx::query("DELETE FROM mfa_recovery_codes WHERE user_id = $1")
        .bind(user_id)
        .execute(&mut **tx)
        .await?;
    let codes = new_recovery_codes()?;
    for c in &codes {
        sqlx::query("INSERT INTO mfa_recovery_codes (user_id, code_hash) VALUES ($1, $2)")
            .bind(user_id)
            .bind(&recovery_hash(user_id, c)[..])
            .execute(&mut **tx)
            .await?;
    }
    Ok(codes.iter().map(|c| mfa::format_recovery(c)).collect())
}

/// Aktifkan MFA dengan kode dari authenticator. Sesi ini ditandai lolos MFA; sesi lain user
/// dicabut (belum melewati langkah kedua). Mengembalikan kode pemulihan (ditampilkan sekali).
pub async fn enable(
    pool: &PgPool,
    k: &Keyring,
    user_id: Uuid,
    session_id: Uuid,
    code: u32,
    now_unix: u64,
) -> Result<Vec<String>, MfaError> {
    let mut tx = pool.begin().await?;
    let cred = lock_cred(&mut tx, user_id)
        .await?
        .ok_or(MfaError::NotSetUp)?;
    if cred.confirmed {
        return Err(MfaError::AlreadyEnabled);
    }
    if !accept_totp(&mut tx, k, user_id, &cred, code, now_unix).await? {
        return Err(MfaError::InvalidCode);
    }
    sqlx::query("UPDATE mfa_credentials SET confirmed_at = now() WHERE id = $1")
        .bind(cred.id)
        .execute(&mut *tx)
        .await?;
    let codes = insert_recovery_codes(&mut tx, user_id).await?;
    sqlx::query("UPDATE user_sessions SET mfa_verified_at = now() WHERE id = $1 AND user_id = $2")
        .bind(session_id)
        .bind(user_id)
        .execute(&mut *tx)
        .await?;
    sqlx::query(
        "UPDATE user_sessions SET revoked_at = now()
         WHERE user_id = $1 AND id <> $2 AND revoked_at IS NULL",
    )
    .bind(user_id)
    .bind(session_id)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(codes)
}

/// Cara langkah kedua yang berhasil.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Method {
    Totp,
    Recovery,
}

impl Method {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Totp => "totp",
            Self::Recovery => "recovery_code",
        }
    }
}

/// Verifikasi kode (TOTP atau kode pemulihan sekali pakai) untuk MFA yang aktif.
async fn verify_in_tx(
    tx: &mut Transaction<'_, Postgres>,
    k: &Keyring,
    user_id: Uuid,
    code: &Code,
    now_unix: u64,
) -> Result<Option<Method>, MfaError> {
    let cred = lock_cred(tx, user_id)
        .await?
        .filter(|c| c.confirmed)
        .ok_or(MfaError::NotEnabled)?;
    match code {
        Code::Totp(c) => Ok(accept_totp(tx, k, user_id, &cred, *c, now_unix)
            .await?
            .then_some(Method::Totp)),
        Code::Recovery(r) => {
            let used = sqlx::query(
                "UPDATE mfa_recovery_codes SET used_at = now()
                 WHERE user_id = $1 AND code_hash = $2 AND used_at IS NULL",
            )
            .bind(user_id)
            .bind(&recovery_hash(user_id, r)[..])
            .execute(&mut **tx)
            .await?
            .rows_affected();
            Ok((used == 1).then_some(Method::Recovery))
        }
    }
}

/// Matikan MFA (butuh kode valid; password dicek pemanggil). Kode pemulihan ikut dihapus.
pub async fn disable(
    pool: &PgPool,
    k: &Keyring,
    user_id: Uuid,
    code: &Code,
    now_unix: u64,
) -> Result<(), MfaError> {
    let mut tx = pool.begin().await?;
    if verify_in_tx(&mut tx, k, user_id, code, now_unix)
        .await?
        .is_none()
    {
        return Err(MfaError::InvalidCode);
    }
    sqlx::query("DELETE FROM mfa_credentials WHERE user_id = $1 AND kind = 'totp'")
        .bind(user_id)
        .execute(&mut *tx)
        .await?;
    sqlx::query("DELETE FROM mfa_recovery_codes WHERE user_id = $1")
        .bind(user_id)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(())
}

/// Ganti seluruh kode pemulihan (butuh TOTP valid).
pub async fn regenerate_recovery(
    pool: &PgPool,
    k: &Keyring,
    user_id: Uuid,
    code: u32,
    now_unix: u64,
) -> Result<Vec<String>, MfaError> {
    let mut tx = pool.begin().await?;
    if verify_in_tx(&mut tx, k, user_id, &Code::Totp(code), now_unix)
        .await?
        .is_none()
    {
        return Err(MfaError::InvalidCode);
    }
    let codes = insert_recovery_codes(&mut tx, user_id).await?;
    tx.commit().await?;
    Ok(codes)
}

/// Password benar + MFA aktif → token tantangan (bukan sesi). Tantangan lama user dihapus.
pub async fn create_challenge(pool: &PgPool, user_id: Uuid) -> Result<String, MfaError> {
    let (token, hash) = new_token().map_err(|_| MfaError::Random)?;
    let mut tx = pool.begin().await?;
    sqlx::query(
        "DELETE FROM mfa_challenges WHERE user_id = $1 OR expires_at < now() - interval '1 day'",
    )
    .bind(user_id)
    .execute(&mut *tx)
    .await?;
    sqlx::query(
        "INSERT INTO mfa_challenges (user_id, token_hash, expires_at)
         VALUES ($1, $2, now() + make_interval(secs => $3))",
    )
    .bind(user_id)
    .bind(&hash.0[..])
    .bind(mfa::CHALLENGE_TTL.as_secs_f64())
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(token)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MfaLogin {
    pub user_id: Uuid,
    pub session_id: Uuid,
    pub session_token: String,
    pub method: Method,
}

/// Selesaikan login: tantangan valid (belum dipakai, belum kedaluwarsa, percobaan < batas) +
/// kode benar → sesi baru bertanda MFA. Kode salah menambah percobaan; mencapai batas
/// membatalkan tantangan (harus login ulang dengan password).
pub async fn complete_login(
    pool: &PgPool,
    k: &Keyring,
    challenge_token: &str,
    code: &Code,
    now_unix: u64,
    session_ttl: std::time::Duration,
    user_agent: Option<&str>,
) -> Result<MfaLogin, MfaError> {
    let hash: TokenHash = hash_token(challenge_token);
    let mut tx = pool.begin().await?;
    let row = sqlx::query(
        "SELECT c.id, c.user_id FROM mfa_challenges c JOIN users u ON u.id = c.user_id
         WHERE c.token_hash = $1 AND c.consumed_at IS NULL AND c.expires_at > now()
           AND c.attempts < $2 AND u.status = 'active'
         FOR UPDATE OF c",
    )
    .bind(&hash.0[..])
    .bind(mfa::CHALLENGE_MAX_ATTEMPTS)
    .fetch_optional(&mut *tx)
    .await?
    .ok_or(MfaError::InvalidChallenge)?;
    let (challenge_id, user_id): (Uuid, Uuid) = (row.get("id"), row.get("user_id"));
    let method = match verify_in_tx(&mut tx, k, user_id, code, now_unix).await {
        Ok(Some(m)) => m,
        Ok(None) => {
            // Verifikasi gagal tidak mengubah apa pun; percobaan dicatat lalu di-commit.
            sqlx::query("UPDATE mfa_challenges SET attempts = attempts + 1 WHERE id = $1")
                .bind(challenge_id)
                .execute(&mut *tx)
                .await?;
            tx.commit().await?;
            return Err(MfaError::InvalidCode);
        }
        Err(MfaError::NotEnabled) => return Err(MfaError::InvalidChallenge),
        Err(e) => return Err(e),
    };
    sqlx::query("UPDATE mfa_challenges SET consumed_at = now() WHERE id = $1")
        .bind(challenge_id)
        .execute(&mut *tx)
        .await?;
    let (session_token, session_hash) = new_token().map_err(|_| MfaError::Random)?;
    let ua = user_agent.map(|s| s.chars().take(512).collect::<String>());
    let session_id: Uuid = sqlx::query_scalar(
        "INSERT INTO user_sessions (user_id, token_hash, expires_at, user_agent, mfa_verified_at)
         VALUES ($1, $2, now() + make_interval(secs => $3), $4, now()) RETURNING id",
    )
    .bind(user_id)
    .bind(&session_hash.0[..])
    .bind(session_ttl.as_secs_f64())
    .bind(ua)
    .fetch_one(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(MfaLogin {
        user_id,
        session_id,
        session_token,
        method,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// RFC 6238 Lampiran B (SHA-1, kunci "12345678901234567890"), 6 digit terakhir.
    #[test]
    fn rfc6238_vectors() {
        let key = b"12345678901234567890";
        for (t, want) in [
            (59u64, 287_082u32),
            (1_111_111_109, 81_804),
            (1_111_111_111, 50_471),
            (1_234_567_890, 5_924),
            (2_000_000_000, 279_037),
            (20_000_000_000, 353_130),
        ] {
            assert_eq!(hotp(key, t / 30), want, "t={t}");
        }
    }

    /// RFC 4226 Lampiran D.
    #[test]
    fn rfc4226_vectors() {
        let key = b"12345678901234567890";
        let want = [
            755_224u32, 287_082, 359_152, 969_429, 338_314, 254_676, 287_922, 162_583, 399_871,
            520_489,
        ];
        for (i, w) in want.iter().enumerate() {
            assert_eq!(hotp(key, i as u64), *w);
        }
    }

    #[test]
    fn window_and_replay() {
        let key = b"12345678901234567890";
        let now = 1_111_111_111u64;
        let step = mfa::step_at(now);
        let prev = hotp(key, (step - 1) as u64);
        let next = hotp(key, (step + 1) as u64);
        let far = hotp(key, (step + 2) as u64);
        assert_eq!(match_totp(key, prev, now, None), Some(step - 1));
        assert_eq!(match_totp(key, next, now, None), Some(step + 1));
        assert_eq!(match_totp(key, far, now, None), None, "di luar ±1 langkah");
        // Sudah dipakai: langkah yang sama/lebih lama ditolak.
        assert_eq!(match_totp(key, prev, now, Some(step - 1)), None);
        assert_eq!(match_totp(key, next, now, Some(step)), Some(step + 1));
    }

    #[test]
    fn recovery_codes_are_distinct_base32() {
        let c = new_recovery_codes().unwrap();
        assert_eq!(c.len(), mfa::RECOVERY_CODES);
        let set: std::collections::BTreeSet<_> = c.iter().collect();
        assert_eq!(set.len(), c.len());
        for x in &c {
            assert!(matches!(mfa::parse_code(x), Some(Code::Recovery(n)) if &n == x));
        }
        // Hash terikat user.
        let (a, b) = (Uuid::new_v4(), Uuid::new_v4());
        assert_ne!(recovery_hash(a, &c[0]), recovery_hash(b, &c[0]));
    }
}
