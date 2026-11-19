//! Website builder (`PRD.md §4.3`, ADR-0007).

pub mod schema;
pub mod schema_v2;

pub use schema::{FieldError, ValidPage, validate_page};

/// Jumlah halaman maksimum (termasuk Beranda). Starter: Beranda + 4 halaman bernama bebas.
pub const STARTER_MAX_PAGES: usize = 5;
pub const PRO_MAX_PAGES: usize = 30;

pub fn max_pages(has_pro: bool) -> usize {
    if has_pro {
        PRO_MAX_PAGES
    } else {
        STARTER_MAX_PAGES
    }
}

/// Halaman sistem toko (slug = rute toko) dan blok intinya. Isinya dokumen Studio biasa — pemilik bebas
/// menambah & memindah blok — tetapi blok inti (keranjang+checkout / status pesanan / akun) wajib ada
/// TEPAT satu dan hanya boleh ada di halamannya sendiri. Halaman sistem tidak dihitung dalam batas paket
/// dan tidak tampil di menu navigasi / sitemap.
pub const SYSTEM_PAGES: [(&str, &str); 3] = [
    ("keranjang", "shop_cart"),
    ("pesanan", "shop_orders"),
    ("akun", "shop_account"),
];

/// Jenis blok inti untuk halaman sistem `slug`.
pub fn system_core(slug: &str) -> Option<&'static str> {
    SYSTEM_PAGES
        .iter()
        .find(|(s, _)| *s == slug)
        .map(|(_, k)| *k)
}

pub fn is_system_slug(slug: &str) -> bool {
    system_core(slug).is_some()
}

pub fn is_core_kind(kind: &str) -> bool {
    SYSTEM_PAGES.iter().any(|(_, k)| *k == kind)
}

/// Slug yang dipakai rute toko/situs; tidak boleh dipakai halaman biasa (akan tertutup rutenya).
pub fn is_reserved_slug(slug: &str) -> bool {
    is_system_slug(slug)
        || matches!(
            slug,
            "produk" | "kebijakan-privasi" | "syarat-transaksi" | "site"
        )
}

/// Aturan blok inti per halaman (di luar skema dokumen karena butuh slug).
pub fn check_core_blocks(slug: &str, doc: &serde_json::Value) -> Result<(), Vec<FieldError>> {
    let want = system_core(slug);
    let mut errs = Vec::new();
    let mut found = 0;
    let blocks = doc.get("blocks").and_then(serde_json::Value::as_array);
    for (i, b) in blocks.into_iter().flatten().enumerate() {
        let kind = b
            .get("kind")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("");
        if !is_core_kind(kind) {
            continue;
        }
        if Some(kind) == want {
            found += 1;
        } else {
            errs.push(FieldError {
                path: format!("blocks[{i}].kind"),
                code: "core_block_not_allowed",
            });
        }
    }
    if want.is_some() && found != 1 {
        errs.push(FieldError {
            path: "blocks".into(),
            code: "core_block_required",
        });
    }
    if errs.is_empty() { Ok(()) } else { Err(errs) }
}

/// Slug halaman: `[a-z0-9-]`, 1–40, tidak diawali/diakhiri `-`.
pub fn is_valid_page_slug(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 40
        && !s.starts_with('-')
        && !s.ends_with('-')
        && s.bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
}
