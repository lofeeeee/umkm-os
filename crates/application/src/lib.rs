//! Use case: orkestrasi domain + port. Tidak berisi detail HTTP atau SQL.
//!
//! Batas lapisan: `docs/decisions/0013-batas-crate-workspace.md`.

pub mod ai;
pub mod notify;
pub mod onboarding;
pub mod website;

/// Nama crate, dipakai untuk smoke test wiring workspace.
pub const CRATE: &str = "umkm-application";

#[cfg(test)]
mod tests {
    #[test]
    fn crate_name() {
        assert_eq!(super::CRATE, "umkm-application");
    }
}
