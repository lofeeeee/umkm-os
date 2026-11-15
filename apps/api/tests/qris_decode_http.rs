mod common;

// Fixture QR dipakai oleh Task 2+; impor wildcard sengaja dipertahankan untuk dekoder HTTP.
#[allow(unused_imports)]
use common::*;
use sqlx::PgPool;
use umkm_database::MIGRATOR;

#[sqlx::test(migrator = "MIGRATOR")]
async fn qris_payload_column_exists(pool: PgPool) {
    let exists: bool = sqlx::query_scalar(
        "SELECT EXISTS (
             SELECT 1 FROM information_schema.columns
             WHERE table_schema = 'public'
               AND table_name = 'commerce_settings'
               AND column_name = 'qris_static_payload'
           )",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert!(exists, "migration 0039 belum menambah qris_static_payload");
}
