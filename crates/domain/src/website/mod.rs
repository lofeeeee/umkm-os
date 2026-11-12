//! Website builder (`PRD.md §4.3`, ADR-0007).

pub mod schema;

pub use schema::{FieldError, ValidPage, validate_page};

/// Slug halaman: `[a-z0-9-]`, 1–40, tidak diawali/diakhiri `-`.
pub fn is_valid_page_slug(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 40
        && !s.starts_with('-')
        && !s.ends_with('-')
        && s.bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
}
