//! P1-9: purge retensi ADR-0012, dijalankan sebagai ROLE APLIKASI (seperti worker).

use std::time::Duration;

use sqlx::{PgPool, postgres::PgPoolOptions};
use umkm_database::MIGRATOR;
use umkm_infrastructure::{
    metrics::Metrics,
    retention::{PurgeReport, RetentionPolicy, purge_job, run_purge},
};
use uuid::Uuid;

const DAY: u64 = 86_400;

fn policy() -> RetentionPolicy {
    RetentionPolicy {
        audit: Duration::from_secs(365 * DAY),
        closed_tenant_grace: Duration::from_secs(30 * DAY),
        auth_artifacts: Duration::from_secs(30 * DAY),
        outbox: Duration::from_secs(30 * DAY),
        conversations: Duration::from_secs(30 * 86_400),
    }
}

async fn app_pool(pool: &PgPool) -> PgPool {
    PgPoolOptions::new()
        .max_connections(2)
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

async fn exec(pool: &PgPool, sql: &str) {
    sqlx::raw_sql(sqlx::AssertSqlSafe(sql.to_owned()))
        .execute(pool)
        .await
        .unwrap();
}

async fn count(pool: &PgPool, sql: &str) -> i64 {
    sqlx::query_scalar(sqlx::AssertSqlSafe(sql.to_owned()))
        .fetch_one(pool)
        .await
        .unwrap()
}

async fn tenant(pool: &PgPool, slug: &str, closed_days_ago: Option<i64>) -> Uuid {
    let id: Uuid =
        sqlx::query_scalar("INSERT INTO tenants (name, slug) VALUES ('T', $1) RETURNING id")
            .bind(slug)
            .fetch_one(pool)
            .await
            .unwrap();
    sqlx::query("INSERT INTO tenant_domains (tenant_id, hostname, type, status, verified_at) VALUES ($1, $2 || '.ryasai.my.id', 'subdomain', 'active', now())")
        .bind(id)
        .bind(slug)
        .execute(pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO business_profiles (tenant_id, name) VALUES ($1, 'T')")
        .bind(id)
        .execute(pool)
        .await
        .unwrap();
    if let Some(d) = closed_days_ago {
        sqlx::query("UPDATE tenants SET status = 'closed', closed_at = now() - make_interval(days => $2::int) WHERE id = $1")
            .bind(id)
            .bind(d as i32)
            .execute(pool)
            .await
            .unwrap();
    }
    id
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn audit_older_than_one_year_is_purged_newer_kept(pool: PgPool) {
    exec(
        &pool,
        "INSERT INTO audit_logs (actor_type, action, resource_type, created_at) VALUES
        ('system', 'tenant.updated', 'tenant', now() - interval '400 days'),
        ('system', 'tenant.updated', 'tenant', now() - interval '366 days'),
        ('system', 'tenant.updated', 'tenant', now() - interval '300 days'),
        ('system', 'tenant.updated', 'tenant', now())",
    )
    .await;
    let r = run_purge(&app_pool(&pool).await, &policy()).await.unwrap();
    assert_eq!(r.audit_rows, 2);
    assert_eq!(count(&pool, "SELECT count(*) FROM audit_logs").await, 2);
    // Append-only tetap berlaku di luar fungsi purge
    let e = sqlx::query("DELETE FROM audit_logs")
        .execute(&pool)
        .await
        .unwrap_err();
    assert_eq!(
        e.as_database_error().unwrap().code().as_deref(),
        Some("42501")
    );
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn closed_tenants_purged_after_grace_with_their_data_but_audit_kept(pool: PgPool) {
    let old = tenant(&pool, "tutup-lama", Some(31)).await;
    let recent = tenant(&pool, "tutup-baru", Some(5)).await;
    let active = tenant(&pool, "aktif", None).await;
    sqlx::query("INSERT INTO audit_logs (tenant_id, actor_type, action, resource_type) VALUES ($1, 'system', 'tenant.updated', 'tenant')")
        .bind(old)
        .execute(&pool)
        .await
        .unwrap();

    let r = run_purge(&app_pool(&pool).await, &policy()).await.unwrap();
    assert_eq!(r.tenants, 1);
    let left: Vec<Uuid> = sqlx::query_scalar("SELECT id FROM tenants ORDER BY slug")
        .fetch_all(&pool)
        .await
        .unwrap();
    assert!(!left.contains(&old) && left.contains(&recent) && left.contains(&active));
    for t in ["tenant_domains", "business_profiles"] {
        assert_eq!(
            count(
                &pool,
                &format!("SELECT count(*) FROM {t} WHERE tenant_id = '{old}'")
            )
            .await,
            0,
            "{t} ikut terhapus"
        );
    }
    assert_eq!(
        count(
            &pool,
            &format!("SELECT count(*) FROM audit_logs WHERE tenant_id = '{old}'")
        )
        .await,
        1,
        "audit tetap (retensi 1 tahun)"
    );
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn stale_sessions_tokens_and_finished_outbox_are_purged(pool: PgPool) {
    let u: Uuid = sqlx::query_scalar(
        "INSERT INTO users (email, password_hash) VALUES ('u@x.id', '$argon2id$x') RETURNING id",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    let h = |b: u8| vec![b; 32];
    for (b, created, expires, revoked) in [
        (
            1u8,
            "now() - interval '90 days'",
            "now() - interval '60 days'",
            "NULL",
        ), // kedaluwarsa lama -> hapus
        (
            2,
            "now() - interval '90 days'",
            "now() + interval '1 day'",
            "now() - interval '40 days'",
        ), // dicabut lama -> hapus
        (3, "now()", "now() + interval '14 days'", "NULL"), // aktif -> simpan
        (
            4,
            "now() - interval '20 days'",
            "now() - interval '10 days'",
            "NULL",
        ), // kedaluwarsa baru -> simpan
    ] {
        sqlx::query(sqlx::AssertSqlSafe(format!(
            "INSERT INTO user_sessions (user_id, token_hash, created_at, expires_at, revoked_at) VALUES ($1, $2, {created}, {expires}, {revoked})"
        )))
        .bind(u)
        .bind(h(b))
        .execute(&pool)
        .await
        .unwrap();
    }
    exec(&pool, &format!(
        "INSERT INTO user_tokens (user_id, purpose, token_hash, created_at, expires_at, consumed_at) VALUES
         ('{u}', 'password_reset', '\\x{}', now() - interval '60 days', now() - interval '59 days', NULL),
         ('{u}', 'email_verification', '\\x{}', now(), now() + interval '1 day', NULL)",
        "11".repeat(32), "22".repeat(32)
    )).await;
    exec(&pool, "INSERT INTO notification_outbox (event_type, idempotency_key, status, processed_at, created_at) VALUES
        ('auth.email_verification_requested', 'k1', 'sent', now() - interval '40 days', now() - interval '40 days'),
        ('auth.email_verification_requested', 'k2', 'dead', now() - interval '40 days', now() - interval '40 days'),
        ('auth.email_verification_requested', 'k3', 'sent', now(), now()),
        ('auth.email_verification_requested', 'k4', 'pending', NULL, now() - interval '400 days')").await;

    let r = run_purge(&app_pool(&pool).await, &policy()).await.unwrap();
    assert_eq!((r.sessions, r.tokens, r.outbox_events), (2, 1, 2));
    assert_eq!(count(&pool, "SELECT count(*) FROM user_sessions").await, 2);
    assert_eq!(
        count(
            &pool,
            "SELECT count(*) FROM notification_outbox WHERE status = 'pending'"
        )
        .await,
        1,
        "pending tidak pernah dihapus"
    );
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn purge_is_idempotent_and_reports_metrics(pool: PgPool) {
    exec(&pool, "INSERT INTO audit_logs (actor_type, action, resource_type, created_at) VALUES ('system', 'tenant.updated', 'tenant', now() - interval '400 days')").await;
    let app = app_pool(&pool).await;
    assert_eq!(run_purge(&app, &policy()).await.unwrap().audit_rows, 1);
    assert_eq!(
        run_purge(&app, &policy()).await.unwrap(),
        PurgeReport::default()
    );
    let m = Metrics::new();
    purge_job(&app, &policy(), &m).await;
    assert!(
        m.encode()
            .contains(r#"worker_jobs_total{job="retention.purge"} 1"#)
    );
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn database_floors_reject_dangerous_cutoffs_even_if_config_is_wrong(pool: PgPool) {
    let app = app_pool(&pool).await;
    exec(&pool, "INSERT INTO audit_logs (actor_type, action, resource_type) VALUES ('system', 'tenant.updated', 'tenant')").await;
    for sql in [
        "SELECT app.purge_audit(now())",
        "SELECT app.purge_audit(now() - interval '89 days')",
        "SELECT app.purge_closed_tenants(interval '0 days')",
        "SELECT * FROM app.purge_auth_artifacts(now())",
        "SELECT app.purge_outbox(now())",
    ] {
        let e = sqlx::query(sqlx::AssertSqlSafe(sql))
            .execute(&app)
            .await
            .unwrap_err();
        assert_eq!(
            e.as_database_error().unwrap().code().as_deref(),
            Some("22023"),
            "{sql}"
        );
    }
    assert_eq!(
        count(&pool, "SELECT count(*) FROM audit_logs").await,
        1,
        "tidak ada yang terhapus"
    );
}
