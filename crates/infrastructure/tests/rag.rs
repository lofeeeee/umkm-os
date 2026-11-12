//! P5-7 (AC-AIO-004): RAG tenant-scoped di pgvector nyata, role aplikasi (RLS aktif).
//! Embedder tiruan deterministik (bag-of-words ter-hash) — tanpa jaringan.

use std::sync::atomic::{AtomicUsize, Ordering};

use sqlx::PgPool;
use umkm_application::ai::{
    llm::{ChatRequest, ChatResponse, LlmError, LlmPort},
    orchestrator::Retriever,
};
use umkm_database::{MIGRATOR, begin_tenant_tx};
use umkm_domain::{ai::Capabilities, rbac::Actor};
use umkm_infrastructure::rag::{self, PgRetriever, RagError};
use umkm_shared::ids::TenantId;
use uuid::Uuid;

struct HashEmbed {
    dims: usize,
    fail: bool,
    calls: AtomicUsize,
}

impl HashEmbed {
    fn new(dims: usize) -> Self {
        Self {
            dims,
            fail: false,
            calls: AtomicUsize::new(0),
        }
    }
    fn vector(&self, s: &str) -> Vec<f32> {
        let mut v = vec![0f32; self.dims];
        for w in s
            .to_lowercase()
            .split(|c: char| !c.is_alphanumeric())
            .filter(|w| w.len() > 2)
        {
            let h = w
                .bytes()
                .fold(7u64, |a, b| a.wrapping_mul(31).wrapping_add(b as u64));
            v[(h % self.dims as u64) as usize] += 1.0;
        }
        let n = v.iter().map(|x| x * x).sum::<f32>().sqrt().max(1e-6);
        v.iter().map(|x| x / n).collect()
    }
}

impl LlmPort for HashEmbed {
    fn capabilities(&self) -> Capabilities {
        Capabilities::defaults_for("openai")
    }
    async fn chat(&self, _r: ChatRequest) -> Result<ChatResponse, LlmError> {
        Err(LlmError::Unsupported)
    }
    async fn embed(&self, _m: &str, inputs: Vec<String>) -> Result<Vec<Vec<f32>>, LlmError> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        if self.fail {
            return Err(LlmError::Upstream);
        }
        Ok(inputs.iter().map(|s| self.vector(s)).collect())
    }
}

async fn app_pool(pool: &PgPool) -> PgPool {
    sqlx::postgres::PgPoolOptions::new()
        .max_connections(4)
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

async fn tenant(pool: &PgPool, slug: &str) -> TenantId {
    TenantId::from_uuid(
        sqlx::query_scalar("INSERT INTO tenants (name, slug) VALUES ('T', $1) RETURNING id")
            .bind(slug)
            .fetch_one(pool)
            .await
            .unwrap(),
    )
}

fn owner(t: TenantId) -> Actor {
    Actor::new(
        Uuid::new_v4(),
        t,
        "Owner".into(),
        ["ai_config.manage".to_owned()],
    )
}

async fn doc(
    app: &PgPool,
    t: TenantId,
    title: &str,
    body: &str,
    e: &HashEmbed,
    model: &str,
) -> Uuid {
    let id = rag::create_document(app, &owner(t), title, body, (None, None))
        .await
        .unwrap();
    rag::index_document(app, t, id, e, model).await.unwrap();
    id
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn tenant_a_never_receives_tenant_b_chunks_ac_aio_004(pool: PgPool) {
    let app = app_pool(&pool).await;
    let (a, b) = (tenant(&pool, "toko-a").await, tenant(&pool, "toko-b").await);
    let e = HashEmbed::new(64);
    doc(
        &app,
        a,
        "Ongkir A",
        "Ongkir gratis untuk pembelian di atas 100 ribu ke seluruh Jawa.",
        &e,
        "emb-1",
    )
    .await;
    // Dokumen B memakai kata-kata yang PERSIS sama dengan pertanyaan → paling mirip secara vektor.
    doc(
        &app,
        b,
        "Ongkir B",
        "ongkir gratis berapa ongkir gratis berapa RAHASIA-TOKO-B",
        &e,
        "emb-1",
    )
    .await;
    let ra = PgRetriever {
        pool: &app,
        tenant: a,
        llm: &e,
        model: "emb-1",
    };
    let got = ra.retrieve("ongkir gratis berapa", 10).await.unwrap();
    assert!(
        !got.is_empty() && got.iter().all(|c| c.starts_with("Ongkir A")),
        "{got:?}"
    );
    assert!(got.iter().all(|c| !c.contains("RAHASIA-TOKO-B")));
    let rb = PgRetriever {
        pool: &app,
        tenant: b,
        llm: &e,
        model: "emb-1",
    };
    assert!(
        rb.retrieve("ongkir gratis berapa", 10)
            .await
            .unwrap()
            .iter()
            .all(|c| c.starts_with("Ongkir B"))
    );
    // Model berbeda / topik tidak relevan → tidak ada konteks (bukan potongan acak).
    let other = PgRetriever {
        pool: &app,
        tenant: a,
        llm: &e,
        model: "emb-2",
    };
    assert!(
        other
            .retrieve("ongkir gratis", 10)
            .await
            .unwrap()
            .is_empty()
    );
    assert!(ra.retrieve("zzzqqq xxyyww", 10).await.unwrap().is_empty());
    // DB: chunk tidak dapat ditempelkan ke dokumen tenant lain.
    let doc_b: Uuid = sqlx::query_scalar("SELECT id FROM knowledge_documents WHERE tenant_id = $1")
        .bind(b.as_uuid())
        .fetch_one(&pool)
        .await
        .unwrap();
    let mut tx = begin_tenant_tx(&app, a).await.unwrap();
    let err = sqlx::query("INSERT INTO knowledge_chunks (tenant_id, document_id, ord, content, embedding_model, dims, embedding) VALUES ($1, $2, 99, 'x', 'emb-1', 2, '[1,0]')")
        .bind(a.as_uuid())
        .bind(doc_b)
        .execute(&mut *tx)
        .await
        .unwrap_err();
    assert!(
        err.to_string().contains("fk_knowledge_chunks_document"),
        "{err}"
    );
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn indexing_replaces_chunks_marks_failures_and_enforces_limits(pool: PgPool) {
    let app = app_pool(&pool).await;
    let a = tenant(&pool, "toko-a").await;
    let e = HashEmbed::new(32);
    let long = (0..80)
        .map(|i| format!("Paragraf {i} tentang garansi produk elektronik toko."))
        .collect::<Vec<_>>()
        .join("\n\n");
    let id = doc(&app, a, "Garansi", &long, &e, "emb-1").await;
    let docs = rag::list_documents(&app, a).await.unwrap();
    assert_eq!(docs[0].status, "indexed");
    let n = docs[0].chunk_count;
    assert!(n > 1);
    // Indeks ulang tidak menggandakan chunk.
    rag::mark_pending(&app, a, id).await.unwrap();
    rag::index_document(&app, a, id, &e, "emb-1").await.unwrap();
    let count: i64 =
        sqlx::query_scalar("SELECT count(*) FROM knowledge_chunks WHERE document_id = $1")
            .bind(id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(count, n as i64);
    // Provider gagal → status failed + kode, tidak panik.
    let bad = HashEmbed {
        fail: true,
        ..HashEmbed::new(32)
    };
    let id2 = rag::create_document(&app, &owner(a), "Retur", "Retur 7 hari.", (None, None))
        .await
        .unwrap();
    assert!(matches!(
        rag::index_document(&app, a, id2, &bad, "emb-1").await,
        Err(RagError::Provider(LlmError::Upstream))
    ));
    let d = rag::list_documents(&app, a)
        .await
        .unwrap()
        .into_iter()
        .find(|d| d.id == id2)
        .unwrap();
    assert_eq!(
        (d.status.as_str(), d.error_code.as_deref()),
        ("failed", Some("upstream"))
    );
    // Batas dokumen per tenant.
    sqlx::query("INSERT INTO knowledge_documents (tenant_id, title, body) SELECT $1, 'd' || g, 'isi' FROM generate_series(1, 98) g")
        .bind(a.as_uuid())
        .execute(&pool)
        .await
        .unwrap();
    assert!(matches!(
        rag::create_document(&app, &owner(a), "x", "y", (None, None)).await,
        Err(RagError::LimitReached)
    ));
    // Audit tanpa isi dokumen.
    let audit: String = sqlx::query_scalar(
        "SELECT string_agg(summary::text, '') FROM audit_logs WHERE action = 'ai.knowledge_added'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert!(audit.contains("Garansi") && !audit.contains("elektronik"));
}
