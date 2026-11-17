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
    #[error("kuota penyimpanan toko habis")]
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

// ------------------------------------------------------------------ metadata (P9-9)

/// Validasi + buang metadata yang bisa membocorkan privasi (lokasi GPS kamera, nama perangkat, waktu, komentar)
/// sebelum gambar disimpan. Yang disimpan, di-hash, dan disajikan adalah hasil bersihnya.
pub fn sanitize_image(bytes: &[u8], max_bytes: usize) -> Result<(ImageType, Vec<u8>), UploadError> {
    let ty = inspect_image(bytes, max_bytes)?;
    let clean = strip_metadata(bytes, ty)?;
    // Hasil tetap harus lolos aturan yang sama (struktur, tanpa markup).
    inspect_image(&clean, max_bytes)?;
    Ok((ty, clean))
}

/// Buang metadata. JPEG: APP1 (Exif/XMP), APP13 (IPTC), COM, dan data sesudah EOI pertama (gambar tambahan
/// ala MPO membawa EXIF sendiri); tag Orientation dipertahankan dalam EXIF minimal agar foto potret tidak miring.
/// PNG: eXIf/tEXt/zTXt/iTXt/tIME. WebP: chunk EXIF/XMP + flag VP8X. Struktur tak terurai → ditolak (fail-closed).
pub fn strip_metadata(b: &[u8], ty: ImageType) -> Result<Vec<u8>, UploadError> {
    match ty {
        ImageType::Jpeg => strip_jpeg(b),
        ImageType::Png => Ok(strip_png(b)),
        ImageType::Webp => strip_webp(b),
    }
}

fn strip_jpeg(b: &[u8]) -> Result<Vec<u8>, UploadError> {
    let bad = || UploadError::Malformed;
    let mut out = Vec::with_capacity(b.len());
    out.extend_from_slice(&[0xFF, 0xD8]);
    let mut orientation: Option<u16> = None;
    let mut wrote_orientation = false;
    let mut i = 2;
    loop {
        // Penanda segmen: satu atau lebih 0xFF (fill) lalu kode.
        if b.get(i) != Some(&0xFF) {
            return Err(bad());
        }
        while b.get(i) == Some(&0xFF) {
            i += 1;
        }
        let m = *b.get(i).ok_or_else(bad)?;
        i += 1;
        match m {
            0xD9 => {
                out.extend_from_slice(&[0xFF, 0xD9]);
                return Ok(out); // data sesudah EOI pertama dibuang
            }
            0x01 | 0xD0..=0xD7 => {
                out.extend_from_slice(&[0xFF, m]);
                continue;
            }
            _ => {}
        }
        let len = u16::from_be_bytes([*b.get(i).ok_or_else(bad)?, *b.get(i + 1).ok_or_else(bad)?])
            as usize;
        if len < 2 {
            return Err(bad());
        }
        let seg = b.get(i..i + len).ok_or_else(bad)?;
        let payload = &seg[2..];
        i += len;
        let drop = match m {
            0xE1 => {
                if payload.starts_with(b"Exif\0\0") {
                    orientation = orientation.or_else(|| exif_orientation(&payload[6..]));
                }
                true
            }
            0xED | 0xFE => true,
            _ => false,
        };
        // EXIF minimal (hanya Orientation) ditulis tepat sebelum segmen pertama yang dipertahankan sesudah APP0.
        if !drop && !wrote_orientation && m != 0xE0 {
            if let Some(o) = orientation.filter(|o| (2..=8).contains(o)) {
                out.extend(minimal_exif(o));
            }
            wrote_orientation = true;
        }
        if !drop {
            out.extend_from_slice(&[0xFF, m]);
            out.extend_from_slice(seg);
        }
        if m == 0xDA {
            // Data entropi sesudah SOS: salin sampai penanda berikutnya (bukan stuffing 00 / RST / fill).
            loop {
                let x = *b.get(i).ok_or_else(bad)?;
                if x != 0xFF {
                    out.push(x);
                    i += 1;
                    continue;
                }
                let n = *b.get(i + 1).ok_or_else(bad)?;
                if n == 0x00 || (0xD0..=0xD7).contains(&n) {
                    out.extend_from_slice(&[0xFF, n]);
                    i += 2;
                } else if n == 0xFF {
                    i += 1;
                } else {
                    break; // penanda segmen (EOI, DHT, SOS berikutnya pada JPEG progresif, …)
                }
            }
        }
    }
}

/// Nilai tag Orientation (0x0112) dari IFD0 data TIFF di dalam APP1 Exif; None bila tidak terbaca.
fn exif_orientation(t: &[u8]) -> Option<u16> {
    let le = match t.get(0..2)? {
        b"II" => true,
        b"MM" => false,
        _ => return None,
    };
    let u16_at = |o: usize| -> Option<u16> {
        let x = [*t.get(o)?, *t.get(o + 1)?];
        Some(if le {
            u16::from_le_bytes(x)
        } else {
            u16::from_be_bytes(x)
        })
    };
    let u32_at = |o: usize| -> Option<u32> {
        let x = [*t.get(o)?, *t.get(o + 1)?, *t.get(o + 2)?, *t.get(o + 3)?];
        Some(if le {
            u32::from_le_bytes(x)
        } else {
            u32::from_be_bytes(x)
        })
    };
    let ifd = u32_at(4)? as usize;
    let n = u16_at(ifd)? as usize;
    (0..n.min(256)).find_map(|k| {
        let e = ifd + 2 + k * 12;
        (u16_at(e)? == 0x0112 && u16_at(e + 2)? == 3)
            .then(|| u16_at(e + 8))
            .flatten()
    })
}

/// APP1 Exif berisi satu entri IFD0: Orientation (SHORT). Tanpa GPS, perangkat, waktu.
fn minimal_exif(orientation: u16) -> Vec<u8> {
    let mut tiff = b"II*\0".to_vec();
    tiff.extend(8u32.to_le_bytes()); // offset IFD0
    tiff.extend(1u16.to_le_bytes()); // 1 entri
    tiff.extend(0x0112u16.to_le_bytes());
    tiff.extend(3u16.to_le_bytes()); // SHORT
    tiff.extend(1u32.to_le_bytes());
    tiff.extend(orientation.to_le_bytes());
    tiff.extend([0, 0]);
    tiff.extend(0u32.to_le_bytes()); // tidak ada IFD berikutnya
    let mut seg = vec![0xFF, 0xE1];
    seg.extend(((2 + 6 + tiff.len()) as u16).to_be_bytes());
    seg.extend(b"Exif\0\0");
    seg.extend(tiff);
    seg
}

fn strip_png(b: &[u8]) -> Vec<u8> {
    // Struktur sudah diverifikasi check_png (urutan chunk utuh sampai IEND).
    let mut out = PNG_SIG.to_vec();
    let mut i = PNG_SIG.len();
    while let Some(h) = b.get(i..i + 8) {
        let len = u32::from_be_bytes([h[0], h[1], h[2], h[3]]) as usize;
        let end = i + 12 + len;
        let kind = &h[4..8];
        if !matches!(kind, b"eXIf" | b"tEXt" | b"zTXt" | b"iTXt" | b"tIME") {
            out.extend_from_slice(&b[i..end]);
        }
        i = end;
        if kind == b"IEND" {
            break;
        }
    }
    out
}

fn strip_webp(b: &[u8]) -> Result<Vec<u8>, UploadError> {
    // body = "WEBP" + chunk-chunk yang dipertahankan; ukuran RIFF dihitung ulang.
    let mut body = b"WEBP".to_vec();
    let mut vp8x_flags: Option<usize> = None;
    let mut i = 12;
    while i < b.len() {
        let h = b.get(i..i + 8).ok_or(UploadError::Malformed)?;
        let len = u32::from_le_bytes([h[4], h[5], h[6], h[7]]) as usize;
        let padded = len.checked_add(len & 1).ok_or(UploadError::Malformed)?;
        let end = i
            .checked_add(8)
            .and_then(|x| x.checked_add(padded))
            .ok_or(UploadError::Malformed)?;
        let chunk = b.get(i..end).ok_or(UploadError::Malformed)?;
        if !matches!(&h[0..4], b"EXIF" | b"XMP ") {
            if &h[0..4] == b"VP8X" {
                vp8x_flags = Some(body.len() + 8); // byte data pertama = flags
            }
            body.extend_from_slice(chunk);
        }
        i = end;
    }
    if let Some(f) = vp8x_flags.and_then(|at| body.get_mut(at)) {
        *f &= !0x0C; // bit EXIF (0x08) & XMP (0x04)
    }
    let mut out = b"RIFF".to_vec();
    out.extend(
        u32::try_from(body.len())
            .map_err(|_| UploadError::Malformed)?
            .to_le_bytes(),
    );
    out.extend(body);
    Ok(out)
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
    /// JPEG minimal valid: SOI, APP0 JFIF (panjang 16 benar), EOI.
    pub fn jpeg() -> Vec<u8> {
        vec![
            0xFF, 0xD8, 0xFF, 0xE0, 0, 16, b'J', b'F', b'I', b'F', 0, 1, 1, 0, 0, 1, 0, 1, 0, 0,
            0xFF, 0xD9,
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

    // ---------------------------------------------------------- P9-9 metadata

    fn has(h: &[u8], n: &[u8]) -> bool {
        h.windows(n.len()).any(|w| w == n)
    }

    /// APP1 Exif big-endian: IFD0 berisi Orientation=6 + entri "GPS" palsu (penanda teks).
    fn exif_app1() -> Vec<u8> {
        let mut t = b"MM\0*".to_vec();
        t.extend(8u32.to_be_bytes());
        t.extend(2u16.to_be_bytes());
        t.extend([0x01, 0x12, 0, 3, 0, 0, 0, 1, 0, 6, 0, 0]); // Orientation = 6
        t.extend([0x88, 0x25, 0, 4, 0, 0, 0, 1, 0, 0, 0, 0]); // GPSInfo IFD pointer
        t.extend(0u32.to_be_bytes());
        t.extend(b"GPSDEMO-LAT-6.2-LON-106.8");
        let mut seg = vec![0xFF, 0xE1];
        seg.extend(((2 + 6 + t.len()) as u16).to_be_bytes());
        seg.extend(b"Exif\0\0");
        seg.extend(t);
        seg
    }

    fn seg(m: u8, payload: &[u8]) -> Vec<u8> {
        let mut v = vec![0xFF, m];
        v.extend(((payload.len() + 2) as u16).to_be_bytes());
        v.extend(payload);
        v
    }

    fn photo_jpeg() -> Vec<u8> {
        let mut v = vec![0xFF, 0xD8];
        v.extend(seg(
            0xE0,
            &[b'J', b'F', b'I', b'F', 0, 1, 1, 0, 0, 1, 0, 1, 0, 0],
        ));
        v.extend(exif_app1());
        v.extend(seg(0xE1, b"http://ns.adobe.com/xap/1.0/\0<x:GPSDEMO/>"));
        v.extend(seg(0xED, b"Photoshop 3.0\0GPSDEMO-IPTC"));
        v.extend(seg(0xFE, b"GPSDEMO-comment"));
        v.extend(seg(0xDB, &[0; 65])); // DQT
        v.extend(seg(0xDA, &[1, 1, 0, 0, 63, 0])); // SOS
        v.extend([0x12, 0xFF, 0x00, 0x34, 0xFF, 0xD0, 0x56]); // entropi: stuffing + RST0
        v.extend([0xFF, 0xD9]);
        // gambar kedua (MPO) dengan EXIF sendiri
        v.extend([0xFF, 0xD8]);
        v.extend(exif_app1());
        v.extend([0xFF, 0xD9]);
        v
    }

    #[test]
    fn jpeg_metadata_is_removed_but_orientation_kept() {
        let src = photo_jpeg();
        assert!(has(&src, b"GPSDEMO"));
        let (ty, out) = sanitize_image(&src, MAX).unwrap();
        assert_eq!(ty, ImageType::Jpeg);
        assert!(
            !has(&out, b"GPSDEMO"),
            "GPS/XMP/IPTC/komentar/gambar tambahan harus hilang"
        );
        assert!(!has(&out, b"http://ns.adobe.com"));
        assert_eq!(
            out.iter().filter(|&&x| x == 0xD8).count(),
            1,
            "hanya satu SOI"
        );
        // Orientation tetap (foto potret tidak miring), dalam EXIF minimal.
        let app1 = out.windows(6).position(|w| w == b"Exif\0\0").unwrap();
        assert_eq!(exif_orientation(&out[app1 + 6..]), Some(6));
        // Data gambar utuh, termasuk stuffing & RST.
        assert!(has(
            &out,
            &[0x12, 0xFF, 0x00, 0x34, 0xFF, 0xD0, 0x56, 0xFF, 0xD9]
        ));
        assert!(out.ends_with(&[0xFF, 0xD9]));
        assert_eq!(strip_metadata(&out, ty).unwrap(), out, "idempoten");
    }

    #[test]
    fn jpeg_without_metadata_is_unchanged_and_broken_segments_rejected() {
        assert_eq!(sanitize_image(&jpeg(), MAX).unwrap().1, jpeg());
        let mut broken = jpeg();
        broken[5] = 200; // panjang APP0 melampaui file
        assert_eq!(
            strip_metadata(&broken, ImageType::Jpeg),
            Err(UploadError::Malformed)
        );
    }

    #[test]
    fn png_text_and_exif_chunks_are_removed() {
        let base = png();
        let iend = base.len() - 12;
        let chunk = |kind: &[u8], data: &[u8]| {
            let mut v = (data.len() as u32).to_be_bytes().to_vec();
            v.extend(kind);
            v.extend(data);
            v.extend([0, 0, 0, 0]);
            v
        };
        let mut src = base[..iend].to_vec();
        src.extend(chunk(b"tEXt", b"Comment\0GPSDEMO"));
        src.extend(chunk(b"eXIf", b"MM\0*GPSDEMO"));
        src.extend(&base[iend..]);
        let (_, out) = sanitize_image(&src, MAX).unwrap();
        assert!(!has(&out, b"GPSDEMO"));
        assert_eq!(out, base, "chunk gambar tetap");
    }

    #[test]
    fn webp_exif_xmp_chunks_and_flags_are_removed() {
        let vp8 = &webp()[12..];
        let mut body = b"WEBP".to_vec();
        body.extend(b"VP8X");
        body.extend(10u32.to_le_bytes());
        body.extend([0x10 | 0x08 | 0x04, 0, 0, 0, 0, 0, 0, 0, 0, 0]); // alpha + exif + xmp
        body.extend(vp8);
        body.extend(b"EXIF");
        body.extend(7u32.to_le_bytes());
        body.extend(b"GPSDEMO\0"); // panjang ganjil + padding
        body.extend(b"XMP ");
        body.extend(4u32.to_le_bytes());
        body.extend(b"GPS!");
        let mut src = b"RIFF".to_vec();
        src.extend((body.len() as u32).to_le_bytes());
        src.extend(body);
        assert_eq!(inspect_image(&src, MAX), Ok(ImageType::Webp));
        let (_, out) = sanitize_image(&src, MAX).unwrap();
        assert!(!has(&out, b"GPSDEMO") && !has(&out, b"GPS!"));
        assert_eq!(out[20], 0x10, "flag EXIF/XMP dibersihkan, alpha tetap");
        assert_eq!(
            u32::from_le_bytes([out[4], out[5], out[6], out[7]]) as usize + 8,
            out.len()
        );
        assert!(has(&out, vp8), "data gambar tetap");
    }
}
