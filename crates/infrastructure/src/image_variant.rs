//! Varian lebar gambar untuk `srcset` (P9-7). Decode dengan batas dimensi & alokasi (gambar unggahan tidak
//! dipercaya), terapkan orientasi EXIF, perkecil, encode ulang: JPEG (foto) atau PNG (bila ada transparansi).
//! Hasil encode ulang tidak membawa metadata apa pun.

use std::io::Cursor;

use image::{
    DynamicImage, ImageDecoder, ImageFormat, ImageReader, Limits, codecs::jpeg::JpegEncoder,
    imageops::FilterType,
};
use umkm_domain::storage::ImageType;

/// Lebar yang boleh diminta (sama dengan CHECK di migrasi 0035 dan srcset frontend).
pub const WIDTHS: [u32; 4] = [320, 640, 960, 1280];
/// Batas aman decode: unggahan maks ±2 MB tetapi dimensi bisa diklaim besar (bom dekompresi).
const MAX_SIDE: u32 = 8000;
const MAX_ALLOC: u64 = 192 * 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Variant {
    pub content_type: &'static str,
    pub bytes: Vec<u8>,
}

#[derive(Debug, thiserror::Error)]
pub enum VariantError {
    #[error("lebar tidak didukung")]
    Width,
    #[error("gambar tidak dapat diproses: {0}")]
    Image(#[from] image::ImageError),
}

fn reader(bytes: &[u8], ty: ImageType) -> ImageReader<Cursor<&[u8]>> {
    let format = match ty {
        ImageType::Jpeg => ImageFormat::Jpeg,
        ImageType::Png => ImageFormat::Png,
        ImageType::Webp => ImageFormat::WebP,
    };
    let mut r = ImageReader::with_format(Cursor::new(bytes), format);
    let mut limits = Limits::default();
    limits.max_image_width = Some(MAX_SIDE);
    limits.max_image_height = Some(MAX_SIDE);
    limits.max_alloc = Some(MAX_ALLOC);
    r.limits(limits);
    r
}

/// Lebar tampil (setelah orientasi EXIF) dari header saja — tanpa decode piksel.
pub fn display_width(bytes: &[u8], ty: ImageType) -> Result<u32, VariantError> {
    let mut d = reader(bytes, ty).into_decoder()?;
    let (w, h) = d.dimensions();
    let rotated = matches!(
        d.orientation()?,
        image::metadata::Orientation::Rotate90
            | image::metadata::Orientation::Rotate270
            | image::metadata::Orientation::Rotate90FlipH
            | image::metadata::Orientation::Rotate270FlipH
    );
    Ok(if rotated { h } else { w })
}

/// `Ok(None)` bila gambar asli sudah tidak lebih lebar dari `width` (sajikan asli).
pub fn make_variant(
    bytes: &[u8],
    ty: ImageType,
    width: u32,
) -> Result<Option<Variant>, VariantError> {
    if !WIDTHS.contains(&width) {
        return Err(VariantError::Width);
    }
    let mut decoder = reader(bytes, ty).into_decoder()?;
    let orientation = decoder.orientation()?;
    let mut img = DynamicImage::from_decoder(decoder)?;
    img.apply_orientation(orientation);
    if img.width() <= width {
        return Ok(None);
    }
    let height =
        ((u64::from(img.height()) * u64::from(width)) / u64::from(img.width())).max(1) as u32;
    let small = img.resize_exact(width, height, FilterType::CatmullRom);
    let mut out = Vec::new();
    if small.color().has_alpha() {
        small.write_to(&mut Cursor::new(&mut out), ImageFormat::Png)?;
        Ok(Some(Variant {
            content_type: "image/png",
            bytes: out,
        }))
    } else {
        JpegEncoder::new_with_quality(&mut out, 82).encode_image(&small.to_rgb8())?;
        Ok(Some(Variant {
            content_type: "image/jpeg",
            bytes: out,
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{ImageBuffer, Rgb, Rgba};

    fn jpeg(w: u32, h: u32) -> Vec<u8> {
        let img = ImageBuffer::from_fn(w, h, |x, y| Rgb([(x % 255) as u8, (y % 255) as u8, 128]));
        let mut out = Vec::new();
        JpegEncoder::new_with_quality(&mut out, 90)
            .encode_image(&img)
            .unwrap();
        out
    }

    fn dims(v: &Variant) -> (u32, u32) {
        let i = image::load_from_memory(&v.bytes).unwrap();
        (i.width(), i.height())
    }

    #[test]
    fn photo_is_scaled_keeping_aspect_ratio() {
        let v = make_variant(&jpeg(2000, 1000), ImageType::Jpeg, 640)
            .unwrap()
            .unwrap();
        assert_eq!(v.content_type, "image/jpeg");
        assert_eq!(dims(&v), (640, 320));
        assert!(v.bytes.len() < jpeg(2000, 1000).len());
    }

    #[test]
    fn small_image_is_served_as_is_and_odd_widths_are_rejected() {
        assert_eq!(
            make_variant(&jpeg(500, 300), ImageType::Jpeg, 640).unwrap(),
            None
        );
        assert!(matches!(
            make_variant(&jpeg(2000, 1000), ImageType::Jpeg, 641),
            Err(VariantError::Width)
        ));
    }

    #[test]
    fn transparency_is_kept_as_png() {
        let img = ImageBuffer::from_fn(1000, 1000, |x, _| Rgba([0, 0, 0, (x % 255) as u8]));
        let mut png = Vec::new();
        DynamicImage::ImageRgba8(img)
            .write_to(&mut Cursor::new(&mut png), ImageFormat::Png)
            .unwrap();
        let v = make_variant(&png, ImageType::Png, 320).unwrap().unwrap();
        assert_eq!((v.content_type, dims(&v)), ("image/png", (320, 320)));
    }

    #[test]
    fn exif_orientation_is_applied() {
        // JPEG 2000×1000 + APP1 Exif Orientation=6 (putar 90° searah jarum jam) → tampil potret 1000×2000.
        let src = jpeg(2000, 1000);
        let mut tiff = b"MM\0*".to_vec();
        tiff.extend(8u32.to_be_bytes());
        tiff.extend(1u16.to_be_bytes());
        tiff.extend([0x01, 0x12, 0, 3, 0, 0, 0, 1, 0, 6, 0, 0]);
        tiff.extend(0u32.to_be_bytes());
        let mut app1 = vec![0xFF, 0xE1];
        app1.extend(((2 + 6 + tiff.len()) as u16).to_be_bytes());
        app1.extend(b"Exif\0\0");
        app1.extend(tiff);
        let mut rotated = src[..2].to_vec();
        rotated.extend(app1);
        rotated.extend(&src[2..]);
        let v = make_variant(&rotated, ImageType::Jpeg, 640)
            .unwrap()
            .unwrap();
        assert_eq!(dims(&v), (640, 1280));
    }

    #[test]
    fn decompression_bomb_header_is_rejected() {
        // PNG mengaku 20000×20000 (melebihi batas) → error, bukan alokasi raksasa.
        let mut png = b"\x89PNG\r\n\x1a\n".to_vec();
        png.extend(13u32.to_be_bytes());
        png.extend(b"IHDR");
        png.extend(20000u32.to_be_bytes());
        png.extend(20000u32.to_be_bytes());
        png.extend([8, 6, 0, 0, 0]);
        png.extend([0, 0, 0, 0]);
        assert!(make_variant(&png, ImageType::Png, 320).is_err());
    }
}
