//! P1-7: RLS ENABLE + FORCE dibuktikan dengan ROLE APLIKASI (bukan superuser,
//! yang selalu melewati RLS). Setiap koneksi pool uji menjalankan `SET ROLE umkm_app_role`.

use std::{sync::Arc, time::Duration};

use sqlx::{PgPool, Row, postgres::PgPoolOptions};
use umkm_application::onboarding::{LegalVersions, RegisterCommand, register};
use umkm_database::{MIGRATOR, app_role, begin_tenant_tx};
use umkm_domain::auth::AuthPolicy;
use umkm_infrastructure::{
    auth::Argon2idHasher,
    metrics::Metrics,
    onboarding_repo::{BlockingArgon2, OsTokenIssuer, PgRegistrationStore},
    outbox::{CapturingEmailSender, WorkerConfig, process_once},
    rbac_repo, tenant_repo,
};
use umkm_shared::ids::TenantId;
use uuid::Uuid;

/// Tabel milik tenant yang WAJIB ber-RLS (selain yang di allowlist katalog).
const TENANT_TABLES: [&str; 22] = [
    "audit_logs",
    "tenants",
    "tenant_memberships",
    "tenant_domains",
    "invitations",
    "roles",
    "role_permissions",
    "business_profiles",
    "tenant_websites",
    "website_pages",
    "website_page_versions",
    "stored_objects",
    "product_categories",
    "products",
    "product_variants",
    "product_images",
    "inventory_levels",
    "inventory_movements",
    "inventory_reservations",
    "business_addresses",
    "business_hours",
    "business_hour_exceptions",
];
/// Tabel dengan kolom tenant_id yang sengaja tanpa RLS + alasannya.
const RLS_EXEMPT: [(&str, &str); 1] = [(
    "notification_outbox",
    "infrastruktur platform; worker lintas tenant (BUILD_PLAN.md §3.4)",
)];

async fn app_pool(pool: &PgPool) -> PgPool {
    PgPoolOptions::new()
        .max_connections(3)
        .after_connect(|conn, _| {
            Box::pin(async move {
                sqlx::query("SET ROLE umkm_app_role").execute(conn).await?;
                Ok(())
            })
        })
        .connect_with(pool.connect_options().as_ref().clone())
        .await
        .unwrap()
}

async fn register_as_app(app: &PgPool, email: &str, name: &str) -> (Uuid, TenantId) {
    let hasher = BlockingArgon2(Arc::new(Argon2idHasher::new(1024, 1, 1).unwrap()));
    let cmd = RegisterCommand {
        email: email.into(),
        password: "rahasia-123".into(),
        business_name: name.into(),
        slug: None,
        accept_terms: true,
        accept_privacy: true,
        user_agent: None,
        request_id: None,
        client_ip: None,
    };
    let legal = LegalVersions {
        privacy_policy: "v0".into(),
        terms_of_service: "v0".into(),
    };
    let r = register(
        &PgRegistrationStore::new(app.clone()),
        &hasher,
        &OsTokenIssuer,
        cmd,
        "ryasai.my.id",
        &AuthPolicy::default(),
        &legal,
    )
    .await
    .unwrap()
    .registered;
    (r.user_id, r.tenant_id)
}

async fn count_in(app: &PgPool, tenant: Option<TenantId>, table: &str) -> i64 {
    let sql = sqlx::AssertSqlSafe(format!("SELECT count(*) FROM {table}"));
    match tenant {
        Some(t) => {
            let mut tx = begin_tenant_tx(app, t).await.unwrap();
            let n = sqlx::query_scalar(sql).fetch_one(&mut *tx).await.unwrap();
            tx.commit().await.unwrap();
            n
        }
        None => sqlx::query_scalar(sql).fetch_one(app).await.unwrap(),
    }
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn app_role_is_not_superuser_and_cannot_bypass_rls(pool: PgPool) {
    let app = app_pool(&pool).await;
    let info = app_role::current_role(&app).await.unwrap();
    assert_eq!(info.name, "umkm_app_role");
    assert!(info.rls_enforced(), "{info:?}");
    // Kontrol: koneksi uji default (owner) memang superuser -> karena itu tes RLS wajib SET ROLE
    assert!(!app_role::current_role(&pool).await.unwrap().rls_enforced());
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn every_tenant_table_has_forced_rls(pool: PgPool) {
    // Katalog: setiap tabel public dengan kolom tenant_id (atau tabel `tenants`) wajib RLS+FORCE.
    let rows = sqlx::query(
        "SELECT c.relname, c.relrowsecurity, c.relforcerowsecurity
         FROM pg_class c JOIN pg_namespace n ON n.oid = c.relnamespace
         WHERE n.nspname = 'public' AND c.relkind = 'r'
           AND (c.relname = 'tenants' OR EXISTS (SELECT 1 FROM pg_attribute a WHERE a.attrelid = c.oid AND a.attname = 'tenant_id' AND NOT a.attisdropped)
                OR c.relname = 'role_permissions')",
    )
    .fetch_all(&pool)
    .await
    .unwrap();
    let mut covered = vec![];
    for r in rows {
        let (name, enabled, forced): (String, bool, bool) = (r.get(0), r.get(1), r.get(2));
        if RLS_EXEMPT.iter().any(|(t, _)| *t == name) {
            continue;
        }
        assert!(
            enabled && forced,
            "tabel {name} punya tenant_id tetapi RLS ENABLE+FORCE tidak aktif"
        );
        covered.push(name);
    }
    covered.sort();
    let mut want: Vec<String> = TENANT_TABLES.iter().map(|s| s.to_string()).collect();
    want.sort();
    assert_eq!(covered, want);
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn without_tenant_context_tenant_tables_are_empty(pool: PgPool) {
    let app = app_pool(&pool).await;
    register_as_app(&app, "a@x.id", "Toko A").await;
    for t in [
        "tenants",
        "tenant_memberships",
        "tenant_domains",
        "invitations",
        "business_profiles",
    ] {
        assert_eq!(
            count_in(&app, None, t).await,
            0,
            "{t} bocor tanpa tenant context"
        );
    }
    // Preset sistem tetap terbaca
    assert_eq!(count_in(&app, None, "roles").await, 6);
    // Kontrol: owner melihat datanya
    assert_eq!(count_in(&pool, None, "tenants").await, 1);
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn tenant_a_cannot_read_or_modify_tenant_b(pool: PgPool) {
    let app = app_pool(&pool).await;
    let (_, a) = register_as_app(&app, "a@x.id", "Toko A").await;
    let (_, b) = register_as_app(&app, "b@x.id", "Toko B").await;
    for t in [
        "tenants",
        "tenant_memberships",
        "tenant_domains",
        "business_profiles",
    ] {
        assert_eq!(
            count_in(&app, Some(a), t).await,
            1,
            "{t}: A hanya melihat miliknya"
        );
    }

    let mut tx = begin_tenant_tx(&app, a).await.unwrap();
    // Baca langsung by id milik B -> tidak ada
    let seen: Option<Uuid> = sqlx::query_scalar("SELECT id FROM tenants WHERE id = $1")
        .bind(b.as_uuid())
        .fetch_optional(&mut *tx)
        .await
        .unwrap();
    assert!(seen.is_none());
    // UPDATE/DELETE baris B -> 0 baris
    assert_eq!(
        sqlx::query("UPDATE tenants SET name = 'diretas' WHERE id = $1")
            .bind(b.as_uuid())
            .execute(&mut *tx)
            .await
            .unwrap()
            .rows_affected(),
        0
    );
    assert_eq!(
        sqlx::query("DELETE FROM business_profiles WHERE tenant_id = $1")
            .bind(b.as_uuid())
            .execute(&mut *tx)
            .await
            .unwrap()
            .rows_affected(),
        0
    );
    tx.commit().await.unwrap();

    // INSERT dengan tenant_id B dari konteks A -> ditolak WITH CHECK
    let mut tx = begin_tenant_tx(&app, a).await.unwrap();
    let err = sqlx::query("INSERT INTO tenant_domains (tenant_id, hostname, type) VALUES ($1, 'curian.ryasai.my.id', 'subdomain')")
        .bind(b.as_uuid())
        .execute(&mut *tx)
        .await
        .unwrap_err();
    assert_eq!(
        err.as_database_error().unwrap().code().as_deref(),
        Some("42501"),
        "{err}"
    );
    drop(tx);

    // Data B utuh (dicek sebagai owner)
    let name: String = sqlx::query_scalar("SELECT name FROM tenants WHERE id = $1")
        .bind(b.as_uuid())
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(name, "Toko B");
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn custom_roles_are_tenant_private_and_presets_read_only(pool: PgPool) {
    let app = app_pool(&pool).await;
    let (_, a) = register_as_app(&app, "a@x.id", "Toko A").await;
    let (_, b) = register_as_app(&app, "b@x.id", "Toko B").await;
    let mut tx = begin_tenant_tx(&app, a).await.unwrap();
    sqlx::query("INSERT INTO roles (tenant_id, name) VALUES ($1, 'Kasir A')")
        .bind(a.as_uuid())
        .execute(&mut *tx)
        .await
        .unwrap();
    // Mengubah preset sistem tidak mungkin dari role aplikasi
    assert_eq!(
        sqlx::query("UPDATE roles SET name = 'Hacked' WHERE tenant_id IS NULL")
            .execute(&mut *tx)
            .await
            .unwrap()
            .rows_affected(),
        0
    );
    let n = sqlx::query("DELETE FROM role_permissions WHERE role_id IN (SELECT id FROM roles WHERE tenant_id IS NULL)")
        .execute(&mut *tx)
        .await
        .unwrap()
        .rows_affected();
    assert_eq!(n, 0, "permission preset tidak dapat dihapus");
    tx.commit().await.unwrap();
    assert_eq!(
        count_in(&app, Some(a), "roles").await,
        7,
        "6 preset + 1 kustom A"
    );
    assert_eq!(
        count_in(&app, Some(b), "roles").await,
        6,
        "B tidak melihat role A"
    );
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn application_flows_work_under_app_role(pool: PgPool) {
    let app = app_pool(&pool).await;
    let (u, a) = register_as_app(&app, "a@x.id", "Toko A").await;
    let (_, b) = register_as_app(&app, "b@x.id", "Toko B").await;

    // RBAC
    let actor = rbac_repo::load_actor(&app, u, a).await.unwrap().unwrap();
    assert_eq!(actor.permissions().count(), 33);
    assert!(rbac_repo::load_actor(&app, u, b).await.unwrap().is_none());
    let t = rbac_repo::get_tenant(&app, &actor).await.unwrap().unwrap();
    assert_eq!(t.primary_hostname.as_deref(), Some("toko-a.ryasai.my.id"));
    let mine = rbac_repo::list_memberships(&app, u).await.unwrap();
    assert_eq!(
        mine.iter().map(|m| m.slug.as_str()).collect::<Vec<_>>(),
        ["toko-a"]
    );

    // Resolver host lintas tenant lewat SECURITY DEFINER
    let rec = tenant_repo::resolve_host(&app, "toko-b.ryasai.my.id")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(rec.tenant_id, b);

    // Worker outbox
    let sender = CapturingEmailSender::default();
    let cfg = WorkerConfig {
        worker_id: "w".into(),
        batch: 10,
        lease: Duration::from_secs(30),
        max_attempts: 3,
        base_backoff: Duration::from_secs(10),
        verify_url_base: "https://app.ryasai.my.id/verify-email".into(),
        policy: AuthPolicy::default(),
    };
    assert_eq!(
        process_once(&app, &sender, &cfg, &Metrics::new())
            .await
            .unwrap()
            .sent,
        2
    );
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn connection_reuse_does_not_leak_context_under_rls(pool: PgPool) {
    let app = PgPoolOptions::new()
        .max_connections(1)
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
        .unwrap();
    let (_, a) = register_as_app(&app, "a@x.id", "Toko A").await;
    assert_eq!(count_in(&app, Some(a), "tenants").await, 1);
    // Koneksi fisik sama, transaksi berikut tanpa context -> 0
    assert_eq!(count_in(&app, None, "tenants").await, 0);
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn app_role_cannot_touch_migrations_or_seed_tables(pool: PgPool) {
    let app = app_pool(&pool).await;
    let e = sqlx::query("SELECT count(*) FROM _sqlx_migrations")
        .fetch_one(&app)
        .await
        .unwrap_err();
    assert_eq!(
        e.as_database_error().unwrap().code().as_deref(),
        Some("42501")
    );
    let e = sqlx::query("INSERT INTO permissions (permission_key) VALUES ('evil.grant')")
        .execute(&app)
        .await
        .unwrap_err();
    assert_eq!(
        e.as_database_error().unwrap().code().as_deref(),
        Some("42501")
    );
    let e = sqlx::query("DELETE FROM reserved_subdomains")
        .execute(&app)
        .await
        .unwrap_err();
    assert_eq!(
        e.as_database_error().unwrap().code().as_deref(),
        Some("42501")
    );
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn startup_guard_rejects_superuser_outside_development(pool: PgPool) {
    // Owner uji = superuser
    assert!(app_role::ensure_rls_enforced(&pool, false).await.is_err());
    assert!(
        app_role::ensure_rls_enforced(&pool, true).await.is_ok(),
        "development: hanya peringatan"
    );
    let app = app_pool(&pool).await;
    assert!(app_role::ensure_rls_enforced(&app, false).await.is_ok());
}
