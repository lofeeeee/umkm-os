//! P1-10 (PRD.md §13.2) tingkat data/worker/cache, sebagai ROLE APLIKASI:
//! #4 penjaga policy semua tabel tenant, #5 background job, #6 cache Redis,
//! #8 pooled connection reuse, #9 app role tidak bypass RLS.

use std::time::Duration;

use sqlx::{PgPool, Row, postgres::PgPoolOptions};
use umkm_application::notify::OutboxEvent;
use umkm_database::{MIGRATOR, app_role, begin_tenant_tx};
use umkm_domain::auth::AuthPolicy;
use umkm_infrastructure::{
    metrics::Metrics,
    outbox::{self, CapturingEmailSender, WorkerConfig, process_once},
    redis::{KeyNamespace, RedisClient},
};
use umkm_shared::{config::SecretString, ids::TenantId};
use uuid::Uuid;

async fn app_pool(pool: &PgPool, max: u32) -> PgPool {
    PgPoolOptions::new()
        .max_connections(max)
        .after_connect(|c, _| {
            Box::pin(async move {
                sqlx::query("SET ROLE umkm_app_role")
                    .execute(c)
                    .await
                    .map(|_| ())
            })
        })
        .connect_with(pool.connect_options().as_ref().clone())
        .await
        .unwrap()
}

/// Tenant + profil dibuat sebagai owner (setup), diuji sebagai role aplikasi.
async fn tenant(pool: &PgPool, slug: &str, profile: &str) -> TenantId {
    let id: Uuid =
        sqlx::query_scalar("INSERT INTO tenants (name, slug) VALUES ($1, $2) RETURNING id")
            .bind(profile)
            .bind(slug)
            .fetch_one(pool)
            .await
            .unwrap();
    sqlx::query("INSERT INTO business_profiles (tenant_id, name) VALUES ($1, $2)")
        .bind(id)
        .bind(profile)
        .execute(pool)
        .await
        .unwrap();
    TenantId::from_uuid(id)
}

fn cfg() -> WorkerConfig {
    WorkerConfig {
        worker_id: "w".into(),
        batch: 10,
        lease: Duration::from_secs(30),
        max_attempts: 3,
        base_backoff: Duration::from_secs(10),
        verify_url_base: "https://app.ryasai.my.id/verify-email".into(),
        policy: AuthPolicy::default(),
    }
}

// ------------------------------------------------------------------ #4
/// Konversasi/AI config/ticket/order/file belum ada (P3/P5/P6). Penjaga ini memastikan
/// SETIAP tabel ber-tenant_id kini dan nanti: RLS ENABLE+FORCE dan semua policy-nya
/// memfilter dengan app.current_tenant_id() (kecuali allowlist beralasan).
#[sqlx::test(migrator = "MIGRATOR")]
async fn t4_every_tenant_table_policy_filters_by_current_tenant(pool: PgPool) {
    const EXEMPT: [&str; 1] = ["notification_outbox"];
    let tables: Vec<String> = sqlx::query_scalar(
        "SELECT c.relname FROM pg_class c JOIN pg_namespace n ON n.oid = c.relnamespace
         WHERE n.nspname = 'public' AND c.relkind = 'r'
           AND EXISTS (SELECT 1 FROM pg_attribute a WHERE a.attrelid = c.oid AND a.attname = 'tenant_id' AND NOT a.attisdropped)",
    )
    .fetch_all(&pool)
    .await
    .unwrap();
    assert!(tables.len() >= 7);
    for t in tables.iter().filter(|t| !EXEMPT.contains(&t.as_str())) {
        let policies = sqlx::query("SELECT policyname, cmd, coalesce(qual, '') AS qual, coalesce(with_check, '') AS wc FROM pg_policies WHERE schemaname = 'public' AND tablename = $1")
            .bind(t)
            .fetch_all(&pool)
            .await
            .unwrap();
        assert!(!policies.is_empty(), "{t}: tidak ada policy");
        for p in policies {
            let (name, qual, wc): (String, String, String) =
                (p.get("policyname"), p.get("qual"), p.get("wc"));
            let expr = format!("{qual} {wc}");
            assert!(
                expr.contains("current_tenant_id()"),
                "{t}.{name}: policy tidak memfilter tenant: {expr}"
            );
            assert!(
                !expr.trim().eq_ignore_ascii_case("true")
                    && !qual.trim().eq_ignore_ascii_case("true"),
                "{t}.{name}: policy permisif"
            );
        }
    }
}

// ------------------------------------------------------------------ #5
#[sqlx::test(migrator = "MIGRATOR")]
async fn t5_background_job_processes_only_its_tenant(pool: PgPool) {
    let a = tenant(&pool, "toko-a", "Profil A").await;
    let b = tenant(&pool, "toko-b", "Profil B").await;
    let app = app_pool(&pool, 3).await;
    // Probe sah untuk A dan B, plus probe "jahat": event tenant A meminta resource B
    for ev in [
        OutboxEvent::tenant_guard_probe(*a.as_uuid(), *a.as_uuid(), Uuid::new_v4()),
        OutboxEvent::tenant_guard_probe(*b.as_uuid(), *b.as_uuid(), Uuid::new_v4()),
        OutboxEvent::tenant_guard_probe(*a.as_uuid(), *b.as_uuid(), Uuid::new_v4()),
    ] {
        outbox::enqueue(&app, &ev).await.unwrap();
    }
    let t = process_once(
        &app,
        &CapturingEmailSender::default(),
        &cfg(),
        &Metrics::new(),
    )
    .await
    .unwrap();
    assert_eq!((t.sent, t.dead), (2, 1));

    // Setiap jejak hasil job berada di tenant event dan hanya memuat data tenant itu
    let rows: Vec<(Uuid, String)> = sqlx::query_as(
        "SELECT tenant_id, summary->>'profile_name' FROM audit_logs WHERE action = 'tenant.guard_probe.processed' ORDER BY summary->>'profile_name'",
    )
    .fetch_all(&pool)
    .await
    .unwrap();
    assert_eq!(
        rows,
        vec![
            (*a.as_uuid(), "Profil A".into()),
            (*b.as_uuid(), "Profil B".into())
        ]
    );
    let dead: (String, Option<String>) =
        sqlx::query_as("SELECT status, last_error FROM notification_outbox WHERE status = 'dead'")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(dead, ("dead".into(), Some("resource_not_in_tenant".into())));
}

// ------------------------------------------------------------------ #6
#[tokio::test]
async fn t6_cache_keys_never_collide_across_tenants_in_redis() {
    let url = std::env::var("REDIS_URL").expect("REDIS_URL wajib");
    let mut conn = redis::Client::open(url.clone())
        .unwrap()
        .get_multiplexed_async_connection()
        .await
        .unwrap();
    let _ = RedisClient::open(&SecretString::new(url)).unwrap();
    let ns = KeyNamespace::new(&format!("umkmtest:{}", Uuid::new_v4().simple())).unwrap();
    let (a, b) = (TenantId::new_v4(), TenantId::new_v4());
    // Resource id identik di dua tenant
    let ka = ns.tenant(a).key("products", "p-1").unwrap();
    let kb = ns.tenant(b).key("products", "p-1").unwrap();
    let _: () = redis::cmd("SET")
        .arg(ka.as_str())
        .arg("milik-A")
        .query_async(&mut conn)
        .await
        .unwrap();
    let _: () = redis::cmd("SET")
        .arg(kb.as_str())
        .arg("milik-B")
        .query_async(&mut conn)
        .await
        .unwrap();
    let va: String = redis::cmd("GET")
        .arg(ka.as_str())
        .query_async(&mut conn)
        .await
        .unwrap();
    let vb: String = redis::cmd("GET")
        .arg(kb.as_str())
        .query_async(&mut conn)
        .await
        .unwrap();
    assert_eq!((va.as_str(), vb.as_str()), ("milik-A", "milik-B"));
    // SCAN dengan pola tenant A hanya menemukan key A
    let found: Vec<String> = redis::cmd("KEYS")
        .arg(ns.tenant(a).scan_pattern())
        .query_async(&mut conn)
        .await
        .unwrap();
    assert_eq!(found, vec![ka.as_str().to_owned()]);
    let _: i64 = redis::cmd("DEL")
        .arg(ka.as_str())
        .arg(kb.as_str())
        .query_async(&mut conn)
        .await
        .unwrap();
}

// ------------------------------------------------------------------ #8
#[sqlx::test(migrator = "MIGRATOR")]
async fn t8_rls_holds_on_pooled_connection_reuse(pool: PgPool) {
    let a = tenant(&pool, "toko-a", "Profil A").await;
    let b = tenant(&pool, "toko-b", "Profil B").await;
    let app = app_pool(&pool, 1).await; // satu koneksi fisik
    for (t, other) in [(a, b), (b, a), (a, b)] {
        let mut tx = begin_tenant_tx(&app, t).await.unwrap();
        let names: Vec<String> = sqlx::query_scalar("SELECT name FROM business_profiles")
            .fetch_all(&mut *tx)
            .await
            .unwrap();
        let ids: Vec<Uuid> = sqlx::query_scalar("SELECT id FROM tenants")
            .fetch_all(&mut *tx)
            .await
            .unwrap();
        tx.commit().await.unwrap();
        assert_eq!(ids, vec![*t.as_uuid()]);
        assert!(!ids.contains(other.as_uuid()));
        assert_eq!(names.len(), 1);
        // Setelah commit pada koneksi yang sama: tanpa context -> kosong
        let leaked: i64 = sqlx::query_scalar("SELECT count(*) FROM business_profiles")
            .fetch_one(&app)
            .await
            .unwrap();
        assert_eq!(leaked, 0);
    }
}

// ------------------------------------------------------------------ #9
/// Login NYATA seperti produksi (`umkm_app`), bukan superuser + `SET ROLE`: dengan
/// `SET ROLE` sesi masih bisa kembali ke superuser, sehingga tidak membuktikan apa pun
/// (ditemukan saat menulis tes ini; lihat docs/evidence/P1-10.txt).
async fn real_app_login(pool: &PgPool) -> PgPool {
    let db: String = sqlx::query_scalar("SELECT current_database()")
        .fetch_one(pool)
        .await
        .unwrap();
    sqlx::raw_sql(sqlx::AssertSqlSafe(format!(
        "DO $$ BEGIN
             BEGIN CREATE ROLE umkm_app_probe LOGIN PASSWORD 'probe-only-not-secret' NOSUPERUSER NOBYPASSRLS;
             EXCEPTION WHEN duplicate_object THEN NULL; END;
         END $$;
         GRANT umkm_app_role TO umkm_app_probe;
         GRANT CONNECT ON DATABASE \"{db}\" TO umkm_app_probe;"
    )))
    .execute(pool)
    .await
    .unwrap();
    let opts = pool
        .connect_options()
        .as_ref()
        .clone()
        .username("umkm_app_probe")
        .password("probe-only-not-secret");
    PgPoolOptions::new()
        .max_connections(1)
        .connect_with(opts)
        .await
        .unwrap()
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn t9_app_role_cannot_bypass_rls(pool: PgPool) {
    tenant(&pool, "toko-a", "Profil A").await;
    let app = real_app_login(&pool).await;
    let who: (String, String) = sqlx::query_as("SELECT session_user::text, current_user::text")
        .fetch_one(&app)
        .await
        .unwrap();
    assert_eq!(who, ("umkm_app_probe".into(), "umkm_app_probe".into()));
    let info = app_role::current_role(&app).await.unwrap();
    assert!(!info.superuser && !info.bypass_rls, "{info:?}");
    // Upaya mematikan RLS / naik ke owner / mematikan trigger / menghapus policy
    for sql in [
        "ALTER TABLE tenants DISABLE ROW LEVEL SECURITY",
        "ALTER TABLE tenants NO FORCE ROW LEVEL SECURITY",
        "SET ROLE umkm_owner",
        "SET session_replication_role = replica",
        "DROP POLICY tenant_isolation ON tenants",
        "ALTER ROLE umkm_app_probe BYPASSRLS",
    ] {
        let e = sqlx::raw_sql(sqlx::AssertSqlSafe(sql)).execute(&app).await;
        assert!(e.is_err(), "harus ditolak: {sql}");
    }
    let n: i64 = sqlx::query_scalar("SELECT count(*) FROM tenants")
        .fetch_one(&app)
        .await
        .unwrap();
    assert_eq!(n, 0, "tetap tidak terlihat tanpa context");
    app.close().await;
}
