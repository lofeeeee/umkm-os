//! umkm-api — HTTP API (Axum). Lapisan: handler tipis -> application use case (`PRD.md §7.2`).

pub mod approvals;
pub mod audit;
pub mod auth;
pub mod bootstrap;
pub mod business;
pub mod catalog;
pub mod commerce;
pub mod crm;
pub mod customer_auth;
pub mod error;
pub mod health;
pub mod http_util;
pub mod inventory;
pub mod metrics_server;
pub mod middleware;
pub mod notifications;
pub mod openapi;
pub mod readiness;
pub mod realtime;
pub mod router;
pub mod session;
pub mod state;
pub mod team;
pub mod tenants;
pub mod tickets;
pub mod upload;
pub mod validation;
pub mod website;

pub use error::ApiError;
pub use router::{HttpLimits, build_router, with_middleware};
pub use state::{AppState, ReadinessCheck};
