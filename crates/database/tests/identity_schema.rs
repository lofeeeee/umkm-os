//! P1-1: constraint skema identity (migrations/0002_identity) di Postgres nyata.
//! Setiap aturan diuji dengan kasus ditolak (SQLSTATE dicek) dan kasus diterima.

use sqlx::PgPool;
use umkm_database::MIGRATOR;
use uuid::Uuid;

const HASH: &str = "$argon2id$v=19$m=19456,t=2,p=1$c2FsdA$aGFzaA";

const UNIQUE: &str = "23505";
const CHECK: &str = "23514";
const FK: &str = "23503";

async fn exec(pool: &PgPool, sql: &str) -> Result<(), String> {
    // Fixture tes: SQL ditulis tangan di file ini, bukan input pengguna.
    sqlx::raw_sql(sqlx::AssertSqlSafe(sql.to_owned()))
        .execute(pool)
        .await
        .map(|_| ())
        .map_err(|e| {
            e.as_database_error()
                .and_then(|d| d.code().map(|c| c.into_owned()))
                .unwrap_or_else(|| e.to_string())
        })
}

async fn ok(pool: &PgPool, sql: &str) {
    exec(pool, sql)
        .await
        .unwrap_or_else(|code| panic!("harus diterima, dapat {code}: {sql}"));
}

async fn rejected(pool: &PgPool, sql: &str, code: &str) {
    match exec(pool, sql).await {
        Ok(()) => panic!("harus ditolak ({code}): {sql}"),
        Err(got) => assert_eq!(got, code, "SQLSTATE salah untuk: {sql}"),
    }
}

async fn user(pool: &PgPool, email: &str) -> Uuid {
    sqlx::query_scalar("INSERT INTO users (email, password_hash) VALUES ($1, $2) RETURNING id")
        .bind(email)
        .bind(HASH)
        .fetch_one(pool)
        .await
        .unwrap()
}

async fn tenant(pool: &PgPool, slug: &str) -> Uuid {
    sqlx::query_scalar("INSERT INTO tenants (name, slug) VALUES ('Toko', $1) RETURNING id")
        .bind(slug)
        .fetch_one(pool)
        .await
        .unwrap()
}

async fn role(pool: &PgPool, tenant: Option<Uuid>, name: &str) -> Uuid {
    sqlx::query_scalar(
        "INSERT INTO roles (tenant_id, name, is_system) VALUES ($1, $2, $1 IS NULL) RETURNING id",
    )
    .bind(tenant)
    .bind(name)
    .fetch_one(pool)
    .await
    .unwrap()
}

/// Preset role sistem dari migration 0003.
async fn preset(pool: &PgPool, name: &str) -> Uuid {
    sqlx::query_scalar("SELECT id FROM roles WHERE tenant_id IS NULL AND is_system AND name = $1")
        .bind(name)
        .fetch_one(pool)
        .await
        .unwrap()
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn seeds_37_permissions_and_reserved_subdomains(pool: PgPool) {
    let n: i64 = sqlx::query_scalar("SELECT count(*) FROM permissions")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(
        n, 37,
        "PRD.md §3.3 + Gate D (product-decision-log 2026-11-12)"
    );
    for label in ["app", "api", "www", "admin"] {
        let r: bool = sqlx::query_scalar(
            "SELECT EXISTS (SELECT 1 FROM reserved_subdomains WHERE label = $1)",
        )
        .bind(label)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert!(r, "{label} harus cadangan");
    }
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn users_email_unique_case_insensitive_and_hash_must_be_argon2id(pool: PgPool) {
    user(&pool, "Budi@Example.com").await;
    rejected(
        &pool,
        &format!("INSERT INTO users (email, password_hash) VALUES ('budi@example.COM', '{HASH}')"),
        UNIQUE,
    )
    .await;
    rejected(
        &pool,
        "INSERT INTO users (email, password_hash) VALUES ('x@y.id', 'plaintext-password')",
        CHECK,
    )
    .await;
    rejected(
        &pool,
        "INSERT INTO users (email, password_hash) VALUES ('x@y.id', '$2b$bcrypt')",
        CHECK,
    )
    .await;
    rejected(
        &pool,
        &format!("INSERT INTO users (email, password_hash) VALUES ('no-at-sign', '{HASH}')"),
        CHECK,
    )
    .await;
    rejected(
        &pool,
        &format!("INSERT INTO users (email, password_hash) VALUES (' pad@x.id', '{HASH}')"),
        CHECK,
    )
    .await;
    rejected(&pool, &format!("INSERT INTO users (email, password_hash, platform_role) VALUES ('p@x.id', '{HASH}', 'owner')"), CHECK).await;
    ok(&pool, &format!("INSERT INTO users (email, password_hash, platform_role) VALUES ('sa@x.id', '{HASH}', 'super_admin')")).await;
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn tenant_slug_format_unique_and_not_reserved(pool: PgPool) {
    tenant(&pool, "toko-budi").await;
    rejected(
        &pool,
        "INSERT INTO tenants (name, slug) VALUES ('X', 'toko-budi')",
        UNIQUE,
    )
    .await;
    for bad in [
        "Toko",
        "-toko",
        "toko-",
        "ab",
        "toko_budi",
        "toko.budi",
        &"a".repeat(64),
    ] {
        rejected(
            &pool,
            &format!("INSERT INTO tenants (name, slug) VALUES ('X', '{bad}')"),
            CHECK,
        )
        .await;
    }
    for reserved in ["app", "api", "www", "admin"] {
        rejected(
            &pool,
            &format!("INSERT INTO tenants (name, slug) VALUES ('X', '{reserved}')"),
            CHECK,
        )
        .await;
    }
    // Rename ke slug cadangan juga ditolak
    rejected(
        &pool,
        "UPDATE tenants SET slug = 'api' WHERE slug = 'toko-budi'",
        CHECK,
    )
    .await;
    ok(
        &pool,
        &format!(
            "INSERT INTO tenants (name, slug) VALUES ('X', '{}')",
            "a".repeat(63)
        ),
    )
    .await;
    ok(
        &pool,
        "INSERT INTO tenants (name, slug) VALUES ('X', 'abc')",
    )
    .await;
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn tenant_closed_status_requires_closed_at(pool: PgPool) {
    tenant(&pool, "toko-a").await;
    rejected(
        &pool,
        "UPDATE tenants SET status = 'closed' WHERE slug = 'toko-a'",
        CHECK,
    )
    .await;
    ok(
        &pool,
        "UPDATE tenants SET status = 'closed', closed_at = now() WHERE slug = 'toko-a'",
    )
    .await;
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn membership_unique_per_user_tenant_and_role_must_match_tenant(pool: PgPool) {
    let (u, a, b) = (
        user(&pool, "u@x.id").await,
        tenant(&pool, "toko-a").await,
        tenant(&pool, "toko-b").await,
    );
    let preset = preset(&pool, "Owner").await;
    let role_b = role(&pool, Some(b), "Kasir").await;

    ok(&pool, &format!("INSERT INTO tenant_memberships (tenant_id, user_id, role_id) VALUES ('{a}', '{u}', '{preset}')")).await;
    // AUTH-008: user boleh di beberapa tenant
    ok(&pool, &format!("INSERT INTO tenant_memberships (tenant_id, user_id, role_id) VALUES ('{b}', '{u}', '{role_b}')")).await;
    rejected(&pool, &format!("INSERT INTO tenant_memberships (tenant_id, user_id, role_id) VALUES ('{a}', '{u}', '{preset}')"), UNIQUE).await;

    // Role kustom tenant B tidak boleh dipakai di tenant A (batas tenant)
    let v = user(&pool, "v@x.id").await;
    rejected(&pool, &format!("INSERT INTO tenant_memberships (tenant_id, user_id, role_id) VALUES ('{a}', '{v}', '{role_b}')"), FK).await;
    rejected(
        &pool,
        &format!("UPDATE tenant_memberships SET role_id = '{role_b}' WHERE tenant_id = '{a}'"),
        FK,
    )
    .await;
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn roles_system_iff_global_and_unique_name_per_scope(pool: PgPool) {
    let a = tenant(&pool, "toko-a").await;
    // 'Owner' sudah ada sebagai preset (0003); nama sama (case-insensitive) ditolak.
    rejected(
        &pool,
        "INSERT INTO roles (tenant_id, name, is_system) VALUES (NULL, 'owner', true)",
        UNIQUE,
    )
    .await;
    rejected(
        &pool,
        "INSERT INTO roles (tenant_id, name, is_system) VALUES (NULL, 'Ghost', false)",
        CHECK,
    )
    .await;
    rejected(
        &pool,
        &format!("INSERT INTO roles (tenant_id, name, is_system) VALUES ('{a}', 'X', true)"),
        CHECK,
    )
    .await;
    ok(
        &pool,
        &format!("INSERT INTO roles (tenant_id, name) VALUES ('{a}', 'Owner')"),
    )
    .await;
    rejected(
        &pool,
        &format!("INSERT INTO roles (tenant_id, name) VALUES ('{a}', 'OWNER')"),
        UNIQUE,
    )
    .await;
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn domain_hostname_unique_lowercase_and_custom_needs_verification(pool: PgPool) {
    let (a, b) = (tenant(&pool, "toko-a").await, tenant(&pool, "toko-b").await);
    ok(&pool, &format!("INSERT INTO tenant_domains (tenant_id, hostname, type, status) VALUES ('{a}', 'toko-a.ryasai.my.id', 'subdomain', 'active')")).await;
    rejected(&pool, &format!("INSERT INTO tenant_domains (tenant_id, hostname, type) VALUES ('{b}', 'toko-a.ryasai.my.id', 'subdomain')"), UNIQUE).await;
    for bad in [
        "Toko-A.ryasai.my.id",
        "toko a.ryasai.my.id",
        "-x.ryasai.my.id",
        "localhost",
        "x..id",
    ] {
        rejected(&pool, &format!("INSERT INTO tenant_domains (tenant_id, hostname, type) VALUES ('{b}', '{bad}', 'subdomain')"), CHECK).await;
    }
    rejected(&pool, &format!("INSERT INTO tenant_domains (tenant_id, hostname, type, status) VALUES ('{b}', 'tokob.com', 'custom', 'active')"), CHECK).await;
    ok(&pool, &format!("INSERT INTO tenant_domains (tenant_id, hostname, type, status, verified_at) VALUES ('{b}', 'tokob.com', 'custom', 'active', now())")).await;
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn invitations_hashed_expiring_single_use_and_tenant_scoped_role(pool: PgPool) {
    let (a, b) = (tenant(&pool, "toko-a").await, tenant(&pool, "toko-b").await);
    let preset = preset(&pool, "Viewer").await;
    let role_b = role(&pool, Some(b), "Kasir").await;
    let h1 = "'\\x".to_owned() + &"11".repeat(32) + "'";
    let h2 = "'\\x".to_owned() + &"22".repeat(32) + "'";

    let insert = |email: &str, role: Uuid, hash: &str, exp: &str| {
        format!(
            "INSERT INTO invitations (tenant_id, email, role_id, token_hash, expires_at) VALUES ('{a}', '{email}', '{role}', {hash}, {exp})"
        )
    };
    ok(
        &pool,
        &insert("x@y.id", preset, &h1, "now() + interval '7 days'"),
    )
    .await;
    // Satu undangan tertunda per email per tenant (case-insensitive)
    rejected(
        &pool,
        &insert("X@Y.id", preset, &h2, "now() + interval '7 days'"),
        UNIQUE,
    )
    .await;
    // Hash token unik global
    rejected(
        &pool,
        &insert("z@y.id", preset, &h1, "now() + interval '7 days'"),
        UNIQUE,
    )
    .await;
    // Bukan hash 32 byte (mis. token plaintext)
    rejected(
        &pool,
        &insert(
            "z@y.id",
            preset,
            "'plain-token'::bytea",
            "now() + interval '7 days'",
        ),
        CHECK,
    )
    .await;
    // Expiry di masa lalu
    rejected(
        &pool,
        &insert("z@y.id", preset, &h2, "now() - interval '1 second'"),
        CHECK,
    )
    .await;
    // Role tenant lain
    rejected(
        &pool,
        &insert("z@y.id", role_b, &h2, "now() + interval '7 days'"),
        FK,
    )
    .await;
    // Sekali pakai: diterima DAN dicabut tidak mungkin
    rejected(
        &pool,
        "UPDATE invitations SET accepted_at = now(), revoked_at = now()",
        CHECK,
    )
    .await;
    ok(&pool, "UPDATE invitations SET accepted_at = now()").await;
    // Setelah diterima, undangan baru untuk email yang sama boleh
    ok(
        &pool,
        &insert("x@y.id", preset, &h2, "now() + interval '7 days'"),
    )
    .await;
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn sessions_tokens_consents_mfa_constraints(pool: PgPool) {
    let u = user(&pool, "u@x.id").await;
    let h = "'\\x".to_owned() + &"aa".repeat(32) + "'";
    ok(&pool, &format!("INSERT INTO user_sessions (user_id, token_hash, expires_at) VALUES ('{u}', {h}, now() + interval '30 days')")).await;
    rejected(&pool, &format!("INSERT INTO user_sessions (user_id, token_hash, expires_at) VALUES ('{u}', {h}, now() + interval '30 days')"), UNIQUE).await;
    rejected(&pool, &format!("INSERT INTO user_sessions (user_id, token_hash, expires_at) VALUES ('{u}', 'short'::bytea, now() + interval '1 day')"), CHECK).await;

    rejected(&pool, &format!("INSERT INTO user_tokens (user_id, purpose, token_hash, expires_at) VALUES ('{u}', 'magic_link', {h}, now() + interval '1 hour')"), CHECK).await;
    ok(&pool, &format!("INSERT INTO user_tokens (user_id, purpose, token_hash, expires_at) VALUES ('{u}', 'password_reset', {h}, now() + interval '1 hour')")).await;

    ok(&pool, &format!("INSERT INTO user_consents (user_id, document, version) VALUES ('{u}', 'terms_of_service', 'placeholder-v0')")).await;
    rejected(&pool, &format!("INSERT INTO user_consents (user_id, document, version) VALUES ('{u}', 'terms_of_service', 'placeholder-v0')"), UNIQUE).await;
    rejected(
        &pool,
        &format!(
            "INSERT INTO user_consents (user_id, document, version) VALUES ('{u}', 'cookies', 'v1')"
        ),
        CHECK,
    )
    .await;

    rejected(&pool, &format!("INSERT INTO mfa_credentials (user_id, encrypted_secret) VALUES ('{u}', 'JBSWY3DP'::bytea)"), CHECK).await;
    ok(
        &pool,
        &format!("INSERT INTO mfa_credentials (user_id, encrypted_secret) VALUES ('{u}', {h})"),
    )
    .await;
    rejected(
        &pool,
        &format!("INSERT INTO mfa_credentials (user_id, encrypted_secret) VALUES ('{u}', {h})"),
        UNIQUE,
    )
    .await;
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn updated_at_trigger_advances(pool: PgPool) {
    let a = tenant(&pool, "toko-a").await;
    let before = ts(&pool, a).await;
    sqlx::query("SELECT pg_sleep(0.01)")
        .execute(&pool)
        .await
        .unwrap();
    ok(
        &pool,
        &format!("UPDATE tenants SET name = 'Baru' WHERE id = '{a}'"),
    )
    .await;
    assert!(ts(&pool, a).await > before);
}

/// `updated_at` dalam epoch mikrodetik.
async fn ts(pool: &PgPool, id: Uuid) -> i64 {
    sqlx::query_scalar(
        "SELECT (extract(epoch FROM updated_at) * 1000000)::bigint FROM tenants WHERE id = $1",
    )
    .bind(id)
    .fetch_one(pool)
    .await
    .unwrap()
}

#[sqlx::test(migrations = false)]
async fn identity_migration_reversible(pool: PgPool) {
    MIGRATOR.run(&pool).await.unwrap();
    MIGRATOR.undo(&pool, 1).await.unwrap();
    let n: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM pg_tables WHERE schemaname = 'public' AND tablename <> '_sqlx_migrations'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(n, 0, "down 0002 harus menghapus semua tabel identity");
    let fns: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM pg_proc p JOIN pg_namespace n ON n.oid = p.pronamespace
         WHERE n.nspname = 'app' AND p.proname <> 'current_tenant_id'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(fns, 0, "fungsi helper 0002 harus terhapus");
    MIGRATOR.run(&pool).await.unwrap();
}
