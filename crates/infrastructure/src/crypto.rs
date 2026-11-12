//! Envelope encryption rahasia (P5-2, `PRD.md §6.7`, ADR-0011).
//!
//! - Setiap rahasia punya DEK acak 32 byte; data dienkripsi XChaCha20-Poly1305 dengan DEK.
//! - DEK dibungkus (dienkripsi) dengan KEK berversi dari secret runtime `UMKM_DATA_KEK`
//!   (`v1:<base64>[,v2:<base64>...]`). Versi tertinggi dipakai untuk enkripsi baru.
//! - AAD mengikat ciphertext ke konteksnya (mis. `ai_credential|<tenant>|<id>`): ciphertext yang
//!   dipindah ke baris/tenant lain gagal didekripsi.
//! - Rotasi KEK: tambahkan versi baru lalu `rewrap` (DEK dibuka & dibungkus ulang; data tidak disentuh).
//! - Nilai rahasia tidak pernah di-`Debug`/log; hanya bentuk masked yang boleh keluar.

use std::collections::BTreeMap;

use base64::{Engine as _, engine::general_purpose::STANDARD};
use chacha20poly1305::{
    XChaCha20Poly1305, XNonce,
    aead::{Aead, KeyInit, Payload},
};

pub const KEK_ENV: &str = "UMKM_DATA_KEK";

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum CryptoError {
    #[error("format {KEK_ENV} tidak valid (harus v<n>:<base64 32 byte>[,...])")]
    InvalidKeyring,
    #[error("versi KEK {0} tidak tersedia")]
    UnknownKek(i16),
    #[error("dekripsi gagal (kunci/konteks salah atau data rusak)")]
    Decrypt,
    #[error("sumber acak OS gagal")]
    Random,
}

/// Kumpulan KEK berversi. `Debug` tidak menampilkan kunci.
#[derive(Clone)]
pub struct Keyring {
    keys: BTreeMap<i16, [u8; 32]>,
}

impl std::fmt::Debug for Keyring {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Keyring")
            .field("versions", &self.keys.keys().collect::<Vec<_>>())
            .finish()
    }
}

impl Keyring {
    pub fn parse(s: &str) -> Result<Self, CryptoError> {
        let mut keys = BTreeMap::new();
        for part in s.split(',').map(str::trim).filter(|p| !p.is_empty()) {
            let (v, b) = part.split_once(':').ok_or(CryptoError::InvalidKeyring)?;
            let v: i16 = v
                .strip_prefix('v')
                .and_then(|n| n.parse().ok())
                .filter(|n| *n > 0)
                .ok_or(CryptoError::InvalidKeyring)?;
            let raw = STANDARD
                .decode(b)
                .map_err(|_| CryptoError::InvalidKeyring)?;
            let key: [u8; 32] = raw.try_into().map_err(|_| CryptoError::InvalidKeyring)?;
            if key.iter().all(|x| *x == 0) || keys.insert(v, key).is_some() {
                return Err(CryptoError::InvalidKeyring);
            }
        }
        if keys.is_empty() {
            return Err(CryptoError::InvalidKeyring);
        }
        Ok(Self { keys })
    }

    pub fn current_version(&self) -> i16 {
        *self.keys.keys().next_back().expect("keyring tidak kosong")
    }

    fn kek(&self, v: i16) -> Result<XChaCha20Poly1305, CryptoError> {
        let k = self.keys.get(&v).ok_or(CryptoError::UnknownKek(v))?;
        Ok(XChaCha20Poly1305::new(k.into()))
    }
}

/// Rahasia terenkripsi siap disimpan.
#[derive(Clone, PartialEq, Eq)]
pub struct Sealed {
    pub kek_version: i16,
    /// nonce (24) || DEK terenkripsi (32 + tag 16)
    pub wrapped_dek: Vec<u8>,
    /// nonce (24) || ciphertext + tag
    pub ciphertext: Vec<u8>,
}

impl std::fmt::Debug for Sealed {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Sealed")
            .field("kek_version", &self.kek_version)
            .finish_non_exhaustive()
    }
}

/// Plaintext rahasia; tidak dapat di-`Debug`/`Display`, dihapus dari memori saat drop.
pub struct Secret(Vec<u8>);

impl Secret {
    pub fn expose(&self) -> &[u8] {
        &self.0
    }

    pub fn expose_str(&self) -> Result<&str, CryptoError> {
        std::str::from_utf8(&self.0).map_err(|_| CryptoError::Decrypt)
    }
}

impl Drop for Secret {
    fn drop(&mut self) {
        self.0.iter_mut().for_each(|b| *b = 0);
    }
}

fn random<const N: usize>() -> Result<[u8; N], CryptoError> {
    let mut b = [0u8; N];
    getrandom::fill(&mut b).map_err(|_| CryptoError::Random)?;
    Ok(b)
}

fn seal_with(cipher: &XChaCha20Poly1305, msg: &[u8], aad: &[u8]) -> Result<Vec<u8>, CryptoError> {
    let nonce: [u8; 24] = random()?;
    let ct = cipher
        .encrypt(&XNonce::from(nonce), Payload { msg, aad })
        .map_err(|_| CryptoError::Decrypt)?;
    Ok([nonce.as_slice(), &ct].concat())
}

fn open_with(cipher: &XChaCha20Poly1305, blob: &[u8], aad: &[u8]) -> Result<Vec<u8>, CryptoError> {
    if blob.len() < 24 + 16 {
        return Err(CryptoError::Decrypt);
    }
    let (nonce, ct) = blob.split_at(24);
    let nonce = XNonce::try_from(nonce).map_err(|_| CryptoError::Decrypt)?;
    cipher
        .decrypt(&nonce, Payload { msg: ct, aad })
        .map_err(|_| CryptoError::Decrypt)
}

/// AAD pembungkus DEK menyertakan versi KEK + konteks.
fn dek_aad(v: i16, aad: &[u8]) -> Vec<u8> {
    [b"umkm-dek-v1|".as_slice(), &v.to_be_bytes(), b"|", aad].concat()
}

pub fn seal(k: &Keyring, plaintext: &[u8], aad: &[u8]) -> Result<Sealed, CryptoError> {
    let v = k.current_version();
    let dek: [u8; 32] = random()?;
    let ciphertext = seal_with(&XChaCha20Poly1305::new((&dek).into()), plaintext, aad)?;
    let wrapped_dek = seal_with(&k.kek(v)?, &dek, &dek_aad(v, aad))?;
    Ok(Sealed {
        kek_version: v,
        wrapped_dek,
        ciphertext,
    })
}

fn unwrap_dek(k: &Keyring, s: &Sealed, aad: &[u8]) -> Result<[u8; 32], CryptoError> {
    let dek = open_with(
        &k.kek(s.kek_version)?,
        &s.wrapped_dek,
        &dek_aad(s.kek_version, aad),
    )?;
    dek.try_into().map_err(|_| CryptoError::Decrypt)
}

pub fn open(k: &Keyring, s: &Sealed, aad: &[u8]) -> Result<Secret, CryptoError> {
    let mut dek = unwrap_dek(k, s, aad)?;
    let out = open_with(&XChaCha20Poly1305::new((&dek).into()), &s.ciphertext, aad);
    dek.iter_mut().for_each(|b| *b = 0);
    out.map(Secret)
}

/// Bungkus ulang DEK dengan KEK terbaru (rotasi). Ciphertext data tidak berubah.
pub fn rewrap(k: &Keyring, s: &Sealed, aad: &[u8]) -> Result<Sealed, CryptoError> {
    let v = k.current_version();
    if s.kek_version == v {
        return Ok(s.clone());
    }
    let mut dek = unwrap_dek(k, s, aad)?;
    let wrapped_dek = seal_with(&k.kek(v)?, &dek, &dek_aad(v, aad));
    dek.iter_mut().for_each(|b| *b = 0);
    Ok(Sealed {
        kek_version: v,
        wrapped_dek: wrapped_dek?,
        ciphertext: s.ciphertext.clone(),
    })
}

/// Tampilan aman: 4 karakter terakhir saja, mis. `••••abcd`. Rahasia < 12 karakter tidak dibuka sama sekali.
pub fn mask(secret: &str) -> String {
    let n = secret.chars().count();
    if n < 12 {
        return "••••".to_owned();
    }
    let tail: String = secret.chars().skip(n - 4).collect();
    format!("••••{tail}")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn b64(n: u8) -> String {
        STANDARD.encode([n; 32])
    }

    #[test]
    fn keyring_parsing() {
        let k = Keyring::parse(&format!("v1:{},v2:{}", b64(1), b64(2))).unwrap();
        assert_eq!(k.current_version(), 2);
        assert!(!format!("{k:?}").contains(&b64(1)));
        for bad in [
            "",
            "v1:xx",
            "1:AAAA",
            &format!("v0:{}", b64(1)),
            &format!("v1:{}", b64(0)),
            &format!("v1:{},v1:{}", b64(1), b64(2)),
        ] {
            assert_eq!(
                Keyring::parse(bad).unwrap_err(),
                CryptoError::InvalidKeyring,
                "{bad}"
            );
        }
    }

    #[test]
    fn seal_open_bound_to_context() {
        let k = Keyring::parse(&format!("v1:{}", b64(7))).unwrap();
        let s = seal(&k, b"sk-rahasia-123456", b"ai|tenant-a|1").unwrap();
        assert_eq!(
            open(&k, &s, b"ai|tenant-a|1").unwrap().expose(),
            b"sk-rahasia-123456"
        );
        assert_eq!(
            open(&k, &s, b"ai|tenant-b|1").err(),
            Some(CryptoError::Decrypt),
            "AAD lain"
        );
        let s2 = seal(&k, b"sk-rahasia-123456", b"ai|tenant-a|1").unwrap();
        assert_ne!(s.ciphertext, s2.ciphertext, "nonce & DEK acak");
        let mut tampered = s.clone();
        let last = tampered.ciphertext.len() - 1;
        tampered.ciphertext[last] ^= 1;
        assert!(open(&k, &tampered, b"ai|tenant-a|1").is_err());
        assert!(!format!("{s:?}").contains("rahasia"));
    }

    #[test]
    fn rotation_rewraps_without_touching_data() {
        let old = Keyring::parse(&format!("v1:{}", b64(1))).unwrap();
        let s = seal(&old, b"rahasia", b"ctx").unwrap();
        let both = Keyring::parse(&format!("v1:{},v2:{}", b64(1), b64(2))).unwrap();
        assert_eq!(
            open(&both, &s, b"ctx").unwrap().expose(),
            b"rahasia",
            "versi lama tetap terbaca"
        );
        let r = rewrap(&both, &s, b"ctx").unwrap();
        assert_eq!((r.kek_version, &r.ciphertext), (2, &s.ciphertext));
        let only_new = Keyring::parse(&format!("v2:{}", b64(2))).unwrap();
        assert_eq!(open(&only_new, &r, b"ctx").unwrap().expose(), b"rahasia");
        assert_eq!(
            open(&only_new, &s, b"ctx").err(),
            Some(CryptoError::UnknownKek(1))
        );
    }

    #[test]
    fn masking() {
        assert_eq!(mask("sk-abcdefghijklmnop"), "••••mnop");
        assert_eq!(mask("pendek"), "••••");
    }
}
