//! P6-1 (FR-CNV-001, AC-CNV-001): skema conversation/session/message — UUID terpisah, 4 sender
//! type, scope tenant (RLS + FK komposit), append-only, paritas lifecycle Rust ↔ trigger, token
//! sesi hanya hash, purge retensi ADR-0012. Dijalankan sebagai role aplikasi (RLS aktif).

use std::time::Duration;

use sqlx::{PgPool, Row};
use umkm_database::{MIGRATOR, begin_tenant_tx};
use umkm_domain::conversation::{ALL_STATUSES, ConversationStatus, SenderType};
use umkm_infrastructure::conversation_repo::{
    self as repo, Actor, ConversationError, NewMessage, StartInput,
};
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
    let id: Uuid =
        sqlx::query_scalar("INSERT INTO tenants (name, slug) VALUES ('T', $1) RETURNING id")
            .bind(slug)
            .fetch_one(pool)
            .await
            .unwrap();
    TenantId::from_uuid(id)
}

/// Anggota aktif (role Support Agent) di tenant `t`.
async fn member(pool: &PgPool, t: TenantId, email: &str) -> Uuid {
    let u: Uuid = sqlx::query_scalar(
        "INSERT INTO users (email, password_hash) VALUES ($1, '$argon2id$x') RETURNING id",
    )
    .bind(email)
    .fetch_one(pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO tenant_memberships (tenant_id, user_id, role_id)
         SELECT $1, $2, id FROM roles WHERE tenant_id IS NULL AND name = 'Support Agent'",
    )
    .bind(t.as_uuid())
    .bind(u)
    .execute(pool)
    .await
    .unwrap();
    u
}

fn start_input() -> StartInput<'static> {
    StartInput {
        consent_version: umkm_domain::ai::consent::VERSION,
        customer_id: None,
        initial: ConversationStatus::Ai,
    }
}

fn customer_msg(session: Uuid, body: &str) -> NewMessage<'_> {
    NewMessage {
        sender: SenderType::Customer,
        session_id: Some(session),
        sender_user_id: None,
        body,
        ai_run_id: None,
    }
}

fn code(e: &sqlx::Error) -> String {
    e.as_database_error()
        .and_then(|d| d.code())
        .map(|c| c.into_owned())
        .unwrap_or_default()
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn separate_uuids_sender_types_and_session_token(pool: PgPool) {
    let app = app_pool(&pool).await;
    let t = tenant(&pool, "toko-a").await;
    let s = repo::start(&app, t, &start_input()).await.unwrap();
    assert_ne!(s.conversation_id, s.session_id);

    // Token hanya hash di DB; token yang benar membuka sesi milik tenant ini.
    let raw: Vec<u8> = sqlx::query_scalar("SELECT token_hash FROM conversation_sessions")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(raw.len(), 32);
    assert!(!String::from_utf8_lossy(&raw).contains(&s.token));
    let r = repo::resolve_session(&app, t, &s.token)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        (r.conversation_id, r.session_id),
        (s.conversation_id, s.session_id)
    );
    assert_eq!(r.status, ConversationStatus::Ai);
    assert!(
        repo::resolve_session(&app, t, "salah")
            .await
            .unwrap()
            .is_none()
    );

    // Consent D-12 tersimpan per percakapan.
    let (v, has_at): (String, bool) =
        sqlx::query_as("SELECT consent_version, consented_at IS NOT NULL FROM conversations")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(v, umkm_domain::ai::consent::VERSION);
    assert!(has_at);

    // 4 sender type; setiap pesan UUID sendiri & urut.
    let staff = member(&pool, t, "cs@x.id").await;
    let run: Uuid = {
        let mut tx = begin_tenant_tx(&app, t).await.unwrap();
        let id = sqlx::query_scalar(
            "INSERT INTO ai_runs (tenant_id, conversation_id, source, model, status)
             VALUES ($1, $2, 'platform', 'm', 'ok') RETURNING id",
        )
        .bind(t.as_uuid())
        .bind(s.conversation_id)
        .fetch_one(&mut *tx)
        .await
        .unwrap();
        tx.commit().await.unwrap();
        id
    };
    let msgs = [
        customer_msg(s.session_id, "halo"),
        NewMessage {
            sender: SenderType::Ai,
            session_id: None,
            sender_user_id: None,
            body: "Halo kak",
            ai_run_id: Some(run),
        },
        NewMessage {
            sender: SenderType::Admin,
            session_id: None,
            sender_user_id: Some(staff),
            body: "Saya bantu",
            ai_run_id: None,
        },
        NewMessage {
            sender: SenderType::System,
            session_id: None,
            sender_user_id: None,
            body: "Staf bergabung",
            ai_run_id: None,
        },
    ];
    let mut ids = vec![s.conversation_id, s.session_id];
    for m in &msgs {
        ids.push(
            repo::append(&app, t, s.conversation_id, m)
                .await
                .unwrap()
                .id,
        );
    }
    let list = repo::messages(&app, t, s.conversation_id, 0, 50)
        .await
        .unwrap();
    assert_eq!(
        list.iter().map(|m| m.sender.as_str()).collect::<Vec<_>>(),
        ["CUSTOMER", "AI", "ADMIN", "SYSTEM"]
    );
    assert!(list.windows(2).all(|w| w[0].seq < w[1].seq));
    assert_eq!(list[1].ai_run_id, Some(run));
    ids.sort();
    ids.dedup();
    assert_eq!(
        ids.len(),
        6,
        "conversation, session, 4 message: UUID berbeda"
    );
    assert_eq!(
        repo::messages(&app, t, s.conversation_id, list[1].seq, 50)
            .await
            .unwrap()
            .len(),
        2
    );

    // AC-CNV-001: sender type di luar 4 nilai ditolak DB (bukan hanya di Rust).
    let mut tx = begin_tenant_tx(&app, t).await.unwrap();
    for bad in ["BOT", "customer", "STAFF", ""] {
        let e = sqlx::query(
            "INSERT INTO conversation_messages (tenant_id, conversation_id, sender_type, body) VALUES ($1, $2, $3, 'x')",
        )
        .bind(t.as_uuid())
        .bind(s.conversation_id)
        .bind(bad)
        .execute(&mut *tx)
        .await
        .unwrap_err();
        assert_eq!(code(&e), "23514", "{bad:?}");
        tx.rollback().await.unwrap();
        tx = begin_tenant_tx(&app, t).await.unwrap();
    }
    // ADMIN wajib user; non-AI tidak boleh membawa ai_run; isi kosong ditolak.
    for (sender, user, run_id, body) in [
        ("ADMIN", None, None, "x"),
        ("CUSTOMER", Some(staff), None, "x"),
        ("CUSTOMER", None, Some(run), "x"),
        ("CUSTOMER", None, None, "   "),
    ] {
        let e = sqlx::query(
            "INSERT INTO conversation_messages (tenant_id, conversation_id, sender_type, sender_user_id, ai_run_id, body)
             VALUES ($1, $2, $3, $4, $5, $6)",
        )
        .bind(t.as_uuid())
        .bind(s.conversation_id)
        .bind(sender)
        .bind(user)
        .bind(run_id)
        .bind(body)
        .execute(&mut *tx)
        .await
        .unwrap_err();
        assert_eq!(code(&e), "23514", "{sender} {user:?} {run_id:?} {body:?}");
        tx.rollback().await.unwrap();
        tx = begin_tenant_tx(&app, t).await.unwrap();
    }
    tx.rollback().await.unwrap();
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn tenant_scope_and_append_only(pool: PgPool) {
    let app = app_pool(&pool).await;
    let a = tenant(&pool, "toko-a").await;
    let b = tenant(&pool, "toko-b").await;
    let sa = repo::start(&app, a, &start_input()).await.unwrap();
    repo::append(
        &app,
        a,
        sa.conversation_id,
        &customer_msg(sa.session_id, "rahasia A"),
    )
    .await
    .unwrap();

    // Tenant B tidak melihat / tidak dapat menulis ke percakapan A.
    for table in [
        "conversations",
        "conversation_sessions",
        "conversation_messages",
        "conversation_events",
    ] {
        let mut tx = begin_tenant_tx(&app, b).await.unwrap();
        let n: i64 =
            sqlx::query_scalar(sqlx::AssertSqlSafe(format!("SELECT count(*) FROM {table}")))
                .fetch_one(&mut *tx)
                .await
                .unwrap();
        assert_eq!(n, 0, "{table} bocor ke tenant B");
        tx.rollback().await.unwrap();
    }
    assert!(matches!(
        repo::append(
            &app,
            b,
            sa.conversation_id,
            &customer_msg(sa.session_id, "x")
        )
        .await,
        Err(ConversationError::NotFound)
    ));
    assert!(
        repo::resolve_session(&app, b, &sa.token)
            .await
            .unwrap()
            .is_none(),
        "token A tidak berlaku di B"
    );
    assert!(
        repo::messages(&app, b, sa.conversation_id, 0, 50)
            .await
            .unwrap()
            .is_empty()
    );
    // FK komposit: baris tenant B tidak bisa merujuk conversation A walau id diketahui.
    let mut tx = begin_tenant_tx(&app, b).await.unwrap();
    let e = sqlx::query(
        "INSERT INTO conversation_messages (tenant_id, conversation_id, sender_type, body) VALUES ($1, $2, 'CUSTOMER', 'x')",
    )
    .bind(b.as_uuid())
    .bind(sa.conversation_id)
    .execute(&mut *tx)
    .await
    .unwrap_err();
    assert_eq!(
        code(&e),
        "23514",
        "trigger: conversation tidak ada di tenant B"
    );
    tx.rollback().await.unwrap();
    // Menyisipkan dengan tenant_id A dari konteks B ditolak RLS.
    let mut tx = begin_tenant_tx(&app, b).await.unwrap();
    let e = sqlx::query(
        "INSERT INTO conversations (tenant_id, consent_version, consented_at) VALUES ($1, 'v0', now())",
    )
    .bind(a.as_uuid())
    .execute(&mut *tx)
    .await
    .unwrap_err();
    assert_eq!(code(&e), "42501");
    tx.rollback().await.unwrap();
    // Assignee harus anggota aktif tenant yang sama.
    let staff_b = member(&pool, b, "cs-b@x.id").await;
    assert!(matches!(
        repo::transition(
            &app,
            a,
            sa.conversation_id,
            ConversationStatus::Human,
            Actor::Staff(staff_b)
        )
        .await,
        Err(ConversationError::InvalidReference)
    ));

    // Append-only untuk role aplikasi.
    for sql in [
        "UPDATE conversation_messages SET body = 'diubah'",
        "DELETE FROM conversation_messages",
        "UPDATE conversation_events SET kind = 'closed'",
        "DELETE FROM conversation_events",
        "DELETE FROM conversations",
        "DELETE FROM conversation_sessions",
    ] {
        let mut tx = begin_tenant_tx(&app, a).await.unwrap();
        let e = sqlx::query(sql).execute(&mut *tx).await.unwrap_err();
        assert_eq!(code(&e), "42501", "{sql}");
        tx.rollback().await.unwrap();
    }
    let body: String = sqlx::query_scalar("SELECT body FROM conversation_messages")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(body, "rahasia A");
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn lifecycle_parity_takeover_and_close(pool: PgPool) {
    // Paritas: 4 × 4 transisi Rust == fungsi DB.
    let mut checked = 0;
    for from in ALL_STATUSES {
        for to in ALL_STATUSES {
            let db: bool = sqlx::query_scalar("SELECT app.conversation_transition_allowed($1, $2)")
                .bind(from.as_str())
                .bind(to.as_str())
                .fetch_one(&pool)
                .await
                .unwrap();
            assert_eq!(db, from.can_transition(to), "{from:?} -> {to:?}");
            checked += 1;
        }
    }
    assert_eq!(checked, 16);

    let app = app_pool(&pool).await;
    let t = tenant(&pool, "toko-a").await;
    let staff = member(&pool, t, "cs@x.id").await;
    // Status awal hanya AI / WAITING_HUMAN.
    for bad in [ConversationStatus::Human, ConversationStatus::Closed] {
        let r = repo::start(
            &app,
            t,
            &StartInput {
                initial: bad,
                ..start_input()
            },
        )
        .await;
        assert!(
            matches!(r, Err(ConversationError::InvalidTransition)),
            "{bad:?}"
        );
    }
    let mut tx = begin_tenant_tx(&app, t).await.unwrap();
    let e = sqlx::query(
        "INSERT INTO conversations (tenant_id, status, consent_version, consented_at, closed_at)
         VALUES ($1, 'CLOSED', 'v0', now(), now())",
    )
    .bind(t.as_uuid())
    .execute(&mut *tx)
    .await
    .unwrap_err();
    assert_eq!(code(&e), "23514", "DB juga menolak status awal CLOSED");
    tx.rollback().await.unwrap();

    let s = repo::start(&app, t, &start_input()).await.unwrap();
    // HUMAN hanya oleh staf (menjadi assignee).
    assert!(matches!(
        repo::transition(
            &app,
            t,
            s.conversation_id,
            ConversationStatus::Human,
            Actor::Ai
        )
        .await,
        Err(ConversationError::InvalidTransition)
    ));
    repo::transition(
        &app,
        t,
        s.conversation_id,
        ConversationStatus::WaitingHuman,
        Actor::Customer,
    )
    .await
    .unwrap();
    repo::transition(
        &app,
        t,
        s.conversation_id,
        ConversationStatus::Human,
        Actor::Staff(staff),
    )
    .await
    .unwrap();
    let r = repo::resolve_session(&app, t, &s.token)
        .await
        .unwrap()
        .unwrap();
    assert!(
        !r.status.ai_may_reply(),
        "takeover menghentikan auto-response"
    );
    repo::transition(
        &app,
        t,
        s.conversation_id,
        ConversationStatus::Closed,
        Actor::Staff(staff),
    )
    .await
    .unwrap();
    // CLOSED final: tidak menerima pesan & tidak bisa dibuka lagi.
    assert!(matches!(
        repo::append(
            &app,
            t,
            s.conversation_id,
            &customer_msg(s.session_id, "halo?")
        )
        .await,
        Err(ConversationError::Closed)
    ));
    assert!(matches!(
        repo::transition(
            &app,
            t,
            s.conversation_id,
            ConversationStatus::Ai,
            Actor::System
        )
        .await,
        Err(ConversationError::InvalidTransition)
    ));
    // Bypass Rust langsung ke SQL tetap ditolak trigger.
    let mut tx = begin_tenant_tx(&app, t).await.unwrap();
    let e = sqlx::query("UPDATE conversations SET status = 'AI', closed_at = NULL WHERE id = $1")
        .bind(s.conversation_id)
        .execute(&mut *tx)
        .await
        .unwrap_err();
    assert_eq!(code(&e), "23514");
    tx.rollback().await.unwrap();

    let row = sqlx::query("SELECT assignee_user_id, human_involved, closed_at IS NOT NULL AS closed FROM conversations")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(row.get::<Option<Uuid>, _>("assignee_user_id"), Some(staff));
    assert!(row.get::<bool, _>("human_involved"));
    assert!(row.get::<bool, _>("closed"));
    let kinds: Vec<String> =
        sqlx::query_scalar("SELECT kind FROM conversation_events ORDER BY seq")
            .fetch_all(&pool)
            .await
            .unwrap();
    assert_eq!(
        kinds,
        ["created", "handover_requested", "taken_over", "closed"]
    );
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn retention_purges_after_last_message_and_keeps_tickets(pool: PgPool) {
    let app = app_pool(&pool).await;
    let t = tenant(&pool, "toko-a").await;
    let old = repo::start(&app, t, &start_input()).await.unwrap();
    let fresh = repo::start(&app, t, &start_input()).await.unwrap();
    repo::append(
        &app,
        t,
        old.conversation_id,
        &customer_msg(old.session_id, "lama"),
    )
    .await
    .unwrap();
    repo::append(
        &app,
        t,
        fresh.conversation_id,
        &customer_msg(fresh.session_id, "baru"),
    )
    .await
    .unwrap();
    // Ticket & ai_run yang merujuk percakapan lama.
    let mut tx = begin_tenant_tx(&app, t).await.unwrap();
    sqlx::query(
        "INSERT INTO tickets (tenant_id, code, source, conversation_id, subject, first_response_due_at, resolution_due_at)
         VALUES ($1, 'TKT-AAAAAAAAAA', 'chat', $2, 'Tanya', now() + interval '1 hour', now() + interval '1 day')",
    )
    .bind(t.as_uuid())
    .bind(old.conversation_id)
    .execute(&mut *tx)
    .await
    .unwrap();
    // Ticket chat tanpa conversation tetap ditolak saat dibuat.
    let e = sqlx::query(
        "INSERT INTO tickets (tenant_id, code, source, subject, first_response_due_at, resolution_due_at)
         VALUES ($1, 'TKT-BBBBBBBBBB', 'chat', 'x', now() + interval '1 hour', now() + interval '1 day')",
    )
    .bind(t.as_uuid())
    .execute(&mut *tx)
    .await
    .unwrap_err();
    assert_eq!(code(&e), "23514");
    tx.rollback().await.unwrap();
    let mut tx = begin_tenant_tx(&app, t).await.unwrap();
    sqlx::query(
        "INSERT INTO tickets (tenant_id, code, source, conversation_id, subject, first_response_due_at, resolution_due_at)
         VALUES ($1, 'TKT-AAAAAAAAAA', 'chat', $2, 'Tanya', now() + interval '1 hour', now() + interval '1 day')",
    )
    .bind(t.as_uuid())
    .bind(old.conversation_id)
    .execute(&mut *tx)
    .await
    .unwrap();
    sqlx::query("INSERT INTO ai_runs (tenant_id, conversation_id, source, model, status) VALUES ($1, $2, 'platform', 'm', 'ok')")
        .bind(t.as_uuid())
        .bind(old.conversation_id)
        .execute(&mut *tx)
        .await
        .unwrap();
    tx.commit().await.unwrap();
    // Pesan terakhir 31 hari lalu (owner memundurkan jam untuk uji).
    sqlx::query(
        "UPDATE conversations SET last_message_at = now() - interval '31 days' WHERE id = $1",
    )
    .bind(old.conversation_id)
    .execute(&pool)
    .await
    .unwrap();

    // Batas bawah keamanan: cutoff < 7 hari ditolak.
    assert!(
        repo::purge(&app, Duration::from_secs(86_400))
            .await
            .is_err()
    );
    let thirty = Duration::from_secs(30 * 86_400);
    assert_eq!(repo::purge(&app, thirty).await.unwrap(), 1);
    assert_eq!(repo::purge(&app, thirty).await.unwrap(), 0, "idempoten");
    let left: Vec<Uuid> = sqlx::query_scalar("SELECT id FROM conversations")
        .fetch_all(&pool)
        .await
        .unwrap();
    assert_eq!(left, [fresh.conversation_id]);
    let n: i64 = sqlx::query_scalar("SELECT count(*) FROM conversation_messages")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(n, 1, "pesan ikut terhapus (CASCADE)");
    // Ticket & metering tetap ada, rujukan dikosongkan.
    let tc: Option<Uuid> = sqlx::query_scalar("SELECT conversation_id FROM tickets")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert!(tc.is_none());
    let rc: Option<Uuid> = sqlx::query_scalar("SELECT conversation_id FROM ai_runs")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert!(rc.is_none());
}
