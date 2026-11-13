//! P8-5: rollback image tidak macet di langkah migrate. Database yang sudah memuat migrasi dari rilis LEBIH
//! BARU (tidak dikenal binary ini) tetap bisa dijalankan `deploy_migrator()`; MIGRATOR bawaan (dipakai uji) menolak.
//! Checksum migrasi yang dikenal tetap diverifikasi.

use sqlx::PgPool;
use umkm_database::{MIGRATOR, deploy_migrator};

async fn add_future_migration(pool: &PgPool) {
    sqlx::query(
        "INSERT INTO _sqlx_migrations (version, description, success, checksum, execution_time)
         VALUES (99990101000000, 'rilis berikutnya', true, '\\x00', 1)",
    )
    .execute(pool)
    .await
    .unwrap();
}

#[sqlx::test(migrations = false)]
async fn older_binary_tolerates_newer_applied_migrations(pool: PgPool) {
    deploy_migrator().run(&pool).await.unwrap();
    add_future_migration(&pool).await;
    // Binary lama (yang tidak mengenal 99990101000000) tetap bisa menjalankan migrate → api boleh start.
    deploy_migrator().run(&pool).await.unwrap();
    // Kontrol: migrator ketat menolak (perilaku yang dulu membuat rollback macet).
    let err = MIGRATOR.run(&pool).await.unwrap_err().to_string();
    assert!(err.contains("missing"), "{err}");
}

#[sqlx::test(migrations = false)]
async fn edited_applied_migration_is_still_rejected(pool: PgPool) {
    deploy_migrator().run(&pool).await.unwrap();
    // Simulasikan file migrasi yang sudah diterapkan lalu diedit: checksum di DB tidak cocok.
    sqlx::query("UPDATE _sqlx_migrations SET checksum = '\\x01' WHERE version = (SELECT min(version) FROM _sqlx_migrations)")
        .execute(&pool)
        .await
        .unwrap();
    let err = deploy_migrator().run(&pool).await.unwrap_err().to_string();
    assert!(
        err.contains("modified") || err.contains("checksum"),
        "{err}"
    );
}
