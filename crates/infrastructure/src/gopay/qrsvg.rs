//! Render payload QRIS (atau teks apa pun) jadi SVG QR code (P10). Tanpa raster/font: hanya path
//! SVG hitam-putih, aman di-inline di halaman bayar. Gagal (mis. data terlalu panjang) → None.

use qrcode::{EcLevel, QrCode, render::svg};

/// SVG QR dari `data`. Ukuran modul & warna tetap; viewBox auto dari qrcode.
pub fn to_svg(data: &str) -> Option<String> {
    // Error correction M: seimbang untuk pembayaran (toleran kotor, ukuran wajar).
    let code = QrCode::with_error_correction_level(data.as_bytes(), EcLevel::M).ok()?;
    Some(
        code.render::<svg::Color>()
            .min_dimensions(240, 240)
            .quiet_zone(true)
            .dark_color(svg::Color("#000000"))
            .light_color(svg::Color("#ffffff"))
            .build(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renders_svg() {
        let svg = to_svg("00020101021126").unwrap();
        assert!(svg.contains("<svg"));
        assert!(svg.contains("</svg>"));
    }
}
