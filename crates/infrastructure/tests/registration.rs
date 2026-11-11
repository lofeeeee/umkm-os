//! P1-3 acceptance `PRD.md §13.3` Registration di Postgres nyata:
//! register membuat user + tenant, Owner membership, subdomain unik, slug cadangan ditolak.

use std::sync::Arc;

use sqlx::PgPool;
use umkm_application::onboarding::{LegalVersions, RegisterCommand, RegisterError, register};
use umkm_database::MIGRATOR;
use umkm_domain::{auth::AuthPolicy, tenant::TenantError};
use umkm_infrastructure::{
    auth::{Argon2idHasher, hash_token},
    auth_repo,
    onboarding_repo::{BlockingArgon2, OsTokenIssuer, PgRegistrationStore},
};

const ROOT: &str = "ryasai.my.id";

fn legal() -> LegalVersions {
    LegalVersions {
        privacy_policy: "placeholder-v0".into(),
        terms_of_service: "placeholder-v0".into(),
    }
}

fn cmd(email: &str, name: &str, slug: Option<&str>) -> RegisterCommand {
    RegisterCommand {
        email: email.into(),
        password: "rahasia-123".into(),
        business_name: name.into(),
        slug: slug.map(Into::into),
        accept_terms: true,
        accept_privacy: true,
        user_agent: Some("tes".into()),
        request_id: None,
        client_ip: None,
    }
}

async fn reg(
    pool: &PgPool,
    c: RegisterCommand,
) -> Result<umkm_application::onboarding::RegistrationOutcome, RegisterError> {
    let hasher = BlockingArgon2(Arc::new(Argon2idHasher::new(1024, 1, 1).unwrap()));
    register(
        &PgRegistrationStore::new(pool.clone()),
        &hasher,
        &OsTokenIssuer,
        c,
        ROOT,
        &AuthPolicy::default(),
        &legal(),
    )
    .await
}

async fn count(pool: &PgPool, table: &str) -> i64 {
    sqlx::query_scalar(sqlx::AssertSqlSafe(format!("SELECT count(*) FROM {table}")))
        .fetch_one(pool)
        .await
        .unwrap()
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn register_creates_user_tenant_owner_subdomain_profile_consents_session(pool: PgPool) {
    let out = reg(&pool, cmd("Budi@X.id", "Toko Budi", None))
        .await
        .unwrap();
    let r = &out.registered;
    assert_eq!(r.slug.as_str(), "toko-budi");
    assert_eq!(r.hostname, "toko-budi.ryasai.my.id");
    let tid = *r.tenant_id.as_uuid();

    // Owner membership dengan seluruh 33 permission
    let perms: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM tenant_memberships m JOIN roles ro ON ro.id = m.role_id
         JOIN role_permissions rp ON rp.role_id = ro.id
         WHERE m.tenant_id = $1 AND m.user_id = $2 AND m.status = 'active' AND ro.name = 'Owner' AND ro.is_system",
    )
    .bind(tid)
    .bind(r.user_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(perms, 33);

    let (host, ty, status): (String, String, String) =
        sqlx::query_as("SELECT hostname, type, status FROM tenant_domains WHERE tenant_id = $1")
            .bind(tid)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(
        (host.as_str(), ty.as_str(), status.as_str()),
        ("toko-budi.ryasai.my.id", "subdomain", "active")
    );

    let profile: String =
        sqlx::query_scalar("SELECT name FROM business_profiles WHERE tenant_id = $1")
            .bind(tid)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(profile, "Toko Budi");

    let consents: Vec<(String, String)> = sqlx::query_as(
        "SELECT document, version FROM user_consents WHERE user_id = $1 ORDER BY document",
    )
    .bind(r.user_id)
    .fetch_all(&pool)
    .await
    .unwrap();
    assert_eq!(
        consents,
        vec![
            ("privacy_policy".into(), "placeholder-v0".into()),
            ("terms_of_service".into(), "placeholder-v0".into())
        ]
    );

    // Sesi login langsung valid; plaintext token hanya di outcome
    let s = auth_repo::authenticate_session(&pool, hash_token(&out.session_token))
        .await
        .unwrap()
        .unwrap();
    assert_eq!((s.user_id, s.session_id), (r.user_id, r.session_id));

    let hash: String = sqlx::query_scalar("SELECT password_hash FROM users WHERE id = $1")
        .bind(r.user_id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert!(hash.starts_with("$argon2id$") && !hash.contains("rahasia"));
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn duplicate_email_rolls_back_everything(pool: PgPool) {
    reg(&pool, cmd("budi@x.id", "Toko Budi", None))
        .await
        .unwrap();
    let before = (
        count(&pool, "tenants").await,
        count(&pool, "tenant_domains").await,
        count(&pool, "user_sessions").await,
    );
    let err = reg(&pool, cmd("BUDI@x.id", "Toko Lain", None))
        .await
        .unwrap_err();
    assert_eq!(err, RegisterError::EmailTaken);
    let after = (
        count(&pool, "tenants").await,
        count(&pool, "tenant_domains").await,
        count(&pool, "user_sessions").await,
    );
    assert_eq!(before, after, "tidak boleh ada data parsial");
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn auto_slug_collision_gets_suffix_and_hostnames_stay_unique(pool: PgPool) {
    let a = reg(&pool, cmd("a@x.id", "Toko Budi", None)).await.unwrap();
    let b = reg(&pool, cmd("b@x.id", "Toko Budi", None)).await.unwrap();
    let c = reg(&pool, cmd("c@x.id", "TOKO  budi!", None))
        .await
        .unwrap();
    assert_eq!(a.registered.slug.as_str(), "toko-budi");
    assert_eq!(b.registered.slug.as_str(), "toko-budi-2");
    assert_eq!(c.registered.slug.as_str(), "toko-budi-3");
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn reserved_slug_rejected_explicit_and_skipped_when_automatic(pool: PgPool) {
    // Eksplisit: ditolak, user tidak dibuat (TEN-006)
    let err = reg(&pool, cmd("a@x.id", "API Shop", Some("api")))
        .await
        .unwrap_err();
    assert_eq!(err, RegisterError::Tenant(TenantError::SlugUnavailable));
    assert_eq!(count(&pool, "users").await, 0, "user tidak boleh tersisa");
    // Otomatis: nama "Admin" -> 'admin' dicadangkan -> 'admin-2'
    let ok = reg(&pool, cmd("b@x.id", "Admin", None)).await.unwrap();
    assert_eq!(ok.registered.slug.as_str(), "admin-2");
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn explicit_slug_taken_is_rejected_without_partial_data(pool: PgPool) {
    reg(&pool, cmd("a@x.id", "Warung", Some("warung-siti")))
        .await
        .unwrap();
    let err = reg(&pool, cmd("b@x.id", "Warung Lain", Some("warung-siti")))
        .await
        .unwrap_err();
    assert_eq!(err, RegisterError::Tenant(TenantError::SlugUnavailable));
    assert_eq!(count(&pool, "users").await, 1);
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn concurrent_registrations_same_name_all_get_unique_slugs(pool: PgPool) {
    let mut set = tokio::task::JoinSet::new();
    for i in 0..8 {
        let pool = pool.clone();
        set.spawn(async move {
            reg(
                &pool,
                cmd(&format!("u{i}@x.id"), "Kopi Kenangan Senja", None),
            )
            .await
        });
    }
    let results = set.join_all().await;
    let mut slugs: Vec<String> = results
        .iter()
        .map(|r| r.as_ref().unwrap().registered.slug.to_string())
        .collect();
    slugs.sort();
    slugs.dedup();
    assert_eq!(slugs.len(), 8, "{slugs:?}");
    assert_eq!(count(&pool, "tenant_domains").await, 8);
}
