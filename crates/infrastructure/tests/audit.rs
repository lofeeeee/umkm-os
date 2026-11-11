//! P1-8: audit log append-only + redaksi + isolasi tenant, diuji dengan ROLE APLIKASI.

use std::sync::Arc;

use sqlx::{PgPool, postgres::PgPoolOptions};
use umkm_application::onboarding::{LegalVersions, RegisterCommand, register};
use umkm_database::{MIGRATOR, begin_tenant_tx};
use umkm_domain::{
    audit::{AuditEvent, REDACTED, actions},
    auth::AuthPolicy,
};
use umkm_infrastructure::{
    audit_repo,
    auth::Argon2idHasher,
    onboarding_repo::{BlockingArgon2, OsTokenIssuer, PgRegistrationStore},
    rbac_repo,
};
use umkm_shared::ids::TenantId;
use uuid::Uuid;

async fn app_pool(pool: &PgPool) -> PgPool {
    PgPoolOptions::new()
        .max_connections(3)
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

async fn reg(app: &PgPool, email: &str, name: &str) -> (Uuid, TenantId) {
    let hasher = BlockingArgon2(Arc::new(Argon2idHasher::new(1024, 1, 1).unwrap()));
    let cmd = RegisterCommand {
        email: email.into(),
        password: "rahasia-123".into(),
        business_name: name.into(),
        slug: None,
        accept_terms: true,
        accept_privacy: true,
        user_agent: None,
        request_id: Some("req-test-1".into()),
        client_ip: Some("203.0.113.9".into()),
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

/// (action, actor_user_id, actor_type, request_id, ip, summary)
type AuditRow = (
    String,
    Option<Uuid>,
    String,
    Option<String>,
    Option<String>,
    serde_json::Value,
);

fn sqlstate(e: &sqlx::Error) -> Option<String> {
    e.as_database_error()
        .and_then(|d| d.code().map(|c| c.into_owned()))
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn registration_and_rename_are_audited_atomically_with_context(pool: PgPool) {
    let app = app_pool(&pool).await;
    let (u, t) = reg(&app, "a@x.id", "Toko A").await;
    let actor = rbac_repo::load_actor(&app, u, t).await.unwrap().unwrap();
    assert!(
        rbac_repo::rename_tenant(
            &app,
            &actor,
            "Toko A Baru",
            Some("req-2"),
            Some("198.51.100.1")
        )
        .await
        .unwrap()
    );

    let rows: Vec<AuditRow> = sqlx::query_as(
        "SELECT action, actor_user_id, actor_type, request_id, ip, summary FROM audit_logs WHERE tenant_id = $1 ORDER BY created_at",
    )
    .bind(t.as_uuid())
    .fetch_all(&pool)
    .await
    .unwrap();
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0].0, actions::TENANT_REGISTERED);
    assert_eq!((rows[0].1, rows[0].2.as_str()), (Some(u), "user"));
    assert_eq!(
        (rows[0].3.as_deref(), rows[0].4.as_deref()),
        (Some("req-test-1"), Some("203.0.113.9"))
    );
    assert_eq!(rows[0].5["hostname"], "toko-a.ryasai.my.id");
    assert_eq!(rows[1].0, actions::TENANT_UPDATED);
    assert_eq!(
        rows[1].5,
        serde_json::json!({"before": {"name": "Toko A"}, "after": {"name": "Toko A Baru"}})
    );

    // Registrasi gagal (email ganda) tidak meninggalkan audit
    let before: i64 = sqlx::query_scalar("SELECT count(*) FROM audit_logs")
        .fetch_one(&pool)
        .await
        .unwrap();
    let hasher = BlockingArgon2(Arc::new(Argon2idHasher::new(1024, 1, 1).unwrap()));
    let dup = RegisterCommand {
        email: "A@x.id".into(),
        password: "rahasia-123".into(),
        business_name: "X".into(),
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
    assert!(
        register(
            &PgRegistrationStore::new(app.clone()),
            &hasher,
            &OsTokenIssuer,
            dup,
            "ryasai.my.id",
            &AuthPolicy::default(),
            &legal
        )
        .await
        .is_err()
    );
    let after: i64 = sqlx::query_scalar("SELECT count(*) FROM audit_logs")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(before, after);
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn app_role_cannot_update_delete_or_truncate_audit(pool: PgPool) {
    let app = app_pool(&pool).await;
    let (_, t) = reg(&app, "a@x.id", "Toko A").await;
    let mut tx = begin_tenant_tx(&app, t).await.unwrap();
    let e = sqlx::query("UPDATE audit_logs SET action = 'tenant.hidden'")
        .execute(&mut *tx)
        .await
        .unwrap_err();
    assert_eq!(sqlstate(&e).as_deref(), Some("42501"));
    drop(tx);
    let mut tx = begin_tenant_tx(&app, t).await.unwrap();
    let e = sqlx::query("DELETE FROM audit_logs")
        .execute(&mut *tx)
        .await
        .unwrap_err();
    assert_eq!(sqlstate(&e).as_deref(), Some("42501"));
    drop(tx);
    let e = sqlx::query("TRUNCATE audit_logs")
        .execute(&app)
        .await
        .unwrap_err();
    assert_eq!(sqlstate(&e).as_deref(), Some("42501"));
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn even_owner_cannot_update_and_delete_requires_purge_flag(pool: PgPool) {
    let app = app_pool(&pool).await;
    reg(&app, "a@x.id", "Toko A").await;
    // Koneksi uji = superuser: trigger tetap menolak
    let e = sqlx::query("UPDATE audit_logs SET action = 'tenant.hidden'")
        .execute(&pool)
        .await
        .unwrap_err();
    assert_eq!(sqlstate(&e).as_deref(), Some("42501"));
    let e = sqlx::query("DELETE FROM audit_logs")
        .execute(&pool)
        .await
        .unwrap_err();
    assert_eq!(sqlstate(&e).as_deref(), Some("42501"));
    let e = sqlx::query("TRUNCATE audit_logs")
        .execute(&pool)
        .await
        .unwrap_err();
    assert_eq!(sqlstate(&e).as_deref(), Some("42501"));
    // Hanya dengan flag purge (dipakai fungsi retensi P1-9)
    let mut tx = pool.begin().await.unwrap();
    sqlx::query("SELECT set_config('app.audit_purge', 'on', true)")
        .execute(&mut *tx)
        .await
        .unwrap();
    assert_eq!(
        sqlx::query("DELETE FROM audit_logs")
            .execute(&mut *tx)
            .await
            .unwrap()
            .rows_affected(),
        1
    );
    tx.rollback().await.unwrap();
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn tenants_read_only_their_audit_and_cannot_write_for_others(pool: PgPool) {
    let app = app_pool(&pool).await;
    let (_, a) = reg(&app, "a@x.id", "Toko A").await;
    let (_, b) = reg(&app, "b@x.id", "Toko B").await;
    let count = |t: TenantId| {
        let app = app.clone();
        async move {
            let mut tx = begin_tenant_tx(&app, t).await.unwrap();
            let rows: Vec<Option<Uuid>> = sqlx::query_scalar("SELECT tenant_id FROM audit_logs")
                .fetch_all(&mut *tx)
                .await
                .unwrap();
            tx.commit().await.unwrap();
            rows
        }
    };
    assert_eq!(count(a).await, vec![Some(*a.as_uuid())]);
    assert_eq!(count(b).await, vec![Some(*b.as_uuid())]);
    // Tanpa context: tidak terbaca sama sekali
    let none: i64 = sqlx::query_scalar("SELECT count(*) FROM audit_logs")
        .fetch_one(&app)
        .await
        .unwrap();
    assert_eq!(none, 0);

    // Menulis atas nama tenant B dari context A -> ditolak
    let mut tx = begin_tenant_tx(&app, a).await.unwrap();
    let forged = AuditEvent::new(actions::TENANT_UPDATED, "tenant").in_tenant(b);
    let e = audit_repo::record(&mut *tx, &forged).await.unwrap_err();
    assert_eq!(sqlstate(&e).as_deref(), Some("42501"));
    drop(tx);
    // Event platform (tanpa tenant, mis. login) boleh ditulis tanpa context
    audit_repo::record(&app, &AuditEvent::new(actions::AUTH_LOGIN_FAILED, "user"))
        .await
        .unwrap();
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn sensitive_values_never_reach_storage(pool: PgPool) {
    let app = app_pool(&pool).await;
    let e = AuditEvent::new(actions::TENANT_UPDATED, "tenant").with_summary(serde_json::json!({
        "after": {"name": "X", "api_key": "sk-live-123", "password": "rahasia-123"},
        "provider": {"authorization": "Bearer abc"}
    }));
    audit_repo::record(&app, &e).await.unwrap();
    let stored: String = sqlx::query_scalar("SELECT summary::text FROM audit_logs")
        .fetch_one(&pool)
        .await
        .unwrap();
    for leaked in ["sk-live-123", "rahasia-123", "Bearer abc"] {
        assert!(!stored.contains(leaked), "{leaked} tersimpan: {stored}");
    }
    assert!(stored.contains(REDACTED));
    // Seluruh tabel: password registrasi tidak pernah muncul
    reg(&app, "a@x.id", "Toko A").await;
    let any: i64 =
        sqlx::query_scalar("SELECT count(*) FROM audit_logs WHERE summary::text LIKE '%rahasia%'")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(any, 0);
}
