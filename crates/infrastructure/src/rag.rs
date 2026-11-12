//! RAG tenant-scoped (P5-7, `PRD.md §8.2`, AC-AIO-004): dokumen → chunk → embedding provider eksternal
//! → pgvector → pengambilan HANYA di tenant context (RLS) dengan model & dimensi yang sama.

use serde_json::json;
use sqlx::PgPool;
use umkm_application::ai::{
    llm::{LlmError, LlmPort},
    orchestrator::Retriever,
};
use umkm_domain::{
    ai::knowledge::{self, CHUNK_MAX, CHUNK_OVERLAP, MAX_CHUNKS, MAX_DOCUMENTS},
    audit::AuditEvent,
    rbac::Actor,
};
use umkm_shared::ids::TenantId;
use uuid::Uuid;

use crate::pgx::tenant_tx;

pub const EMBED_BATCH: usize = 32;
/// Jarak kosinus maksimum agar potongan dianggap relevan (lebih jauh = tidak dipakai sebagai konteks).
pub const MAX_COSINE_DISTANCE: f64 = 0.6;

pub mod actions {
    pub const KNOWLEDGE_ADDED: &str = "ai.knowledge_added";
    pub const KNOWLEDGE_DELETED: &str = "ai.knowledge_deleted";
}

#[derive(Debug, thiserror::Error)]
pub enum RagError {
    #[error("tidak ditemukan")]
    NotFound,
    #[error("batas dokumen/chunk tercapai")]
    LimitReached,
    #[error("provider embedding: {0}")]
    Provider(LlmError),
    #[error(transparent)]
    Db(#[from] sqlx::Error),
}

impl RagError {
    pub fn code(&self) -> &'static str {
        match self {
            Self::NotFound => "not_found",
            Self::LimitReached => "limit_reached",
            Self::Provider(e) => e.code(),
            Self::Db(_) => "db",
        }
    }
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct DocumentRow {
    pub id: Uuid,
    pub title: String,
    pub status: String,
    pub chunk_count: i32,
    pub error_code: Option<String>,
    pub updated_at: String,
}

type Meta<'a> = (Option<&'a str>, Option<&'a str>);

/// Literal pgvector `[a,b,...]`; nilai non-finite ditolak.
pub fn vec_literal(v: &[f32]) -> Option<String> {
    if v.is_empty() || v.iter().any(|x| !x.is_finite()) {
        return None;
    }
    Some(format!(
        "[{}]",
        v.iter()
            .map(|x| x.to_string())
            .collect::<Vec<_>>()
            .join(",")
    ))
}

pub async fn create_document(
    pool: &PgPool,
    actor: &Actor,
    title: &str,
    body: &str,
    (rid, ip): Meta<'_>,
) -> Result<Uuid, RagError> {
    let t = actor.tenant_id;
    let mut tx = tenant_tx(pool, t).await?;
    let n: i64 = sqlx::query_scalar("SELECT count(*) FROM knowledge_documents")
        .fetch_one(&mut *tx)
        .await?;
    if n >= MAX_DOCUMENTS {
        return Err(RagError::LimitReached);
    }
    let id: Uuid = sqlx::query_scalar(
        "INSERT INTO knowledge_documents (tenant_id, title, body, created_by_user_id) VALUES ($1, $2, $3, $4) RETURNING id",
    )
    .bind(t.as_uuid())
    .bind(title.trim())
    .bind(body.trim())
    .bind(actor.user_id)
    .fetch_one(&mut *tx)
    .await?;
    crate::audit_repo::record(
        &mut *tx,
        &AuditEvent::new(actions::KNOWLEDGE_ADDED, "knowledge_document")
            .by_user(actor.user_id)
            .in_tenant(t)
            .resource(id)
            .request(rid, ip)
            .with_summary(json!({ "title": title.trim(), "chars": body.trim().chars().count() })),
    )
    .await?;
    tx.commit().await?;
    Ok(id)
}

pub async fn list_documents(pool: &PgPool, t: TenantId) -> Result<Vec<DocumentRow>, RagError> {
    let mut tx = tenant_tx(pool, t).await?;
    let rows: Vec<(Uuid, String, String, i32, Option<String>, String)> = sqlx::query_as(
        "SELECT id, title, status, chunk_count, error_code,
                to_char(updated_at AT TIME ZONE 'UTC', 'YYYY-MM-DD\"T\"HH24:MI:SS\"Z\"')
         FROM knowledge_documents ORDER BY created_at DESC",
    )
    .fetch_all(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(rows
        .into_iter()
        .map(
            |(id, title, status, chunk_count, error_code, updated_at)| DocumentRow {
                id,
                title,
                status,
                chunk_count,
                error_code,
                updated_at,
            },
        )
        .collect())
}

pub async fn delete_document(
    pool: &PgPool,
    actor: &Actor,
    id: Uuid,
    (rid, ip): Meta<'_>,
) -> Result<(), RagError> {
    let t = actor.tenant_id;
    let mut tx = tenant_tx(pool, t).await?;
    let title: Option<String> =
        sqlx::query_scalar("DELETE FROM knowledge_documents WHERE id = $1 RETURNING title")
            .bind(id)
            .fetch_optional(&mut *tx)
            .await?;
    let title = title.ok_or(RagError::NotFound)?;
    crate::audit_repo::record(
        &mut *tx,
        &AuditEvent::new(actions::KNOWLEDGE_DELETED, "knowledge_document")
            .by_user(actor.user_id)
            .in_tenant(t)
            .resource(id)
            .request(rid, ip)
            .with_summary(json!({ "title": title })),
    )
    .await?;
    tx.commit().await?;
    Ok(())
}

/// Tandai dokumen untuk diindeks ulang (mis. setelah ganti model embedding).
pub async fn mark_pending(pool: &PgPool, t: TenantId, id: Uuid) -> Result<(), RagError> {
    let mut tx = tenant_tx(pool, t).await?;
    let n = sqlx::query(
        "UPDATE knowledge_documents SET status = 'pending', indexed_at = NULL, embedding_model = NULL,
            error_code = NULL, attempts = 0, updated_at = now() WHERE id = $1",
    )
    .bind(id)
    .execute(&mut *tx)
    .await?
    .rows_affected();
    tx.commit().await?;
    if n == 0 {
        Err(RagError::NotFound)
    } else {
        Ok(())
    }
}

/// Indeks satu dokumen: chunk → embed per batch → ganti chunk lama atomik → `indexed`.
/// Gagal → `failed` + error_code (chunk lama dibiarkan apa adanya).
pub async fn index_document<L: LlmPort>(
    pool: &PgPool,
    t: TenantId,
    id: Uuid,
    llm: &L,
    model: &str,
) -> Result<usize, RagError> {
    let res = index_inner(pool, t, id, llm, model).await;
    if let Err(e) = &res
        && !matches!(e, RagError::NotFound)
    {
        let mut tx = tenant_tx(pool, t).await?;
        sqlx::query(
            "UPDATE knowledge_documents SET status = 'failed', error_code = $2, attempts = attempts + 1,
                indexed_at = NULL, embedding_model = NULL, updated_at = now() WHERE id = $1",
        )
        .bind(id)
        .bind(e.code())
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
    }
    res
}

async fn index_inner<L: LlmPort>(
    pool: &PgPool,
    t: TenantId,
    id: Uuid,
    llm: &L,
    model: &str,
) -> Result<usize, RagError> {
    let (title, body): (String, String) = {
        let mut tx = tenant_tx(pool, t).await?;
        let r = sqlx::query_as("SELECT title, body FROM knowledge_documents WHERE id = $1")
            .bind(id)
            .fetch_optional(&mut *tx)
            .await?
            .ok_or(RagError::NotFound)?;
        tx.commit().await?;
        r
    };
    let chunks = knowledge::chunk(&title, &body, CHUNK_MAX, CHUNK_OVERLAP);
    // Embedding di luar transaksi (panggilan jaringan), lalu tulis atomik.
    let mut vectors = Vec::with_capacity(chunks.len());
    for batch in chunks.chunks(EMBED_BATCH) {
        let v = llm
            .embed(model, batch.to_vec())
            .await
            .map_err(RagError::Provider)?;
        if v.len() != batch.len() {
            return Err(RagError::Provider(LlmError::InvalidResponse));
        }
        vectors.extend(v);
    }
    let lits: Vec<String> = vectors
        .iter()
        .map(|v| vec_literal(v))
        .collect::<Option<_>>()
        .ok_or(RagError::Provider(LlmError::InvalidResponse))?;
    let mut tx = tenant_tx(pool, t).await?;
    let others: i64 =
        sqlx::query_scalar("SELECT count(*) FROM knowledge_chunks WHERE document_id <> $1")
            .bind(id)
            .fetch_one(&mut *tx)
            .await?;
    if others + chunks.len() as i64 > MAX_CHUNKS {
        return Err(RagError::LimitReached);
    }
    sqlx::query("DELETE FROM knowledge_chunks WHERE document_id = $1")
        .bind(id)
        .execute(&mut *tx)
        .await?;
    for (i, (c, lit)) in chunks.iter().zip(&lits).enumerate() {
        sqlx::query(
            "INSERT INTO knowledge_chunks (tenant_id, document_id, ord, content, embedding_model, dims, embedding)
             VALUES ($1, $2, $3, $4, $5, $6, $7::vector)",
        )
        .bind(t.as_uuid())
        .bind(id)
        .bind(i as i32)
        .bind(c)
        .bind(model)
        .bind(vectors[i].len() as i32)
        .bind(lit)
        .execute(&mut *tx)
        .await?;
    }
    sqlx::query(
        "UPDATE knowledge_documents SET status = 'indexed', embedding_model = $2, chunk_count = $3,
            error_code = NULL, indexed_at = now(), updated_at = now() WHERE id = $1",
    )
    .bind(id)
    .bind(model)
    .bind(chunks.len() as i32)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(chunks.len())
}

/// Retriever terikat SATU tenant: konteks RLS tenant di setiap query; tidak ada parameter tenant lain.
pub struct PgRetriever<'a, L: LlmPort> {
    pub pool: &'a PgPool,
    pub tenant: TenantId,
    pub llm: &'a L,
    pub model: &'a str,
}

impl<L: LlmPort> Retriever for PgRetriever<'_, L> {
    async fn retrieve(&self, query: &str, k: usize) -> Result<Vec<String>, LlmError> {
        let mut v = self.llm.embed(self.model, vec![query.to_owned()]).await?;
        let q = v.pop().ok_or(LlmError::InvalidResponse)?;
        let lit = vec_literal(&q).ok_or(LlmError::InvalidResponse)?;
        let fetch = async {
            let mut tx = tenant_tx(self.pool, self.tenant).await?;
            let rows: Vec<String> = sqlx::query_scalar(
                "SELECT c.content FROM knowledge_chunks c
                 JOIN knowledge_documents d ON d.tenant_id = c.tenant_id AND d.id = c.document_id
                 WHERE d.status = 'indexed' AND c.embedding_model = $1 AND c.dims = $2
                   AND (c.embedding <=> $3::vector) <= $4
                 ORDER BY c.embedding <=> $3::vector LIMIT $5",
            )
            .bind(self.model)
            .bind(q.len() as i32)
            .bind(&lit)
            .bind(MAX_COSINE_DISTANCE)
            .bind(k.min(10) as i64)
            .fetch_all(&mut *tx)
            .await?;
            tx.commit().await?;
            Ok::<_, sqlx::Error>(rows)
        };
        fetch.await.map_err(|_| LlmError::Network("retrieval_db"))
    }
}
