//! Tipe dan utilitas bersama (config, error envelope, ID). Tidak bergantung pada crate lain di workspace.
//!
//! Batas lapisan: `docs/decisions/0013-batas-crate-workspace.md`.

pub mod config;
pub mod http;
pub mod ids;

/// Nama crate, dipakai untuk smoke test wiring workspace.
pub const CRATE: &str = "umkm-shared";

#[cfg(test)]
mod tests {
    #[test]
    fn crate_name() {
        assert_eq!(super::CRATE, "umkm-shared");
    }
}
