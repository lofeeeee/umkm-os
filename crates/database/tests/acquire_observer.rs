//! P8-2: lama tunggu koneksi pool dilaporkan ke observer (`db_pool_wait_seconds` di API).
//! Butuh Postgres nyata: `eval "$(scripts/dev-db.sh up)"`.

use std::{
    sync::{Arc, Mutex},
    time::Duration,
};

use sqlx::{PgPool, postgres::PgPoolOptions};
use umkm_database::{MIGRATOR, begin_tenant_tx, set_acquire_observer};
use umkm_shared::ids::TenantId;

#[sqlx::test(migrator = "MIGRATOR")]
async fn reports_wait_when_pool_exhausted(pool: PgPool) {
    let seen: Arc<Mutex<Vec<Duration>>> = Arc::default();
    let sink = seen.clone();
    set_acquire_observer(move |d| sink.lock().unwrap().push(d));

    let one = PgPoolOptions::new()
        .max_connections(1)
        .connect_with(pool.connect_options().as_ref().clone())
        .await
        .unwrap();
    let tenant = TenantId::new_v4();

    // Satu-satunya koneksi dipegang 200 ms; transaksi kedua harus menunggu selama itu.
    let held = begin_tenant_tx(&one, tenant).await.unwrap();
    let release = tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(200)).await;
        held.rollback().await.unwrap();
    });
    let tx = begin_tenant_tx(&one, tenant).await.unwrap();
    tx.rollback().await.unwrap();
    release.await.unwrap();

    let seen = seen.lock().unwrap();
    assert_eq!(seen.len(), 2, "setiap begin_tenant_tx tercatat: {seen:?}");
    assert!(
        seen[0] < Duration::from_millis(150),
        "pertama tanpa antre: {seen:?}"
    );
    assert!(
        seen[1] >= Duration::from_millis(150),
        "kedua menunggu koneksi: {seen:?}"
    );
}
