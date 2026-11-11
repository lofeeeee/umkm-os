//! Readiness check untuk dependensi nyata (Postgres, Redis).

use sqlx::PgPool;
use umkm_infrastructure::redis::RedisClient;

use crate::{ReadinessCheck, state::CheckFuture};

pub struct Postgres(pub PgPool);

impl ReadinessCheck for Postgres {
    fn name(&self) -> &'static str {
        "postgres"
    }
    fn check(&self) -> CheckFuture<'_> {
        Box::pin(async move {
            umkm_database::ping(&self.0)
                .await
                .map_err(|e| e.to_string())
        })
    }
}

pub struct Redis(pub RedisClient);

impl ReadinessCheck for Redis {
    fn name(&self) -> &'static str {
        "redis"
    }
    fn check(&self) -> CheckFuture<'_> {
        Box::pin(async move { self.0.ping().await.map_err(|e| e.to_string()) })
    }
}
