//! Repository autentikasi (P1-2). Tabel `users`, `user_sessions`, `user_tokens`
//! bersifat global (bukan milik tenant), sehingga tidak memakai tenant context.
//! Semua operasi pada sesi/token dibatasi `user_id` (object-level, `PRD.md §6.2`).

use std::time::Duration;

use sqlx::{PgExecutor, Row};
use umkm_domain::auth::{Email, TokenHash};
use uuid::Uuid;

#[derive(Debug, thiserror::Error)]
pub enum RepoError {
    #[error("email sudah terdaftar")]
    EmailTaken,
    #[error("database: {0}")]
    Db(#[from] sqlx::Error),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TokenPurpose {
    EmailVerification,
    PasswordReset,
}

impl TokenPurpose {
    fn as_str(self) -> &'static str {
        match self {
            Self::EmailVerification => "email_verification",
            Self::PasswordReset => "password_reset",
        }
    }
}

#[derive(Debug, Clone)]
pub struct LoginRecord {
    pub user_id: Uuid,
    pub password_hash: String,
    pub active: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SessionInfo {
    pub session_id: Uuid,
    pub user_id: Uuid,
}

fn secs(d: Duration) -> f64 {
    d.as_secs_f64()
}

pub async fn create_user<'e>(
    db: impl PgExecutor<'e>,
    email: &Email,
    password_hash: &str,
) -> Result<Uuid, RepoError> {
    sqlx::query_scalar("INSERT INTO users (email, password_hash) VALUES ($1, $2) RETURNING id")
        .bind(email.as_str())
        .bind(password_hash)
        .fetch_one(db)
        .await
        .map_err(
            |e| match e.as_database_error().and_then(|d| d.constraint()) {
                Some("uq_users_email") => RepoError::EmailTaken,
                _ => RepoError::Db(e),
            },
        )
}

/// Cari user untuk login (case-insensitive, AUTH-002).
pub async fn find_login<'e>(
    db: impl PgExecutor<'e>,
    email: &Email,
) -> Result<Option<LoginRecord>, RepoError> {
    let row = sqlx::query(
        "SELECT id, password_hash, status = 'active' AS active FROM users WHERE lower(email) = lower($1)",
    )
    .bind(email.as_str())
    .fetch_optional(db)
    .await?;
    Ok(row.map(|r| LoginRecord {
        user_id: r.get("id"),
        password_hash: r.get("password_hash"),
        active: r.get("active"),
    }))
}

pub async fn update_password_hash<'e>(
    db: impl PgExecutor<'e>,
    user_id: Uuid,
    password_hash: &str,
) -> Result<(), RepoError> {
    sqlx::query("UPDATE users SET password_hash = $2 WHERE id = $1")
        .bind(user_id)
        .bind(password_hash)
        .execute(db)
        .await?;
    Ok(())
}

pub async fn create_session<'e>(
    db: impl PgExecutor<'e>,
    user_id: Uuid,
    token: TokenHash,
    ttl: Duration,
    user_agent: Option<&str>,
) -> Result<Uuid, RepoError> {
    let ua = user_agent.map(|s| s.chars().take(512).collect::<String>());
    Ok(sqlx::query_scalar(
        "INSERT INTO user_sessions (user_id, token_hash, expires_at, user_agent)
         VALUES ($1, $2, now() + make_interval(secs => $3), $4) RETURNING id",
    )
    .bind(user_id)
    .bind(&token.0[..])
    .bind(secs(ttl))
    .bind(ua)
    .fetch_one(db)
    .await?)
}

/// Sesi valid: belum dicabut, belum kedaluwarsa, user masih aktif.
/// Memperbarui `last_seen_at` sekaligus.
pub async fn authenticate_session<'e>(
    db: impl PgExecutor<'e>,
    token: TokenHash,
) -> Result<Option<SessionInfo>, RepoError> {
    let row = sqlx::query(
        "UPDATE user_sessions s SET last_seen_at = now()
         FROM users u
         WHERE s.token_hash = $1 AND s.revoked_at IS NULL AND s.expires_at > now()
           AND u.id = s.user_id AND u.status = 'active'
         RETURNING s.id, s.user_id",
    )
    .bind(&token.0[..])
    .fetch_optional(db)
    .await?;
    Ok(row.map(|r| SessionInfo {
        session_id: r.get("id"),
        user_id: r.get("user_id"),
    }))
}

/// Cabut satu sesi milik `user_id` (AUTH-006). Sesi user lain tidak tersentuh.
pub async fn revoke_session<'e>(
    db: impl PgExecutor<'e>,
    user_id: Uuid,
    session_id: Uuid,
) -> Result<bool, RepoError> {
    let n = sqlx::query(
        "UPDATE user_sessions SET revoked_at = now()
         WHERE id = $1 AND user_id = $2 AND revoked_at IS NULL",
    )
    .bind(session_id)
    .bind(user_id)
    .execute(db)
    .await?
    .rows_affected();
    Ok(n == 1)
}

/// Cabut semua sesi user (AUTH-006), opsional kecuali sesi saat ini.
pub async fn revoke_all_sessions<'e>(
    db: impl PgExecutor<'e>,
    user_id: Uuid,
    except: Option<Uuid>,
) -> Result<u64, RepoError> {
    Ok(sqlx::query(
        "UPDATE user_sessions SET revoked_at = now()
         WHERE user_id = $1 AND revoked_at IS NULL AND ($2::uuid IS NULL OR id <> $2)",
    )
    .bind(user_id)
    .bind(except)
    .execute(db)
    .await?
    .rows_affected())
}

pub async fn create_user_token<'e>(
    db: impl PgExecutor<'e>,
    user_id: Uuid,
    purpose: TokenPurpose,
    token: TokenHash,
    ttl: Duration,
) -> Result<(), RepoError> {
    sqlx::query(
        "INSERT INTO user_tokens (user_id, purpose, token_hash, expires_at)
         VALUES ($1, $2, $3, now() + make_interval(secs => $4))",
    )
    .bind(user_id)
    .bind(purpose.as_str())
    .bind(&token.0[..])
    .bind(secs(ttl))
    .execute(db)
    .await?;
    Ok(())
}

/// Konsumsi token sekali pakai secara atomik (AUTH-005). Mengembalikan `user_id`
/// hanya bila token cocok, tujuan sama, belum dipakai, dan belum kedaluwarsa.
/// Dua request paralel dengan token sama: tepat satu yang berhasil.
pub async fn consume_user_token<'e>(
    db: impl PgExecutor<'e>,
    purpose: TokenPurpose,
    token: TokenHash,
) -> Result<Option<Uuid>, RepoError> {
    Ok(sqlx::query_scalar(
        "UPDATE user_tokens SET consumed_at = now()
         WHERE token_hash = $1 AND purpose = $2 AND consumed_at IS NULL AND expires_at > now()
         RETURNING user_id",
    )
    .bind(&token.0[..])
    .bind(purpose.as_str())
    .fetch_optional(db)
    .await?)
}

/// Batalkan token tertunda lain dengan tujuan sama (mis. setelah reset berhasil).
pub async fn invalidate_user_tokens<'e>(
    db: impl PgExecutor<'e>,
    user_id: Uuid,
    purpose: TokenPurpose,
) -> Result<u64, RepoError> {
    Ok(sqlx::query(
        "UPDATE user_tokens SET consumed_at = now()
         WHERE user_id = $1 AND purpose = $2 AND consumed_at IS NULL",
    )
    .bind(user_id)
    .bind(purpose.as_str())
    .execute(db)
    .await?
    .rows_affected())
}
