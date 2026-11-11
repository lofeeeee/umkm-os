//! umkm-api — HTTP API (Axum). Lapisan: handler tipis -> application use case (`PRD.md §7.2`).

pub mod auth;
pub mod bootstrap;
pub mod error;
pub mod health;
pub mod metrics_server;
pub mod middleware;
pub mod openapi;
pub mod readiness;
pub mod router;
pub mod state;

pub use error::ApiError;
pub use router::{HttpLimits, build_router, with_middleware};
pub use state::{AppState, ReadinessCheck};
