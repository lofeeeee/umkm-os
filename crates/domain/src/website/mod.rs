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

/// Slug halaman: `[a-z0-9-]`, 1–40, tidak diawali/diakhiri `-`.
pub fn is_valid_page_slug(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 40
        && !s.starts_with('-')
        && !s.ends_with('-')
        && s.bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
}
