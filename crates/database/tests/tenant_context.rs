//! P0-4 (PRD.md §5.7): migration harness + tenant context transaction-local.
//! Butuh Postgres nyata: `eval "$(scripts/dev-db.sh up)"` (mengisi DATABASE_URL).
//! `#[sqlx::test]` membuat database baru per tes dan menjalankan MIGRATOR.

use sqlx::{PgPool, Row, postgres::PgPoolOptions};
use umkm_database::{MIGRATOR, begin_tenant_tx};
use umkm_shared::ids::TenantId;

async fn current_tenant<'e, E: sqlx::PgExecutor<'e>>(e: E) -> Option<uuid::Uuid> {
    sqlx::query("SELECT app.current_tenant_id() AS t")
        .fetch_one(e)
        .await
        .unwrap()
        .get("t")
}

async fn backend_pid<'e, E: sqlx::PgExecutor<'e>>(e: E) -> i32 {
    sqlx::query("SELECT pg_backend_pid() AS p")
        .fetch_one(e)
        .await
        .unwrap()
        .get("p")
}

/// Pool satu koneksi agar setiap transaksi pasti memakai koneksi fisik yang sama.
async fn single_conn(pool: &PgPool) -> PgPool {
    PgPoolOptions::new()
        .max_connections(1)
        .connect_with(pool.connect_options().as_ref().clone())
        .await
        .unwrap()
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn no_context_means_null_tenant(pool: PgPool) {
    assert_eq!(current_tenant(&pool).await, None);
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn tenant_tx_sets_context(pool: PgPool) {
    let a = TenantId::new_v4();
    let mut tx = begin_tenant_tx(&pool, a).await.unwrap();
    assert_eq!(current_tenant(&mut *tx).await, Some(*a.as_uuid()));
    tx.commit().await.unwrap();
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn context_does_not_leak_on_connection_reuse(pool: PgPool) {
    let pool = single_conn(&pool).await;
    let (a, b) = (TenantId::new_v4(), TenantId::new_v4());

    let mut tx = begin_tenant_tx(&pool, a).await.unwrap();
    let pid = backend_pid(&mut *tx).await;
    tx.commit().await.unwrap();

    // Setelah commit: koneksi sama, context harus kosong.
    let mut conn = pool.acquire().await.unwrap();
    assert_eq!(
        backend_pid(&mut *conn).await,
        pid,
        "harus koneksi fisik yang sama"
    );
    assert_eq!(
        current_tenant(&mut *conn).await,
        None,
        "context A bocor setelah commit"
    );
    drop(conn);

    // Setelah rollback: sama.
    let tx = begin_tenant_tx(&pool, b).await.unwrap();
    tx.rollback().await.unwrap();
    assert_eq!(
        current_tenant(&pool).await,
        None,
        "context B bocor setelah rollback"
    );

    // Transaksi yang di-drop tanpa commit (mis. error di tengah handler).
    {
        let _tx = begin_tenant_tx(&pool, a).await.unwrap();
    }
    assert_eq!(
        current_tenant(&pool).await,
        None,
        "context bocor setelah drop"
    );

    // Transaksi B setelah A melihat B, bukan A.
    let mut tx = begin_tenant_tx(&pool, b).await.unwrap();
    assert_eq!(backend_pid(&mut *tx).await, pid);
    assert_eq!(current_tenant(&mut *tx).await, Some(*b.as_uuid()));
    tx.commit().await.unwrap();
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn concurrent_tenants_each_see_only_their_context(pool: PgPool) {
    let pool = PgPoolOptions::new()
        .max_connections(3)
        .connect_with(pool.connect_options().as_ref().clone())
        .await
        .unwrap();
    let mut set = tokio::task::JoinSet::new();
    for _ in 0..30 {
        let pool = pool.clone();
        set.spawn(async move {
            let t = TenantId::new_v4();
            let mut tx = begin_tenant_tx(&pool, t).await.unwrap();
            sqlx::query("SELECT pg_sleep(0.01)")
                .execute(&mut *tx)
                .await
                .unwrap();
            let seen = current_tenant(&mut *tx).await;
            tx.commit().await.unwrap();
            (t, seen)
        });
    }
    for (t, seen) in set.join_all().await {
        assert_eq!(seen, Some(*t.as_uuid()));
    }
}

#[sqlx::test(migrations = false)]
async fn migrations_are_reversible(pool: PgPool) {
    MIGRATOR.run(&pool).await.unwrap();
    MIGRATOR.undo(&pool, 0).await.unwrap();
    let exists: bool = sqlx::query_scalar(
        "SELECT EXISTS (SELECT 1 FROM pg_proc WHERE proname = 'current_tenant_id')",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert!(!exists, "down migration harus menghapus fungsi");
    MIGRATOR.run(&pool).await.unwrap();
    assert_eq!(current_tenant(&pool).await, None);
}
