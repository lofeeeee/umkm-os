//! P5-1: skema AI — rujukan komposit sesama tenant, batas parameter (AC-AIC-001) dicerminkan DB,
//! aturan URL platform/BYOK, rahasia hanya ciphertext envelope, metering append-only.
//! Dijalankan sebagai role aplikasi (RLS aktif).

use sqlx::PgPool;
use umkm_database::{MIGRATOR, begin_tenant_tx};
use umkm_infrastructure::crypto::{Keyring, Sealed, open, seal};
use umkm_shared::ids::TenantId;
use uuid::Uuid;

async fn app_pool(pool: &PgPool) -> PgPool {
    sqlx::postgres::PgPoolOptions::new()
        .max_connections(4)
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

async fn tenant(pool: &PgPool, slug: &str) -> TenantId {
    TenantId::from_uuid(
        sqlx::query_scalar("INSERT INTO tenants (name, slug) VALUES ('T', $1) RETURNING id")
            .bind(slug)
            .fetch_one(pool)
            .await
            .unwrap(),
    )
}

/// Jalankan satu statement dalam konteks tenant; kembalikan pesan error (atau None bila sukses).
async fn exec(app: &PgPool, t: TenantId, sql: &'static str, binds: &[Uuid]) -> Option<String> {
    let mut tx = begin_tenant_tx(app, t).await.unwrap();
    let mut q = sqlx::query(sql).bind(t.as_uuid());
    for b in binds {
        q = q.bind(*b);
    }
    match q.execute(&mut *tx).await {
        Ok(_) => {
            tx.commit().await.unwrap();
            None
        }
        Err(e) => Some(e.to_string()),
    }
}

async fn byok(app: &PgPool, t: TenantId, name: &str) -> Uuid {
    let mut tx = begin_tenant_tx(app, t).await.unwrap();
    let id: Uuid = sqlx::query_scalar(
        "INSERT INTO ai_providers (tenant_id, name, source, provider_type, base_url, model)
         VALUES ($1, $2, 'byok', 'openai', 'https://api.example.com/v1', 'gpt-4o-mini') RETURNING id",
    )
    .bind(t.as_uuid())
    .bind(name)
    .fetch_one(&mut *tx)
    .await
    .unwrap();
    tx.commit().await.unwrap();
    id
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn config_cannot_reference_another_tenants_provider(pool: PgPool) {
    let app = app_pool(&pool).await;
    let (a, b) = (tenant(&pool, "toko-a").await, tenant(&pool, "toko-b").await);
    let pa = byok(&app, a, "Utama").await;
    let pb = byok(&app, b, "Milik B").await;
    let ins = "INSERT INTO chatbot_configs (tenant_id, primary_provider_id) VALUES ($1, $2)";
    let err = exec(&app, a, ins, &[pb])
        .await
        .expect("provider tenant B ditolak");
    assert!(err.contains("fk_chatbot_primary"), "{err}");
    assert_eq!(exec(&app, a, ins, &[pa]).await, None);
    // Rahasia & jejak run juga terikat komposit.
    let err = exec(
        &app,
        a,
        "INSERT INTO ai_provider_secrets (tenant_id, provider_id, kek_version, wrapped_dek, ciphertext)
         VALUES ($1, $2, 1, decode(repeat('00', 72), 'hex'), decode(repeat('00', 50), 'hex'))",
        &[pb],
    )
    .await
    .expect("rahasia untuk provider tenant lain ditolak");
    assert!(err.contains("fk_ai_provider_secrets_provider"), "{err}");
    let err = exec(
        &app,
        a,
        "INSERT INTO ai_runs (tenant_id, provider_id, source, model, status) VALUES ($1, $2, 'byok', 'm', 'ok')",
        &[pb],
    )
    .await
    .expect("run untuk provider tenant lain ditolak");
    assert!(err.contains("fk_ai_runs_provider"), "{err}");
    // RLS: tenant B tidak melihat provider A.
    let mut tx = begin_tenant_tx(&app, b).await.unwrap();
    let n: i64 = sqlx::query_scalar("SELECT count(*) FROM ai_providers WHERE id = $1")
        .bind(pa)
        .fetch_one(&mut *tx)
        .await
        .unwrap();
    assert_eq!(n, 0);
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn parameter_bounds_and_url_rules_mirror_domain(pool: PgPool) {
    let app = app_pool(&pool).await;
    let a = tenant(&pool, "toko-a").await;
    let bad = [
        (
            "INSERT INTO chatbot_configs (tenant_id, temperature) VALUES ($1, 2.01)",
            "ck_chatbot_temperature",
        ),
        (
            "INSERT INTO chatbot_configs (tenant_id, max_output_tokens) VALUES ($1, 4097)",
            "ck_chatbot_max_tokens",
        ),
        (
            "INSERT INTO chatbot_configs (tenant_id, timeout_ms) VALUES ($1, 999)",
            "ck_chatbot_timeout",
        ),
        (
            "INSERT INTO chatbot_configs (tenant_id, mode) VALUES ($1, 'yolo')",
            "ck_chatbot_mode",
        ),
        (
            "INSERT INTO chatbot_configs (tenant_id, enabled, mode) VALUES ($1, true, 'hybrid')",
            "ck_chatbot_enabled_provider",
        ),
        (
            "INSERT INTO chatbot_configs (tenant_id, fallback_message) VALUES ($1, '  ')",
            "ck_chatbot_fallback_msg",
        ),
        (
            "INSERT INTO chatbot_tools (tenant_id, tool_name, enabled) VALUES ($1, 'run_sql', true)",
            "ck_chatbot_tools_name",
        ),
        (
            "INSERT INTO ai_providers (tenant_id, name, source, provider_type, base_url, model) VALUES ($1, 'p', 'byok', 'custom', 'http://10.0.0.1/v1', 'm')",
            "ck_ai_providers_url",
        ),
        (
            "INSERT INTO ai_providers (tenant_id, name, source, provider_type, base_url, model) VALUES ($1, 'p', 'byok', 'custom', 'https://user:pw@evil.example/v1', 'm')",
            "ck_ai_providers_url",
        ),
        (
            "INSERT INTO ai_providers (tenant_id, name, source, provider_type, base_url, model) VALUES ($1, 'p', 'platform', 'platform', 'https://x.example', 'm')",
            "ck_ai_providers_url",
        ),
        (
            "INSERT INTO ai_providers (tenant_id, name, source, provider_type, model) VALUES ($1, 'p', 'byok', 'platform', 'm')",
            "ck_ai_providers_platform",
        ),
        (
            "INSERT INTO ai_providers (tenant_id, name, source, provider_type, model) VALUES ($1, 'p', 'platform', 'platform', 'model with space')",
            "ck_ai_providers_model",
        ),
    ];
    for (sql, constraint) in bad {
        let err = exec(&app, a, sql, &[])
            .await
            .unwrap_or_else(|| panic!("harus ditolak: {sql}"));
        assert!(err.contains(constraint), "{constraint}: {err}");
    }
    // Batas tepat diterima; Human Only aktif tanpa provider diterima.
    assert_eq!(
        exec(&app, a, "INSERT INTO chatbot_configs (tenant_id, enabled, mode, temperature, max_output_tokens, timeout_ms) VALUES ($1, true, 'human_only', 2, 4096, 60000)", &[]).await,
        None
    );
    assert_eq!(
        exec(&app, a, "INSERT INTO ai_providers (tenant_id, name, source, provider_type, model) VALUES ($1, 'Platform', 'platform', 'platform', 'default')", &[]).await,
        None
    );
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn secret_is_envelope_ciphertext_and_metering_is_append_only(pool: PgPool) {
    let app = app_pool(&pool).await;
    let a = tenant(&pool, "toko-a").await;
    let p = byok(&app, a, "Utama").await;
    let k = Keyring::parse("v1:a2VrLXJhaGFzaWEta2VrLXJhaGFzaWEta2VrLXJhaGE=").unwrap();
    let aad = format!("ai_provider|{}|{p}", a.as_uuid());
    let s = seal(&k, b"sk-live-RAHASIA-1234567890", aad.as_bytes()).unwrap();
    let mut tx = begin_tenant_tx(&app, a).await.unwrap();
    sqlx::query("INSERT INTO ai_provider_secrets (tenant_id, provider_id, kek_version, wrapped_dek, ciphertext) VALUES ($1, $2, $3, $4, $5)")
        .bind(a.as_uuid())
        .bind(p)
        .bind(s.kek_version)
        .bind(&s.wrapped_dek)
        .bind(&s.ciphertext)
        .execute(&mut *tx)
        .await
        .unwrap();
    let (v, dek, ct): (i16, Vec<u8>, Vec<u8>) = sqlx::query_as(
        "SELECT kek_version, wrapped_dek, ciphertext FROM ai_provider_secrets WHERE provider_id = $1",
    )
    .bind(p)
    .fetch_one(&mut *tx)
    .await
    .unwrap();
    // Tidak ada plaintext tersimpan di mana pun pada baris.
    let dump: String = sqlx::query_scalar(
        "SELECT row_to_json(s)::text FROM ai_provider_secrets s WHERE provider_id = $1",
    )
    .bind(p)
    .fetch_one(&mut *tx)
    .await
    .unwrap();
    assert!(!dump.contains("RAHASIA") && !dump.contains("sk-live"));
    let back = Sealed {
        kek_version: v,
        wrapped_dek: dek,
        ciphertext: ct,
    };
    assert_eq!(
        open(&k, &back, aad.as_bytes()).unwrap().expose(),
        b"sk-live-RAHASIA-1234567890"
    );
    // Rahasia dipindah ke provider lain tidak dapat dibuka (AAD).
    assert!(
        open(
            &k,
            &back,
            format!("ai_provider|{}|{}", a.as_uuid(), Uuid::nil()).as_bytes()
        )
        .is_err()
    );

    let run: Uuid = sqlx::query_scalar(
        "INSERT INTO ai_runs (tenant_id, provider_id, source, model, status, prompt_tokens) VALUES ($1, $2, 'byok', 'm', 'ok', 10) RETURNING id",
    )
    .bind(a.as_uuid())
    .bind(p)
    .fetch_one(&mut *tx)
    .await
    .unwrap();
    tx.commit().await.unwrap();
    for sql in [
        "UPDATE ai_runs SET prompt_tokens = 0 WHERE id = $2",
        "DELETE FROM ai_runs WHERE id = $2",
    ] {
        let err = exec(&app, a, sql, &[run])
            .await
            .expect("metering append-only");
        assert!(err.contains("permission denied"), "{err}");
    }
    let err = exec(
        &app,
        a,
        "INSERT INTO ai_tool_calls (tenant_id, ai_run_id, tool_name, result_status) VALUES ($1, $2, 'drop_tables', 'ok')",
        &[run],
    )
    .await
    .expect("nama tool bebas ditolak");
    assert!(err.contains("ck_ai_tool_calls_name"), "{err}");
}
