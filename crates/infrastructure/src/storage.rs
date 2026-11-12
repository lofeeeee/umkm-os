//! Object storage (P2-6, ADR-0009): port provider-neutral + adapter PostgreSQL + signed URL.
//!
//! Semua akses berjalan dalam transaksi ber-tenant-context (RLS) dan tetap memfilter
//! `tenant_id` (defense-in-depth). Signed URL mengikat tenant + objek + expiry dengan
//! HMAC-SHA256; URL tenant A tidak dapat dipakai membaca objek tenant B.

use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use hmac::{Hmac, KeyInit, Mac};
use sha2::{Digest, Sha256};
use sqlx::{PgPool, Row};
use umkm_domain::{
    audit::AuditEvent,
    rbac::Actor,
    storage::{ImageType, UploadError, inspect_image},
};
use umkm_shared::{config::SecretString, ids::TenantId};
use uuid::Uuid;

pub mod actions {
    pub const OBJECT_UPLOADED: &str = "storage.object.uploaded";
    pub const OBJECT_DELETED: &str = "storage.object.deleted";
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredObjectMeta {
    pub id: Uuid,
    pub content_type: String,
    pub byte_size: i32,
    pub created_at_unix: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredObject {
    pub content_type: ImageType,
    pub bytes: Vec<u8>,
    pub sha256_hex: String,
}

#[derive(Debug, thiserror::Error)]
pub enum StorageError {
    #[error(transparent)]
    Rejected(#[from] UploadError),
    #[error("objek masih dirujuk halaman website")]
    InUse,
    #[error(transparent)]
    Db(#[from] sqlx::Error),
}

/// Port storage (ADR-0009). Adapter S3 kelak mengimplementasikan trait yang sama.
pub trait ObjectStorage: Send + Sync {
    fn put(
        &self,
        actor: &Actor,
        bytes: &[u8],
        audit_ctx: (Option<&str>, Option<&str>),
    ) -> impl Future<Output = Result<StoredObjectMeta, StorageError>> + Send;
    fn get(
        &self,
        tenant: TenantId,
        id: Uuid,
    ) -> impl Future<Output = Result<Option<StoredObject>, StorageError>> + Send;
    fn list(
        &self,
        tenant: TenantId,
    ) -> impl Future<Output = Result<Vec<StoredObjectMeta>, StorageError>> + Send;
    fn delete(
        &self,
        actor: &Actor,
        id: Uuid,
        audit_ctx: (Option<&str>, Option<&str>),
    ) -> impl Future<Output = Result<bool, StorageError>> + Send;
    /// Subset `ids` yang ADA dan milik tenant (untuk validasi `asset:<uuid>`).
    fn existing(
        &self,
        tenant: TenantId,
        ids: &[Uuid],
    ) -> impl Future<Output = Result<Vec<Uuid>, StorageError>> + Send;
}

#[derive(Debug, Clone, Copy)]
pub struct StorageLimits {
    pub max_object_bytes: usize,
    pub max_objects_per_tenant: i64,
}

#[derive(Clone)]
pub struct PgObjectStorage {
    pool: PgPool,
    limits: StorageLimits,
}

impl PgObjectStorage {
    pub fn new(pool: PgPool, limits: StorageLimits) -> Self {
        Self { pool, limits }
    }
}

fn db_err(e: umkm_database::DbError) -> sqlx::Error {
    match e {
        umkm_database::DbError::Sqlx(e) => e,
        umkm_database::DbError::InvalidUrl => sqlx::Error::Configuration("invalid url".into()),
    }
}

fn meta(r: &sqlx::postgres::PgRow) -> StoredObjectMeta {
    StoredObjectMeta {
        id: r.get("id"),
        content_type: r.get("content_type"),
        byte_size: r.get("byte_size"),
        created_at_unix: r.get("created_unix"),
    }
}

impl ObjectStorage for PgObjectStorage {
    async fn put(
        &self,
        actor: &Actor,
        bytes: &[u8],
        (request_id, ip): (Option<&str>, Option<&str>),
    ) -> Result<StoredObjectMeta, StorageError> {
        let ty = inspect_image(bytes, self.limits.max_object_bytes)?;
        let digest = Sha256::digest(bytes);
        let mut tx = umkm_database::begin_tenant_tx(&self.pool, actor.tenant_id)
            .await
            .map_err(db_err)?;
        // Serialisasi upload per tenant agar kuota tidak terlampaui oleh request paralel.
        sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended('storage:' || $1::text, 0))")
            .bind(actor.tenant_id.as_uuid())
            .execute(&mut *tx)
            .await?;
        let count: i64 =
            sqlx::query_scalar("SELECT count(*) FROM stored_objects WHERE tenant_id = $1")
                .bind(actor.tenant_id.as_uuid())
                .fetch_one(&mut *tx)
                .await?;
        if count >= self.limits.max_objects_per_tenant {
            return Err(UploadError::QuotaExceeded.into());
        }
        let row = sqlx::query(
            "INSERT INTO stored_objects (tenant_id, content_type, byte_size, sha256, bytes, uploaded_by)
             VALUES ($1, $2, $3, $4, $5, $6)
             RETURNING id, content_type, byte_size, extract(epoch FROM created_at)::bigint AS created_unix",
        )
        .bind(actor.tenant_id.as_uuid())
        .bind(ty.content_type())
        .bind(i32::try_from(bytes.len()).unwrap_or(i32::MAX))
        .bind(digest.as_slice())
        .bind(bytes)
        .bind(actor.user_id)
        .fetch_one(&mut *tx)
        .await?;
        let m = meta(&row);
        crate::audit_repo::record(
            &mut *tx,
            &AuditEvent::new(actions::OBJECT_UPLOADED, "stored_object")
                .by_user(actor.user_id)
                .in_tenant(actor.tenant_id)
                .resource(m.id)
                .request(request_id, ip)
                .with_summary(serde_json::json!({
                    "content_type": m.content_type, "byte_size": m.byte_size
                })),
        )
        .await?;
        tx.commit().await?;
        Ok(m)
    }

    async fn get(&self, tenant: TenantId, id: Uuid) -> Result<Option<StoredObject>, StorageError> {
        let mut tx = umkm_database::begin_tenant_tx(&self.pool, tenant)
            .await
            .map_err(db_err)?;
        let row = sqlx::query(
            "SELECT content_type, bytes, sha256 FROM stored_objects WHERE tenant_id = $1 AND id = $2",
        )
        .bind(tenant.as_uuid())
        .bind(id)
        .fetch_optional(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(row.and_then(|r| {
            let ty = ImageType::from_content_type(r.get("content_type"))?;
            let sha: Vec<u8> = r.get("sha256");
            Some(StoredObject {
                content_type: ty,
                bytes: r.get("bytes"),
                sha256_hex: sha.iter().map(|b| format!("{b:02x}")).collect(),
            })
        }))
    }

    async fn list(&self, tenant: TenantId) -> Result<Vec<StoredObjectMeta>, StorageError> {
        let mut tx = umkm_database::begin_tenant_tx(&self.pool, tenant)
            .await
            .map_err(db_err)?;
        let rows = sqlx::query(
            "SELECT id, content_type, byte_size, extract(epoch FROM created_at)::bigint AS created_unix
             FROM stored_objects WHERE tenant_id = $1 ORDER BY created_at DESC, id LIMIT 500",
        )
        .bind(tenant.as_uuid())
        .fetch_all(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(rows.iter().map(meta).collect())
    }

    async fn delete(
        &self,
        actor: &Actor,
        id: Uuid,
        (request_id, ip): (Option<&str>, Option<&str>),
    ) -> Result<bool, StorageError> {
        let mut tx = umkm_database::begin_tenant_tx(&self.pool, actor.tenant_id)
            .await
            .map_err(db_err)?;
        // Objek yang dirujuk draft atau versi mana pun tidak boleh dihapus: versi terbit
        // immutable dan rollback harus tetap menampilkan gambarnya.
        let needle = format!("\"asset:{id}\"");
        let in_use: bool = sqlx::query_scalar(
            "SELECT EXISTS (SELECT 1 FROM website_pages WHERE tenant_id = $1 AND strpos(draft_content::text, $2) > 0)
                 OR EXISTS (SELECT 1 FROM website_page_versions WHERE tenant_id = $1 AND strpos(content_json::text, $2) > 0)",
        )
        .bind(actor.tenant_id.as_uuid())
        .bind(&needle)
        .fetch_one(&mut *tx)
        .await?;
        if in_use {
            return Err(StorageError::InUse);
        }
        // FK RESTRICT (logo/cover business profile, P2-5) -> 23503 = masih dipakai.
        let n = match sqlx::query("DELETE FROM stored_objects WHERE tenant_id = $1 AND id = $2")
            .bind(actor.tenant_id.as_uuid())
            .bind(id)
            .execute(&mut *tx)
            .await
        {
            Ok(r) => r.rows_affected(),
            Err(e) if e.as_database_error().and_then(|d| d.code()).as_deref() == Some("23503") => {
                return Err(StorageError::InUse);
            }
            Err(e) => return Err(e.into()),
        };
        if n == 1 {
            crate::audit_repo::record(
                &mut *tx,
                &AuditEvent::new(actions::OBJECT_DELETED, "stored_object")
                    .by_user(actor.user_id)
                    .in_tenant(actor.tenant_id)
                    .resource(id)
                    .request(request_id, ip),
            )
            .await?;
        }
        tx.commit().await?;
        Ok(n == 1)
    }

    async fn existing(&self, tenant: TenantId, ids: &[Uuid]) -> Result<Vec<Uuid>, StorageError> {
        if ids.is_empty() {
            return Ok(vec![]);
        }
        let mut tx = umkm_database::begin_tenant_tx(&self.pool, tenant)
            .await
            .map_err(db_err)?;
        let found = sqlx::query_scalar(
            "SELECT id FROM stored_objects WHERE tenant_id = $1 AND id = ANY($2)",
        )
        .bind(tenant.as_uuid())
        .bind(ids)
        .fetch_all(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(found)
    }
}

// ------------------------------------------------------------------ signed URL

/// Parameter signed URL yang sudah diverifikasi.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VerifiedFile {
    pub tenant: TenantId,
    pub object: Uuid,
    pub expires_unix: u64,
}

#[derive(Clone)]
pub struct UrlSigner {
    key: SecretString,
    ttl_secs: u64,
}

impl std::fmt::Debug for UrlSigner {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("UrlSigner")
            .field("ttl_secs", &self.ttl_secs)
            .finish()
    }
}

impl UrlSigner {
    pub fn new(key: SecretString, ttl_secs: u64) -> Self {
        Self { key, ttl_secs }
    }

    fn mac(&self, tenant: TenantId, object: Uuid, exp: u64) -> Hmac<Sha256> {
        // `new_from_slice` hanya gagal untuk panjang kunci tak didukung; HMAC menerima semua.
        let mut m = <Hmac<Sha256> as KeyInit>::new_from_slice(self.key.expose().as_bytes())
            .expect("HMAC menerima kunci sembarang panjang");
        m.update(format!("umkm-file-v1|{}|{object}|{exp}", tenant.as_uuid()).as_bytes());
        m
    }

    /// Expiry dibulatkan ke atas ke kelipatan `ttl/2` agar URL stabil (cache browser)
    /// sambil menjamin sisa masa berlaku ≥ ttl/2 dan ≤ ttl.
    pub fn expiry_for(&self, now_unix: u64) -> u64 {
        let step = (self.ttl_secs / 2).max(1);
        (now_unix / step + 2) * step
    }

    /// Path + query relatif terhadap origin API: `/files/{id}?t=..&exp=..&sig=..`.
    pub fn sign(&self, tenant: TenantId, object: Uuid, now_unix: u64) -> String {
        let exp = self.expiry_for(now_unix);
        let sig = URL_SAFE_NO_PAD.encode(self.mac(tenant, object, exp).finalize().into_bytes());
        format!("/files/{object}?t={}&exp={exp}&sig={sig}", tenant.as_uuid())
    }

    /// Verifikasi konstan-waktu. `None` bila signature salah, kedaluwarsa, atau expiry
    /// melampaui batas wajar (mencegah URL "abadi" dari kunci yang bocor sebagian).
    pub fn verify(
        &self,
        object: Uuid,
        tenant: Uuid,
        exp: u64,
        sig: &str,
        now_unix: u64,
    ) -> Option<VerifiedFile> {
        if exp <= now_unix || exp > now_unix + self.ttl_secs * 2 {
            return None;
        }
        let raw = URL_SAFE_NO_PAD.decode(sig).ok()?;
        let tenant = TenantId::from_uuid(tenant);
        self.mac(tenant, object, exp).verify_slice(&raw).ok()?;
        Some(VerifiedFile {
            tenant,
            object,
            expires_unix: exp,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn signer() -> UrlSigner {
        UrlSigner::new(SecretString::new("k".repeat(48)), 3600)
    }

    fn parts(url: &str) -> (Uuid, Uuid, u64, String) {
        let (path, q) = url.split_once('?').unwrap();
        let obj = path.trim_start_matches("/files/").parse().unwrap();
        let mut t = Uuid::nil();
        let (mut exp, mut sig) = (0, String::new());
        for kv in q.split('&') {
            let (k, v) = kv.split_once('=').unwrap();
            match k {
                "t" => t = v.parse().unwrap(),
                "exp" => exp = v.parse().unwrap(),
                _ => sig = v.to_owned(),
            }
        }
        (obj, t, exp, sig)
    }

    #[test]
    fn roundtrip_and_tamper_detection() {
        let s = signer();
        let (a, obj) = (TenantId::from_uuid(Uuid::new_v4()), Uuid::new_v4());
        let now = 1_800_000_000;
        let (o, t, exp, sig) = parts(&s.sign(a, obj, now));
        assert_eq!(s.verify(o, t, exp, &sig, now).unwrap().tenant, a);
        // Objek lain, tenant lain, expiry diubah, signature diubah -> ditolak
        assert!(s.verify(Uuid::new_v4(), t, exp, &sig, now).is_none());
        assert!(s.verify(o, Uuid::new_v4(), exp, &sig, now).is_none());
        assert!(s.verify(o, t, exp + 1800, &sig, now).is_none());
        let mut bad = sig.clone();
        bad.replace_range(0..1, if bad.starts_with('A') { "B" } else { "A" });
        assert!(s.verify(o, t, exp, &bad, now).is_none());
        // Kunci lain
        let other = UrlSigner::new(SecretString::new("z".repeat(48)), 3600);
        assert!(other.verify(o, t, exp, &sig, now).is_none());
    }

    #[test]
    fn expiry_is_bounded_and_enforced() {
        let s = signer();
        for now in [1_800_000_000u64, 1_800_000_001, 1_800_001_799] {
            let exp = s.expiry_for(now);
            assert!(exp - now >= 1800 && exp - now <= 3600, "{now} {exp}");
        }
        let (a, obj) = (TenantId::from_uuid(Uuid::new_v4()), Uuid::new_v4());
        let (o, t, exp, sig) = parts(&s.sign(a, obj, 1_800_000_000));
        assert!(
            s.verify(o, t, exp, &sig, exp).is_none(),
            "tepat saat expiry ditolak"
        );
        assert!(s.verify(o, t, exp, &sig, exp - 1).is_some());
    }

    #[test]
    fn debug_never_prints_the_key() {
        assert!(!format!("{:?}", signer()).contains("kkkk"));
    }
}
