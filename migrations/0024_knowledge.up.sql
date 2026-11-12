-- P5-7: basis pengetahuan tenant + embedding pgvector (PRD.md §8.2, AC-AIO-004).
-- Satu basis pengetahuan per tenant (dokumen langsung milik tenant). Dimensi vektor tidak dipatok:
-- provider berbeda punya dimensi berbeda; pencarian hanya membandingkan chunk dengan model & dimensi
-- yang sama, selalu di dalam tenant (RLS) — tanpa indeks ANN (korpus per tenant kecil & dibatasi).
CREATE EXTENSION IF NOT EXISTS vector;

CREATE TABLE knowledge_documents (
    id               uuid        NOT NULL DEFAULT gen_random_uuid(),
    tenant_id        uuid        NOT NULL REFERENCES tenants (id) ON DELETE CASCADE,
    title            text        NOT NULL,
    body             text        NOT NULL,
    status           text        NOT NULL DEFAULT 'pending',
    embedding_model  text,
    chunk_count      integer     NOT NULL DEFAULT 0,
    error_code       text,
    attempts         integer     NOT NULL DEFAULT 0,
    created_by_user_id uuid,
    created_at       timestamptz NOT NULL DEFAULT now(),
    updated_at       timestamptz NOT NULL DEFAULT now(),
    indexed_at       timestamptz,
    PRIMARY KEY (id),
    CONSTRAINT uq_knowledge_documents_tenant_id UNIQUE (tenant_id, id),
    CONSTRAINT ck_knowledge_documents_title CHECK (length(btrim(title)) BETWEEN 1 AND 200),
    CONSTRAINT ck_knowledge_documents_body CHECK (length(btrim(body)) BETWEEN 1 AND 50000),
    CONSTRAINT ck_knowledge_documents_status CHECK (status IN ('pending', 'indexed', 'failed', 'disabled')),
    CONSTRAINT ck_knowledge_documents_indexed CHECK ((status = 'indexed') = (indexed_at IS NOT NULL AND embedding_model IS NOT NULL)),
    CONSTRAINT ck_knowledge_documents_error CHECK (error_code IS NULL OR error_code ~ '^[a-z_]{1,40}$')
);
CREATE INDEX ix_knowledge_documents_pending ON knowledge_documents (status, updated_at) WHERE status = 'pending';

CREATE TABLE knowledge_chunks (
    id               uuid        NOT NULL DEFAULT gen_random_uuid(),
    tenant_id        uuid        NOT NULL,
    document_id      uuid        NOT NULL,
    ord              integer     NOT NULL,
    content          text        NOT NULL,
    embedding_model  text        NOT NULL,
    dims             integer     NOT NULL,
    embedding        vector      NOT NULL,
    created_at       timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (id),
    CONSTRAINT fk_knowledge_chunks_document FOREIGN KEY (tenant_id, document_id)
        REFERENCES knowledge_documents (tenant_id, id) ON DELETE CASCADE,
    CONSTRAINT uq_knowledge_chunks_ord UNIQUE (tenant_id, document_id, ord),
    CONSTRAINT ck_knowledge_chunks_content CHECK (length(content) BETWEEN 1 AND 2000),
    CONSTRAINT ck_knowledge_chunks_dims CHECK (dims BETWEEN 1 AND 4096 AND vector_dims(embedding) = dims)
);
CREATE INDEX ix_knowledge_chunks_lookup ON knowledge_chunks (tenant_id, embedding_model, dims);

DO $$
DECLARE t text;
BEGIN
    FOREACH t IN ARRAY ARRAY['knowledge_documents', 'knowledge_chunks'] LOOP
        EXECUTE format('ALTER TABLE %I ENABLE ROW LEVEL SECURITY', t);
        EXECUTE format('ALTER TABLE %I FORCE ROW LEVEL SECURITY', t);
        EXECUTE format('CREATE POLICY tenant_isolation ON %I USING (tenant_id = app.current_tenant_id()) WITH CHECK (tenant_id = app.current_tenant_id())', t);
    END LOOP;
END $$;
