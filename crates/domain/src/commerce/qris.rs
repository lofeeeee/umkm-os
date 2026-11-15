//! QRIS EMVCo (SNI/ASPI): ubah QRIS **statis** toko menjadi **dinamis** dengan nominal terkunci,
//! plus kode unik untuk mencocokkan pembayaran masuk (P10, integrasi GoPay gateway). Murni: tanpa IO.
//!
//! Berbeda dari implementasi JS asal (`lib/qris.js`) yang memakai cari-ganti string (`010211`,
//! `5802ID`) sehingga bisa salah bila pola itu muncul di dalam data lain: di sini payload diurai
//! per objek TLV (`id(2) len(2) value`), divalidasi (termasuk CRC), baru diubah dan dirangkai ulang.
//!
//! Satuan: QRIS/e-wallet Indonesia memakai **rupiah bulat** (bukan sen), jadi fungsi di sini
//! bekerja dalam rupiah. Konversi dari [`crate::catalog::Money`] dilakukan di batas (lihat
//! [`charge_rupiah`]).

use crate::catalog::Money;

/// Kode unik ditambahkan ke tagihan: 1..=[`UNIQUE_MAX`] rupiah (3 digit) agar nominal tiap pesanan
/// aktif berbeda, sehingga pembayaran masuk bisa dicocokkan dari nominalnya.
pub const UNIQUE_MAX: u16 = 999;

/// Batas panjang nilai tag 54 (Transaction Amount) per spec EMVCo: 13 digit.
const AMOUNT_MAX_RUPIAH: u64 = 9_999_999_999_999;

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum QrisError {
    #[error("format QRIS tidak valid")]
    Malformed,
    #[error("checksum QRIS tidak cocok")]
    BadCrc,
    #[error("nominal di luar rentang yang diizinkan")]
    AmountRange,
}

/// CRC-16/CCITT-FALSE (poly 0x1021, init 0xFFFF, tanpa refleksi/xor-out). Standar tag 63 QRIS.
pub fn crc16_ccitt(data: &[u8]) -> u16 {
    let mut crc: u16 = 0xFFFF;
    for &b in data {
        crc ^= (b as u16) << 8;
        for _ in 0..8 {
            crc = if crc & 0x8000 != 0 {
                (crc << 1) ^ 0x1021
            } else {
                crc << 1
            };
        }
    }
    crc
}

/// Objek data TLV tingkat atas, mempertahankan urutan asal.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Field {
    id: String,
    value: String,
}

/// Urai payload jadi daftar TLV tingkat atas. ASCII saja; panjang harus pas.
fn parse_top(s: &str) -> Result<Vec<Field>, QrisError> {
    if !s.is_ascii() {
        return Err(QrisError::Malformed);
    }
    let b = s.as_bytes();
    let mut i = 0;
    let mut out = Vec::new();
    while i < b.len() {
        if i + 4 > b.len() {
            return Err(QrisError::Malformed);
        }
        let id = &s[i..i + 2];
        let len_str = &s[i + 2..i + 4];
        if !id.bytes().all(|c| c.is_ascii_digit()) || !len_str.bytes().all(|c| c.is_ascii_digit()) {
            return Err(QrisError::Malformed);
        }
        let len: usize = len_str.parse().map_err(|_| QrisError::Malformed)?;
        let start = i + 4;
        let end = start + len;
        if end > b.len() {
            return Err(QrisError::Malformed);
        }
        out.push(Field {
            id: id.to_string(),
            value: s[start..end].to_string(),
        });
        i = end;
    }
    if out.is_empty() {
        return Err(QrisError::Malformed);
    }
    Ok(out)
}

fn serialize(fields: &[Field]) -> String {
    let mut s = String::new();
    for f in fields {
        s.push_str(&f.id);
        s.push_str(&format!("{:02}", f.value.len()));
        s.push_str(&f.value);
    }
    s
}

/// Validasi payload QRIS: format TLV benar dan CRC (tag 63) cocok.
pub fn verify(static_qris: &str) -> Result<(), QrisError> {
    let fields = parse_top(static_qris)?;
    let last = fields.last().ok_or(QrisError::Malformed)?;
    if last.id != "63" || last.value.len() != 4 {
        return Err(QrisError::Malformed);
    }
    // CRC dihitung atas seluruh payload hingga dan termasuk "6304".
    let upto = static_qris.len() - 4;
    let expected = crc16_ccitt(&static_qris.as_bytes()[..upto]);
    let got = u16::from_str_radix(&last.value, 16).map_err(|_| QrisError::Malformed)?;
    if got != expected {
        return Err(QrisError::BadCrc);
    }
    Ok(())
}

/// Format nominal rupiah bulat untuk tag 54.
fn amount_tag(amount_rupiah: u64) -> Result<String, QrisError> {
    if amount_rupiah == 0 || amount_rupiah > AMOUNT_MAX_RUPIAH {
        return Err(QrisError::AmountRange);
    }
    Ok(amount_rupiah.to_string())
}

/// Ubah QRIS statis jadi dinamis dengan nominal terkunci (rupiah bulat).
///
/// Langkah: validasi payload+CRC → set tag 01 (Point of Initiation) ke `12` (dinamis) →
/// sisipkan/ganti tag 54 (Transaction Amount) → buang tag 63 lama → rangkai ulang → hitung CRC baru.
pub fn to_dynamic(static_qris: &str, amount_rupiah: u64) -> Result<String, QrisError> {
    verify(static_qris)?;
    let amount = amount_tag(amount_rupiah)?;
    let parsed = parse_top(static_qris)?;

    let mut fields: Vec<Field> = Vec::with_capacity(parsed.len() + 1);
    let mut inserted_54 = false;
    // Sisipkan tag 54 tepat sebelum objek pertama dari {58 country, 59 name, 60 city, 63 crc}.
    for f in parsed.into_iter().filter(|f| f.id != "63" && f.id != "54") {
        if !inserted_54 && matches!(f.id.as_str(), "58" | "59" | "60") {
            fields.push(Field {
                id: "54".into(),
                value: amount.clone(),
            });
            inserted_54 = true;
        }
        if f.id == "01" {
            fields.push(Field {
                id: "01".into(),
                value: "12".into(), // 11 = statis, 12 = dinamis
            });
        } else {
            fields.push(f);
        }
    }
    if !inserted_54 {
        fields.push(Field {
            id: "54".into(),
            value: amount,
        });
    }

    let mut body = serialize(&fields);
    body.push_str("6304");
    let crc = crc16_ccitt(body.as_bytes());
    body.push_str(&format!("{crc:04X}"));
    Ok(body)
}

/// Tambahkan kode unik (rupiah) ke nominal dasar. `code` harus 1..=[`UNIQUE_MAX`].
pub fn with_unique(base_rupiah: u64, code: u16) -> Option<u64> {
    if code == 0 || code > UNIQUE_MAX {
        return None;
    }
    base_rupiah.checked_add(code as u64)
}

/// Nominal tagihan (rupiah bulat) dari [`Money`]. QRIS tidak mengenal sen; menolak jika ada pecahan.
pub fn charge_rupiah(amount: Money) -> Option<u64> {
    let sen = amount.sen();
    if sen <= 0 || sen % 100 != 0 {
        return None;
    }
    u64::try_from(sen / 100).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Rangkai QRIS statis valid (CRC dihitung oleh implementasi ini) untuk dipakai sebagai fixture.
    fn sample_static() -> String {
        let fields = [
            ("00", "01"),
            ("01", "11"),
            ("26", "0014COM.EXAMPLE0208ABCDEF12"),
            ("52", "5812"),
            ("53", "360"),
            ("58", "ID"),
            ("59", "WARUNG KOPI"),
            ("60", "JAKARTA"),
            ("62", "0703A01"),
        ];
        let mut body = String::new();
        for (id, v) in fields {
            body.push_str(id);
            body.push_str(&format!("{:02}", v.len()));
            body.push_str(v);
        }
        body.push_str("6304");
        let crc = crc16_ccitt(body.as_bytes());
        body.push_str(&format!("{crc:04X}"));
        body
    }

    #[test]
    fn crc_standard_check_value() {
        // Nilai cek baku CRC-16/CCITT-FALSE untuk "123456789".
        assert_eq!(crc16_ccitt(b"123456789"), 0x29B1);
    }

    #[test]
    fn verify_accepts_valid_and_rejects_tampered() {
        let q = sample_static();
        assert_eq!(verify(&q), Ok(()));

        // Ubah satu karakter nilai → CRC tak cocok.
        let mut bad = q.clone();
        let pos = bad.find("WARUNG").unwrap();
        bad.replace_range(pos..pos + 1, "X");
        assert_eq!(verify(&bad), Err(QrisError::BadCrc));

        assert_eq!(verify("garbage"), Err(QrisError::Malformed));
        // len melebihi sisa payload.
        assert_eq!(verify("0099XX"), Err(QrisError::Malformed));
    }

    #[test]
    fn to_dynamic_locks_amount_and_marks_dynamic() {
        let q = sample_static();
        let dyn_q = to_dynamic(&q, 30_217).unwrap();

        // Hasil tetap payload QRIS valid.
        assert_eq!(verify(&dyn_q), Ok(()));
        let fields = parse_top(&dyn_q).unwrap();

        // Tag 01 jadi dinamis.
        assert_eq!(fields.iter().find(|f| f.id == "01").unwrap().value, "12");
        // Tag 54 berisi nominal, muncul tepat satu kali, sebelum tag 58.
        let fit: Vec<&str> = fields.iter().map(|f| f.id.as_str()).collect();
        assert_eq!(fields.iter().filter(|f| f.id == "54").count(), 1);
        assert_eq!(fields.iter().find(|f| f.id == "54").unwrap().value, "30217");
        let i54 = fit.iter().position(|x| *x == "54").unwrap();
        let i58 = fit.iter().position(|x| *x == "58").unwrap();
        assert!(i54 < i58);
    }

    #[test]
    fn to_dynamic_replaces_existing_amount() {
        // QRIS yang sudah punya tag 54 lama: harus diganti, bukan digandakan.
        let q = sample_static();
        let once = to_dynamic(&q, 1_000).unwrap();
        let twice = to_dynamic(&once, 2_000).unwrap();
        let fields = parse_top(&twice).unwrap();
        assert_eq!(fields.iter().filter(|f| f.id == "54").count(), 1);
        assert_eq!(fields.iter().find(|f| f.id == "54").unwrap().value, "2000");
        assert_eq!(verify(&twice), Ok(()));
    }

    #[test]
    fn amount_bounds() {
        let q = sample_static();
        assert_eq!(to_dynamic(&q, 0), Err(QrisError::AmountRange));
        assert_eq!(
            to_dynamic(&q, AMOUNT_MAX_RUPIAH + 1),
            Err(QrisError::AmountRange)
        );
    }

    #[test]
    fn unique_code_rules() {
        assert_eq!(with_unique(30_000, 217), Some(30_217));
        assert_eq!(with_unique(30_000, 0), None);
        assert_eq!(with_unique(30_000, UNIQUE_MAX + 1), None);
    }

    #[test]
    fn charge_rupiah_rejects_fractions() {
        assert_eq!(charge_rupiah(Money::parse("15000").unwrap()), Some(15_000));
        assert_eq!(charge_rupiah(Money::parse("15000.50").unwrap()), None);
        assert_eq!(charge_rupiah(Money::ZERO), None);
    }
}
