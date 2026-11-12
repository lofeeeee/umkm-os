//! Adapter eksternal (Redis, storage, email, LLM, payment) yang mengimplementasikan port domain.
//!
//! Batas lapisan: `docs/decisions/0013-batas-crate-workspace.md`.

pub mod audit_repo;
pub mod auth;
pub mod auth_repo;
pub mod business_repo;
pub mod catalog_repo;
pub mod commerce_repo;
pub mod crm_repo;
pub mod customer_auth_repo;
pub mod inventory_repo;
pub mod metrics;
pub mod onboarding_repo;
pub mod outbox;
pub mod payment_repo;
pub(crate) mod pgx;
pub mod rate_limit;
pub mod rbac_repo;
pub mod redis;
pub mod retention;
pub mod storage;
pub mod team_repo;
pub mod tenant_repo;
pub mod website_repo;

/// Nama crate, dipakai untuk smoke test wiring workspace.
pub const CRATE: &str = "umkm-infrastructure";

#[cfg(test)]
mod tests {
    #[test]
    fn crate_name() {
        assert_eq!(super::CRATE, "umkm-infrastructure");
    }
}
