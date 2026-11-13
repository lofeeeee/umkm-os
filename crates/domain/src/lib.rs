//! Aturan bisnis murni. DILARANG bergantung pada axum, sqlx, reqwest, atau tokio (`PRD.md §7.2`).
//!
//! Batas lapisan: `docs/decisions/0013-batas-crate-workspace.md`.

pub mod ai;
pub mod approvals;
pub mod audit;
pub mod auth;
pub mod business;
pub mod catalog;
pub mod commerce;
pub mod crm;
pub mod inventory;
pub mod mfa;
pub mod rbac;
pub mod storage;
pub mod team;
pub mod tenant;
pub mod tickets;
pub mod website;

/// Nama crate, dipakai untuk smoke test wiring workspace.
pub const CRATE: &str = "umkm-domain";

#[cfg(test)]
mod tests {
    #[test]
    fn crate_name() {
        assert_eq!(super::CRATE, "umkm-domain");
    }
}
