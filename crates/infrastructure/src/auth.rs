//! Adapter kripto autentikasi (P1-2, `PRD.md §6.1`): Argon2id + token opak.

use std::sync::OnceLock;

use argon2::{
    ARGON2ID_IDENT, Algorithm, Argon2, Params, PasswordHash, PasswordHasher as _,
    PasswordVerifier as _, Version, password_hash,
};
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use sha2::{Digest, Sha256};
use umkm_domain::auth::{AuthError, PasswordHasher, TokenHash};

/// Argon2id dengan parameter yang dapat ditingkatkan. Default = rekomendasi OWASP
/// (m = 19 MiB, t = 2, p = 1). Salt acak 16 byte per hash (bawaan PHC).
#[derive(Clone)]
pub struct Argon2idHasher {
    params: Params,
}

impl Argon2idHasher {
    pub fn new(m_cost_kib: u32, t_cost: u32, p_cost: u32) -> Result<Self, AuthError> {
        Params::new(m_cost_kib, t_cost, p_cost, None)
            .map(|params| Self { params })
            .map_err(|_| AuthError::Internal)
    }

    pub fn owasp() -> Self {
        Self::new(
            Params::DEFAULT_M_COST,
            Params::DEFAULT_T_COST,
            Params::DEFAULT_P_COST,
        )
        .expect("default params valid")
    }

    fn argon(&self) -> Argon2<'static> {
        Argon2::new(Algorithm::Argon2id, Version::V0x13, self.params.clone())
    }

    /// Verifikasi terhadap hash dummy dengan biaya yang sama. Dipanggil saat email
    /// tidak ditemukan agar waktu respons tidak membedakan akun ada/tidak.
    pub fn verify_dummy(&self, password: &str) {
        static DUMMY: OnceLock<String> = OnceLock::new();
        let hash = DUMMY.get_or_init(|| {
            self.hash("dummy-password-for-timing")
                .expect("hashing dummy")
        });
        let _ = self.verify(password, hash);
    }
}

impl PasswordHasher for Argon2idHasher {
    fn hash(&self, password: &str) -> Result<String, AuthError> {
        self.argon()
            .hash_password(password.as_bytes())
            .map(|h| h.to_string())
            .map_err(|_| AuthError::Internal)
    }

    fn verify(&self, password: &str, hash: &str) -> Result<bool, AuthError> {
        let parsed = PasswordHash::new(hash).map_err(|_| AuthError::Internal)?;
        if parsed.algorithm != ARGON2ID_IDENT {
            // Hanya Argon2id yang diterima (PRD.md §6.1).
            return Err(AuthError::Internal);
        }
        // Parameter verifikasi diambil dari string hash, bukan dari `self.params`,
        // sehingga hash lama tetap dapat diverifikasi setelah parameter dinaikkan.
        match self.argon().verify_password(password.as_bytes(), hash) {
            Ok(()) => Ok(true),
            Err(password_hash::Error::PasswordInvalid) => Ok(false),
            Err(_) => Err(AuthError::Internal),
        }
    }

    fn needs_rehash(&self, hash: &str) -> bool {
        let Ok(parsed) = PasswordHash::new(hash) else {
            return true;
        };
        if parsed.algorithm != ARGON2ID_IDENT {
            return true;
        }
        match Params::try_from(&parsed) {
            Ok(p) => {
                p.m_cost() < self.params.m_cost()
                    || p.t_cost() < self.params.t_cost()
                    || p.p_cost() < self.params.p_cost()
            }
            Err(_) => true,
        }
    }
}

/// Token opak 256-bit (base64url, 43 karakter) + hash SHA-256-nya.
/// Plaintext hanya dikirim ke user (cookie/email); yang disimpan hanya hash.
pub fn new_token() -> Result<(String, TokenHash), AuthError> {
    let mut bytes = [0u8; 32];
    getrandom::fill(&mut bytes).map_err(|_| AuthError::Internal)?;
    let token = URL_SAFE_NO_PAD.encode(bytes);
    let hash = hash_token(&token);
    Ok((token, hash))
}

pub fn hash_token(token: &str) -> TokenHash {
    TokenHash(Sha256::digest(token.as_bytes()).into())
}

/// Hash subjek rate-limit (email/IP) agar data pribadi tidak tersimpan apa adanya di Redis.
pub fn subject_digest(subject: &str) -> String {
    Sha256::digest(subject.as_bytes())
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use super::*;

    /// Parameter kecil agar tes cepat; perilaku sama.
    fn fast() -> Argon2idHasher {
        Argon2idHasher::new(1024, 1, 1).unwrap()
    }

    #[test]
    fn owasp_defaults() {
        let h = Argon2idHasher::owasp().hash("rahasia-123").unwrap();
        assert!(h.starts_with("$argon2id$v=19$m=19456,t=2,p=1$"), "{h}");
    }

    #[test]
    fn hash_verify_roundtrip_and_unique_salt() {
        let h = fast();
        let a = h.hash("rahasia-123").unwrap();
        let b = h.hash("rahasia-123").unwrap();
        assert_ne!(a, b, "salt harus unik per hash");
        assert!(!a.contains("rahasia-123"));
        assert!(h.verify("rahasia-123", &a).unwrap());
        assert!(!h.verify("rahasia-124", &a).unwrap());
        assert!(!h.verify("", &a).unwrap());
    }

    #[test]
    fn verify_rejects_non_argon2id_and_garbage() {
        let h = fast();
        assert_eq!(h.verify("x", "plaintext"), Err(AuthError::Internal));
        let argon2i = Argon2::new(
            Algorithm::Argon2i,
            Version::V0x13,
            Params::new(1024, 1, 1, None).unwrap(),
        )
        .hash_password(b"x")
        .unwrap()
        .to_string();
        assert_eq!(h.verify("x", &argon2i), Err(AuthError::Internal));
    }

    #[test]
    fn old_hash_still_verifies_but_needs_rehash_after_param_upgrade() {
        let old = fast().hash("rahasia-123").unwrap();
        let current = Argon2idHasher::new(2048, 2, 1).unwrap();
        assert!(current.verify("rahasia-123", &old).unwrap());
        assert!(current.needs_rehash(&old));
        let fresh = current.hash("rahasia-123").unwrap();
        assert!(!current.needs_rehash(&fresh));
        assert!(current.needs_rehash("garbage"));
    }

    #[test]
    fn tokens_are_random_url_safe_and_only_hash_is_derived() {
        let mut seen = HashSet::new();
        for _ in 0..1000 {
            let (t, h) = new_token().unwrap();
            assert_eq!(t.len(), 43);
            assert!(
                t.bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
            );
            assert_eq!(hash_token(&t), h);
            assert!(seen.insert(t));
        }
        let (t, h) = new_token().unwrap();
        assert_ne!(&h.0[..], t.as_bytes().get(..32).unwrap());
    }

    #[test]
    fn subject_digest_is_hex_sha256() {
        let d = subject_digest("budi@x.id");
        assert_eq!(d.len(), 64);
        assert!(d.bytes().all(|b| b.is_ascii_hexdigit()));
        assert_ne!(d, subject_digest("budi@x.ie"));
    }
}
