//! Decode payload QRIS statis dari gambar yang sudah diunggah (P10 lanjutan).
//! Murni sinkron/CPU-bound; pemanggil API menjalankannya lewat `spawn_blocking`.

use rqrr::PreparedImage;
use umkm_domain::commerce::qris;

/// Batas sisi terpanjang sebelum gambar diperkecil (jaga CPU & memori).
const MAX_SIDE: u32 = 1600;
const MIN_PAYLOAD: usize = 20;
const MAX_PAYLOAD: usize = 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum QrisDecodeError {
    #[error("format gambar tidak dikenal atau rusak")]
    Image,
    #[error("QR tidak ditemukan / tidak dapat dibaca")]
    NoQr,
    #[error("isi QR bukan payload QRIS statis yang valid")]
    InvalidPayload,
}

/// Baca QR pertama yang berisi payload QRIS statis valid.
/// Beberapa QR dalam satu gambar diizinkan: QR non-QRIS dilewati.
pub fn decode_qris_image(bytes: &[u8]) -> Result<String, QrisDecodeError> {
    let mut img = image::load_from_memory(bytes).map_err(|_| QrisDecodeError::Image)?;
    if img.width() > MAX_SIDE || img.height() > MAX_SIDE {
        img = img.resize(MAX_SIDE, MAX_SIDE, image::imageops::FilterType::Lanczos3);
    }
    let gray = img.to_luma8();
    let (w, h) = (gray.width() as usize, gray.height() as usize);
    let pixels = gray.into_raw();
    let mut prepared = PreparedImage::prepare_from_greyscale(w, h, |x, y| pixels[y * w + x]);

    let mut saw_qr = false;
    for grid in prepared.detect_grids() {
        let Ok((_, content)) = grid.decode() else {
            continue;
        };
        saw_qr = true;
        let payload = content.trim().to_owned();
        if (MIN_PAYLOAD..=MAX_PAYLOAD).contains(&payload.len()) && qris::verify(&payload).is_ok() {
            return Ok(payload);
        }
    }
    Err(if saw_qr {
        QrisDecodeError::InvalidPayload
    } else {
        QrisDecodeError::NoQr
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{DynamicImage, ImageFormat, Luma};
    use qrcode::QrCode;
    use std::io::Cursor;
    use umkm_domain::commerce::qris::crc16_ccitt;

    fn payload() -> String {
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

    fn qr_png(content: &str) -> Vec<u8> {
        let img = QrCode::new(content.as_bytes())
            .unwrap()
            .render::<Luma<u8>>()
            .min_dimensions(400, 400)
            .build();
        let mut bytes = Vec::new();
        DynamicImage::ImageLuma8(img)
            .write_to(&mut Cursor::new(&mut bytes), ImageFormat::Png)
            .unwrap();
        bytes
    }

    #[test]
    fn decodes_valid_qris_png() {
        let expected = payload();
        assert_eq!(decode_qris_image(&qr_png(&expected)).unwrap(), expected);
    }

    #[test]
    fn rejects_non_qris_qr() {
        assert_eq!(
            decode_qris_image(&qr_png("https://example.com")).unwrap_err(),
            QrisDecodeError::InvalidPayload
        );
    }

    #[test]
    fn rejects_corrupt_image() {
        assert_eq!(
            decode_qris_image(&[1, 2, 3]).unwrap_err(),
            QrisDecodeError::Image
        );
    }

    #[test]
    fn rejects_bad_crc() {
        let mut b = payload().into_bytes();
        let i = b.len() - 1;
        b[i] = if b[i] == b'0' { b'1' } else { b'0' };
        let bad = String::from_utf8(b).unwrap();
        assert_eq!(
            decode_qris_image(&qr_png(&bad)).unwrap_err(),
            QrisDecodeError::InvalidPayload
        );
    }

    fn blank_png() -> Vec<u8> {
        let img = image::ImageBuffer::<Luma<u8>, Vec<u8>>::from_pixel(64, 64, Luma([255]));
        let mut bytes = Vec::new();
        DynamicImage::ImageLuma8(img)
            .write_to(&mut Cursor::new(&mut bytes), ImageFormat::Png)
            .unwrap();
        bytes
    }

    #[test]
    fn rejects_blank_image() {
        assert_eq!(
            decode_qris_image(&blank_png()).unwrap_err(),
            QrisDecodeError::NoQr
        );
    }
}
