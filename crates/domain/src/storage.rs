//! Aturan upload gambar (P2-6, `PRD.md §6.8`, ADR-0009). Murni: tidak menyentuh I/O.
//!
//! Tipe ditentukan dari isi file (magic bytes + struktur), BUKAN dari header atau nama
//! file klien. SVG/HTML selalu ditolak. Data setelah penanda akhir format (cara umum
//! membuat polyglot "gambar + skrip/arsip") ditolak.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImageType {
    Png,
    Jpeg,
    Webp,
}

impl ImageType {
    pub fn content_type(self) -> &'static str {
        match self {
            Self::Png => "image/png",
            Self::Jpeg => "image/jpeg",
            Self::Webp => "image/webp",
        }
    }

    pub fn from_content_type(s: &str) -> Option<Self> {
        match s {
            "image/png" => Some(Self::Png),
            "image/jpeg" => Some(Self::Jpeg),
            "image/webp" => Some(Self::Webp),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum UploadError {
    #[error("file kosong")]
    Empty,
    #[error("file melebihi {0} byte")]
    TooLarge(usize),
    #[error("tipe file tidak didukung (hanya PNG, JPEG, WebP)")]
    UnsupportedType,
    #[error("struktur file tidak valid atau memuat data tambahan")]
    Malformed,
    #[error("kuota penyimpanan tenant habis")]
    QuotaExceeded,
}

impl UploadError {
    pub fn code(self) -> &'static str {
        match self {
            Self::Empty => "empty_file",
            Self::TooLarge(_) => "file_too_large",
            Self::UnsupportedType => "unsupported_type",
            Self::Malformed => "malformed_file",
            Self::QuotaExceeded => "quota_exceeded",
        }
    }
}

const PNG_SIG: &[u8] = b"\x89PNG\r\n\x1a\n";

/// Validasi lengkap upload: ukuran, tipe dari isi, struktur, dan penanda markup.
pub fn inspect_image(bytes: &[u8], max_bytes: usize) -> Result<ImageType, UploadError> {
    if bytes.is_empty() {
        return Err(UploadError::Empty);
    }
    if bytes.len() > max_bytes {
        return Err(UploadError::TooLarge(max_bytes));
    }
    let ty = if bytes.starts_with(PNG_SIG) {
        check_png(bytes)?;
        ImageType::Png
    } else if bytes.starts_with(&[0xFF, 0xD8, 0xFF]) {
        check_jpeg(bytes)?;
        ImageType::Jpeg
    } else if bytes.len() >= 12 && &bytes[..4] == b"RIFF" && &bytes[8..12] == b"WEBP" {
        check_webp(bytes)?;
        ImageType::Webp
    } else {
        return Err(UploadError::UnsupportedType);
    };
    if contains_markup(bytes) {
        return Err(UploadError::Malformed);
    }
    Ok(ty)
}

/// PNG: rangkaian chunk `len|type|data|crc` sampai `IEND`, tanpa byte sesudahnya.
fn check_png(b: &[u8]) -> Result<(), UploadError> {
    let mut i = PNG_SIG.len();
    let mut first = true;
    loop {
        let header = b.get(i..i + 8).ok_or(UploadError::Malformed)?;
        let len = u32::from_be_bytes([header[0], header[1], header[2], header[3]]) as usize;
        let kind = &header[4..8];
        if first && kind != b"IHDR" {
            return Err(UploadError::Malformed);
        }
        first = false;
        let end = i
            .checked_add(12)
            .and_then(|x| x.checked_add(len))
            .ok_or(UploadError::Malformed)?;
        if end > b.len() || !kind.iter().all(u8::is_ascii_alphabetic) {
            return Err(UploadError::Malformed);
        }
        i = end;
        if kind == b"IEND" {
            return if i == b.len() {
                Ok(())
            } else {
                Err(UploadError::Malformed)
            };
        }
    }
}

/// JPEG: diawali SOI dan diakhiri EOI (`FF D9`) tepat di akhir file.
fn check_jpeg(b: &[u8]) -> Result<(), UploadError> {
    if b.len() < 4 || !b.ends_with(&[0xFF, 0xD9]) {
        return Err(UploadError::Malformed);
    }
    Ok(())
}

/// WebP: ukuran RIFF di header harus sama persis dengan ukuran file.
fn check_webp(b: &[u8]) -> Result<(), UploadError> {
    let declared = u32::from_le_bytes([b[4], b[5], b[6], b[7]]) as usize;
    if declared.checked_add(8) != Some(b.len()) || b.len() < 20 {
        return Err(UploadError::Malformed);
    }
    let chunk = &b[12..16];
    if !matches!(chunk, b"VP8 " | b"VP8L" | b"VP8X") {
        return Err(UploadError::Malformed);
    }
    Ok(())
}

/// Penanda markup/skrip yang tidak pernah muncul sah pada gambar raster dan menjadi
/// ciri polyglot yang ditujukan untuk browser yang salah menebak tipe.
fn contains_markup(b: &[u8]) -> bool {
    const NEEDLES: [&[u8]; 6] = [
        b"<script", b"<html", b"<svg", b"<body", b"<iframe", b"<?php",
    ];
    b.windows(5).enumerate().any(|(i, w)| {
        w[0] == b'<'
            && NEEDLES.iter().any(|n| {
                b.get(i..i + n.len())
                    .is_some_and(|s| s.eq_ignore_ascii_case(n))
            })
    })
}

#[cfg(test)]
pub mod fixtures {
    /// PNG 1x1 valid minimal (IHDR, IDAT, IEND).
    pub fn png() -> Vec<u8> {
        let mut v = super::PNG_SIG.to_vec();
        let chunk = |v: &mut Vec<u8>, kind: &[u8], data: &[u8]| {
            v.extend((data.len() as u32).to_be_bytes());
            v.extend(kind);
            v.extend(data);
            v.extend([0, 0, 0, 0]); // CRC tidak diverifikasi (tidak didekode)
        };
        chunk(&mut v, b"IHDR", &[0, 0, 0, 1, 0, 0, 0, 1, 8, 6, 0, 0, 0]);
        chunk(&mut v, b"IDAT", &[0x78, 0x9c, 0x63, 0, 1, 0, 0, 5, 0, 1]);
        chunk(&mut v, b"IEND", &[]);
        v
    }
    pub fn jpeg() -> Vec<u8> {
        vec![
            0xFF, 0xD8, 0xFF, 0xE0, 0, 16, b'J', b'F', b'I', b'F', 0, 1, 0xFF, 0xD9,
        ]
    }
    pub fn webp() -> Vec<u8> {
        let mut body = b"WEBPVP8 ".to_vec();
        body.extend([10, 0, 0, 0, 0x30, 1, 0, 0x9d, 1, 0x2a, 1, 0, 1, 0]);
        let mut v = b"RIFF".to_vec();
        v.extend((body.len() as u32).to_le_bytes());
        v.extend(body);
        v
    }
}

#[cfg(test)]
mod tests {
    use super::{fixtures::*, *};

    const MAX: usize = 2 * 1024 * 1024;

    #[test]
    fn accepts_valid_png_jpeg_webp() {
        assert_eq!(inspect_image(&png(), MAX), Ok(ImageType::Png));
        assert_eq!(inspect_image(&jpeg(), MAX), Ok(ImageType::Jpeg));
        assert_eq!(inspect_image(&webp(), MAX), Ok(ImageType::Webp));
    }

    #[test]
    fn rejects_svg_html_and_unknown_types() {
        for b in [
            &b"<svg xmlns='http://www.w3.org/2000/svg'><script>alert(1)</script></svg>"[..],
            b"<!doctype html><script>alert(1)</script>",
            b"GIF89a....",
            b"%PDF-1.7",
            b"PK\x03\x04",
        ] {
            assert_eq!(inspect_image(b, MAX), Err(UploadError::UnsupportedType));
        }
    }

    #[test]
    fn rejects_html_renamed_as_png_by_content_not_name() {
        // Nama/Content-Type klien tidak pernah dipakai; isi HTML tetap HTML.
        assert_eq!(
            inspect_image(b"<html><body>hi</body></html>", MAX),
            Err(UploadError::UnsupportedType)
        );
    }

    #[test]
    fn rejects_trailing_payload_polyglots() {
        for mut img in [png(), jpeg(), webp()] {
            img.extend(b"PK\x03\x04zip-payload");
            assert_eq!(inspect_image(&img, MAX), Err(UploadError::Malformed));
        }
    }

    #[test]
    fn rejects_markup_embedded_inside_a_structurally_valid_image() {
        let mut v = PNG_SIG.to_vec();
        let mut chunk = |kind: &[u8], data: &[u8]| {
            v.extend((data.len() as u32).to_be_bytes());
            v.extend(kind);
            v.extend(data);
            v.extend([0, 0, 0, 0]);
        };
        chunk(b"IHDR", &[0, 0, 0, 1, 0, 0, 0, 1, 8, 6, 0, 0, 0]);
        chunk(b"tEXt", b"c\0<SCRIPT>alert(1)</script>");
        chunk(b"IEND", &[]);
        assert_eq!(inspect_image(&v, MAX), Err(UploadError::Malformed));
    }

    #[test]
    fn rejects_truncated_and_lying_headers() {
        let p = png();
        assert_eq!(
            inspect_image(&p[..p.len() - 3], MAX),
            Err(UploadError::Malformed)
        );
        let mut w = webp();
        w[4] = w[4].wrapping_add(1);
        assert_eq!(inspect_image(&w, MAX), Err(UploadError::Malformed));
        let mut huge = PNG_SIG.to_vec();
        huge.extend(u32::MAX.to_be_bytes());
        huge.extend(b"IHDR");
        assert_eq!(inspect_image(&huge, MAX), Err(UploadError::Malformed));
    }

    #[test]
    fn rejects_empty_and_too_large() {
        assert_eq!(inspect_image(&[], MAX), Err(UploadError::Empty));
        let mut big = jpeg();
        big.splice(2..2, std::iter::repeat_n(0u8, MAX));
        assert_eq!(inspect_image(&big, MAX), Err(UploadError::TooLarge(MAX)));
    }
}
