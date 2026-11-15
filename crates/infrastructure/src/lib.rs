//! Adapter eksternal (Redis, storage, email, LLM, payment) yang mengimplementasikan port domain.
//!
//! Batas lapisan: `docs/decisions/0013-batas-crate-workspace.md`.

pub mod ai;
pub mod ai_repo;
pub mod analytics_repo;
pub mod approval_repo;
pub mod audit_repo;
pub mod auth;
pub mod auth_repo;
pub mod billing_repo;
pub mod business_repo;
pub mod catalog_repo;
pub mod commerce_repo;
pub mod conversation_repo;
pub mod crm_repo;
pub mod crypto;
pub mod customer_auth_repo;
pub mod gopay;
pub mod gopay_repo;
pub mod http_client;
pub mod image_variant;
pub mod inventory_repo;
pub mod metrics;
pub mod mfa;
pub mod notification_repo;
pub mod onboarding_repo;
pub mod outbox;
pub mod payment_repo;
pub(crate) mod pgx;
pub mod qris_decode;
pub mod rag;
pub mod rate_limit;
pub mod rbac_repo;
pub mod realtime;
pub mod redis;
pub mod retention;
pub mod storage;
pub mod team_repo;
pub mod tenant_repo;
pub mod ticket_repo;
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
