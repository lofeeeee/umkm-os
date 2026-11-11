//! P1-7: migration memperbaiki `umkm_app_role` yang sudah ada dengan atribut berbahaya.
//! Binary tes terpisah (satu tes) karena mengubah role cluster-level yang dipakai tes lain;
//! cargo menjalankan binary tes secara berurutan.

use sqlx::PgPool;
use umkm_database::MIGRATOR;

#[sqlx::test(migrator = "MIGRATOR")]
async fn migration_repairs_preexisting_role_with_bypassrls(pool: PgPool) {
    // Simulasi: seseorang memberi BYPASSRLS ke role aplikasi di luar migration
    sqlx::query("ALTER ROLE umkm_app_role BYPASSRLS")
        .execute(&pool)
        .await
        .unwrap();
    let bad: bool =
        sqlx::query_scalar("SELECT rolbypassrls FROM pg_roles WHERE rolname = 'umkm_app_role'")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert!(bad);

    // Re-apply 0007 (rollback ke 6 lalu migrate ulang)
    MIGRATOR.undo(&pool, 6).await.unwrap();
    MIGRATOR.run(&pool).await.unwrap();

    let fixed: bool =
        sqlx::query_scalar("SELECT rolbypassrls FROM pg_roles WHERE rolname = 'umkm_app_role'")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert!(!fixed, "migration harus mencabut BYPASSRLS");

    // Satu koneksi: SET ROLE berlaku per sesi.
    let mut conn = pool.acquire().await.unwrap();
    sqlx::query("SET ROLE umkm_app_role")
        .execute(&mut *conn)
        .await
        .unwrap();
    let (sup, bypass): (bool, bool) =
        sqlx::query_as("SELECT rolsuper, rolbypassrls FROM pg_roles WHERE rolname = current_user")
            .fetch_one(&mut *conn)
            .await
            .unwrap();
    assert!(
        !sup && !bypass,
        "role sesi aplikasi tidak boleh melewati RLS"
    );
    sqlx::query("RESET ROLE").execute(&mut *conn).await.unwrap();
}
