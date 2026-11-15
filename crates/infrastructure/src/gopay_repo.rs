//! Penyimpanan integrasi GoPay Merchant (P10, ADR-0020): sesi GoBiz per toko (token disegel
//! envelope, pola sama seperti `ai_provider_secrets`) + charge QRIS dinamis per pesanan.
//! Token hanya ada sebagai plaintext sesaat di fungsi ini; yang tersimpan hanya ciphertext.

use sqlx::{PgPool, Row};
use umkm_domain::commerce::qris::{self, UNIQUE_MAX};
use umkm_shared::ids::TenantId;
use uuid::Uuid;

use crate::{
    crypto::{self, Keyring},
    gopay::GopaySession,
    pgx::{Tx, tenant_tx, violation},
};

#[derive(Debug, thiserror::Error)]
pub enum GopayRepoError {
    #[error(transparent)]
    Db(#[from] sqlx::Error),
    #[error("enkripsi/dekripsi sesi gagal")]
    Crypto,
    #[error("sesi GoPay belum terhubung")]
    NotConnected,
    #[error("QRIS statis toko belum diatur")]
    NoStaticQris,
    #[error("QRIS statis tidak valid")]
    BadQris,
    #[error("tidak ada nominal unik tersisa")]
    NoUniqueCode,
}

/// AAD: mengikat ciphertext sesi ke tenant (dipindah ke tenant lain → gagal dibuka).
fn session_aad(t: TenantId) -> String {
    format!("gopay_session|{}", t.as_uuid())
}

/// Sesi tersimpan + metadata non-rahasia.
#[derive(Debug, Clone)]
pub struct StoredSession {
    pub session: GopaySession,
    pub static_qris: Option<String>,
    pub status: String,
}

/// Charge QRIS dinamis satu pesanan.
#[derive(Debug, Clone)]
pub struct ChargeRow {
    pub order_id: Uuid,
    pub base_rupiah: i64,
    pub unique_code: i16,
    pub total_rupiah: i64,
    pub qris_payload: String,
    pub status: String,
    pub issued_at_ms: i64,
    pub expires_at_ms: i64,
}

// ---- sesi ----

/// Simpan/segel sesi GoBiz toko. `static_qris` dipertahankan bila sudah ada (upsert tanpa menimpa).
pub async fn put_session(
    pool: &PgPool,
    t: TenantId,
    session: &GopaySession,
    k: &Keyring,
) -> Result<(), GopayRepoError> {
    let plain = serde_json::to_vec(session).map_err(|_| GopayRepoError::Crypto)?;
    let sealed =
        crypto::seal(k, &plain, session_aad(t).as_bytes()).map_err(|_| GopayRepoError::Crypto)?;
    let mut tx = tenant_tx(pool, t).await?;
    sqlx::query(
        "INSERT INTO gopay_sessions
           (tenant_id, phone_e164, merchant_id, outlet_name, kek_version, wrapped_dek, ciphertext,
            status, expires_at)
         VALUES ($1, $2, $3, $4, $5, $6, $7, 'active', to_timestamp($8::double precision / 1000.0))
         ON CONFLICT (tenant_id) DO UPDATE SET
           phone_e164 = EXCLUDED.phone_e164, merchant_id = EXCLUDED.merchant_id,
           outlet_name = EXCLUDED.outlet_name, kek_version = EXCLUDED.kek_version,
           wrapped_dek = EXCLUDED.wrapped_dek, ciphertext = EXCLUDED.ciphertext,
           status = 'active', expires_at = EXCLUDED.expires_at, updated_at = now()",
    )
    .bind(t.as_uuid())
    .bind(&session.phone_e164)
    .bind(&session.merchant_id)
    .bind(&session.outlet_name)
    .bind(sealed.kek_version)
    .bind(&sealed.wrapped_dek)
    .bind(&sealed.ciphertext)
    .bind(session.expires_at_ms)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(())
}

/// Ambil & buka sesi toko. `None` bila belum terhubung.
pub async fn get_session(
    pool: &PgPool,
    t: TenantId,
    k: &Keyring,
) -> Result<Option<StoredSession>, GopayRepoError> {
    let mut tx = tenant_tx(pool, t).await?;
    let row = sqlx::query(
        "SELECT kek_version, wrapped_dek, ciphertext, static_qris, status
         FROM gopay_sessions WHERE tenant_id = $1",
    )
    .bind(t.as_uuid())
    .fetch_optional(&mut *tx)
    .await?;
    tx.commit().await?;

    let Some(r) = row else {
        return Ok(None);
    };
    let sealed = crypto::Sealed {
        kek_version: r.try_get("kek_version")?,
        wrapped_dek: r.try_get("wrapped_dek")?,
        ciphertext: r.try_get("ciphertext")?,
    };
    let secret =
        crypto::open(k, &sealed, session_aad(t).as_bytes()).map_err(|_| GopayRepoError::Crypto)?;
    let session: GopaySession =
        serde_json::from_slice(secret.expose()).map_err(|_| GopayRepoError::Crypto)?;
    Ok(Some(StoredSession {
        session,
        static_qris: r.try_get("static_qris")?,
        status: r.try_get("status")?,
    }))
}

/// Atur QRIS statis toko (payload EMVCo tervalidasi). Toko harus sudah terhubung.
pub async fn set_static_qris(
    pool: &PgPool,
    t: TenantId,
    static_qris: &str,
) -> Result<(), GopayRepoError> {
    qris::verify(static_qris).map_err(|_| GopayRepoError::BadQris)?;
    let mut tx = tenant_tx(pool, t).await?;
    let n = sqlx::query(
        "UPDATE gopay_sessions SET static_qris = $2, updated_at = now() WHERE tenant_id = $1",
    )
    .bind(t.as_uuid())
    .bind(static_qris)
    .execute(&mut *tx)
    .await?
    .rows_affected();
    tx.commit().await?;
    if n == 0 {
        return Err(GopayRepoError::NotConnected);
    }
    Ok(())
}

/// Tandai sesi perlu login ulang (mis. refresh gagal 401).
pub async fn mark_needs_relogin(pool: &PgPool, t: TenantId) -> Result<(), GopayRepoError> {
    let mut tx = tenant_tx(pool, t).await?;
    sqlx::query("UPDATE gopay_sessions SET status = 'needs_relogin', updated_at = now() WHERE tenant_id = $1")
        .bind(t.as_uuid())
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(())
}

/// Putuskan hubungan: hapus sesi toko (charge pending tetap ada tapi tak akan dipoll).
pub async fn delete_session(pool: &PgPool, t: TenantId) -> Result<(), GopayRepoError> {
    let mut tx = tenant_tx(pool, t).await?;
    sqlx::query("DELETE FROM gopay_sessions WHERE tenant_id = $1")
        .bind(t.as_uuid())
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(())
}

// ---- charge ----

async fn used_totals(tx: &mut Tx, t: TenantId) -> Result<Vec<i64>, sqlx::Error> {
    let rows = sqlx::query(
        "SELECT total_rupiah FROM gopay_charges WHERE tenant_id = $1 AND status = 'pending'",
    )
    .bind(t.as_uuid())
    .fetch_all(&mut **tx)
    .await?;
    rows.iter().map(|r| r.try_get("total_rupiah")).collect()
}

/// Buat charge QRIS dinamis untuk pesanan: pilih kode unik bebas-tabrakan, sintesis QRIS, simpan.
pub async fn create_charge(
    pool: &PgPool,
    t: TenantId,
    order_id: Uuid,
    base_rupiah: u64,
    static_qris: &str,
    ttl_secs: i64,
) -> Result<ChargeRow, GopayRepoError> {
    qris::verify(static_qris).map_err(|_| GopayRepoError::BadQris)?;
    let mut tx = tenant_tx(pool, t).await?;

    let used: std::collections::HashSet<i64> = used_totals(&mut tx, t).await?.into_iter().collect();
    // Mulai dari offset acak lalu pindai siklik agar kode unik tidak mudah ditebak.
    let mut seed = [0u8; 2];
    let _ = getrandom::fill(&mut seed);
    let start = u16::from_le_bytes(seed) % UNIQUE_MAX;
    let (code, total) = (0..UNIQUE_MAX)
        .map(|i| (start + i) % UNIQUE_MAX + 1) // 1..=UNIQUE_MAX
        .map(|c| (c, base_rupiah + c as u64))
        .find(|(_, total)| !used.contains(&(*total as i64)))
        .ok_or(GopayRepoError::NoUniqueCode)?;

    let payload = qris::to_dynamic(static_qris, total).map_err(|_| GopayRepoError::BadQris)?;

    let row = sqlx::query(
        "INSERT INTO gopay_charges
           (tenant_id, order_id, base_rupiah, unique_code, total_rupiah, qris_payload, expires_at)
         VALUES ($1, $2, $3, $4, $5, $6, now() + make_interval(secs => $7))
         ON CONFLICT (tenant_id, order_id) DO UPDATE SET
           base_rupiah = EXCLUDED.base_rupiah, unique_code = EXCLUDED.unique_code,
           total_rupiah = EXCLUDED.total_rupiah, qris_payload = EXCLUDED.qris_payload,
           status = 'pending', matched_txn_id = NULL, matched_at = NULL,
           issued_at = now(), expires_at = EXCLUDED.expires_at
         RETURNING order_id, base_rupiah, unique_code, total_rupiah, qris_payload, status,
           (extract(epoch from issued_at) * 1000)::bigint AS issued_ms,
           (extract(epoch from expires_at) * 1000)::bigint AS expires_ms",
    )
    .bind(t.as_uuid())
    .bind(order_id)
    .bind(base_rupiah as i64)
    .bind(code as i16)
    .bind(total as i64)
    .bind(&payload)
    .bind(ttl_secs)
    .fetch_one(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(charge_row(&row)?)
}

fn charge_row(r: &sqlx::postgres::PgRow) -> Result<ChargeRow, sqlx::Error> {
    Ok(ChargeRow {
        order_id: r.try_get("order_id")?,
        base_rupiah: r.try_get("base_rupiah")?,
        unique_code: r.try_get("unique_code")?,
        total_rupiah: r.try_get("total_rupiah")?,
        qris_payload: r.try_get("qris_payload")?,
        status: r.try_get("status")?,
        issued_at_ms: r.try_get("issued_ms")?,
        expires_at_ms: r.try_get("expires_ms")?,
    })
}

/// Ambil charge satu pesanan.
pub async fn get_charge(
    pool: &PgPool,
    t: TenantId,
    order_id: Uuid,
) -> Result<Option<ChargeRow>, GopayRepoError> {
    let mut tx = tenant_tx(pool, t).await?;
    let row = sqlx::query(
        "SELECT order_id, base_rupiah, unique_code, total_rupiah, qris_payload, status,
           (extract(epoch from issued_at) * 1000)::bigint AS issued_ms,
           (extract(epoch from expires_at) * 1000)::bigint AS expires_ms
         FROM gopay_charges WHERE tenant_id = $1 AND order_id = $2",
    )
    .bind(t.as_uuid())
    .bind(order_id)
    .fetch_optional(&mut *tx)
    .await?;
    tx.commit().await?;
    row.as_ref().map(charge_row).transpose().map_err(Into::into)
}

/// Charge yang masih menunggu & belum kedaluwarsa (untuk poller, per tenant).
pub async fn pending_charges(pool: &PgPool, t: TenantId) -> Result<Vec<ChargeRow>, GopayRepoError> {
    let mut tx = tenant_tx(pool, t).await?;
    let rows = sqlx::query(
        "SELECT order_id, base_rupiah, unique_code, total_rupiah, qris_payload, status,
           (extract(epoch from issued_at) * 1000)::bigint AS issued_ms,
           (extract(epoch from expires_at) * 1000)::bigint AS expires_ms
         FROM gopay_charges
         WHERE tenant_id = $1 AND status = 'pending' AND expires_at > now()
         ORDER BY issued_at",
    )
    .bind(t.as_uuid())
    .fetch_all(&mut *tx)
    .await?;
    tx.commit().await?;
    rows.iter()
        .map(charge_row)
        .collect::<Result<_, _>>()
        .map_err(Into::into)
}

/// Tandai charge lunas oleh transaksi `txn_id`. `Ok(false)` bila txn sudah diklaim charge lain
/// (indeks unik anti double-claim) atau charge tak lagi pending.
pub async fn mark_matched(
    pool: &PgPool,
    t: TenantId,
    order_id: Uuid,
    txn_id: &str,
) -> Result<bool, GopayRepoError> {
    let mut tx = tenant_tx(pool, t).await?;
    let res = sqlx::query(
        "UPDATE gopay_charges SET status = 'matched', matched_txn_id = $3, matched_at = now()
         WHERE tenant_id = $1 AND order_id = $2 AND status = 'pending'",
    )
    .bind(t.as_uuid())
    .bind(order_id)
    .bind(txn_id)
    .execute(&mut *tx)
    .await;
    match res {
        Ok(r) => {
            tx.commit().await?;
            Ok(r.rows_affected() == 1)
        }
        Err(e) => {
            let _ = tx.rollback().await;
            // Pelanggaran indeks unik txn = transaksi sudah diklaim pesanan lain.
            match violation(&e) {
                Some((s, c)) if s == "23505" && c == "uq_gopay_charges_txn" => Ok(false),
                _ => Err(GopayRepoError::Db(e)),
            }
        }
    }
}

/// Tenant yang punya charge pending belum kedaluwarsa (lintas-tenant, untuk poller worker).
pub async fn due_tenants(pool: &PgPool, limit: i32) -> Result<Vec<TenantId>, GopayRepoError> {
    let ids: Vec<Uuid> = sqlx::query_scalar("SELECT app.gopay_due_tenants($1)")
        .bind(limit)
        .fetch_all(pool)
        .await?;
    Ok(ids.into_iter().map(TenantId::from_uuid).collect())
}

/// Kedaluwarsakan charge pending yang lewat batas waktu, semua tenant (worker). Jumlah terubah.
pub async fn expire_charges_global(pool: &PgPool, batch: i32) -> Result<i32, GopayRepoError> {
    Ok(sqlx::query_scalar("SELECT app.gopay_expire_charges($1)")
        .bind(batch)
        .fetch_one(pool)
        .await?)
}

/// Kedaluwarsakan charge pending yang sudah lewat batas waktu (per tenant). Kembalikan jumlahnya.
pub async fn expire_due(pool: &PgPool, t: TenantId) -> Result<u64, GopayRepoError> {
    let mut tx = tenant_tx(pool, t).await?;
    let n = sqlx::query(
        "UPDATE gopay_charges SET status = 'expired'
         WHERE tenant_id = $1 AND status = 'pending' AND expires_at <= now()",
    )
    .bind(t.as_uuid())
    .execute(&mut *tx)
    .await?
    .rows_affected();
    tx.commit().await?;
    Ok(n)
}
