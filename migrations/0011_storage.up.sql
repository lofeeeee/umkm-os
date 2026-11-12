-- P2-6: Object storage lokal di PostgreSQL (ADR-0009, PRD.md §6.3, §6.8).
-- Key = UUID acak; nama file/path asli tidak pernah disimpan. Role aplikasi: SELECT/INSERT/DELETE
-- (tanpa UPDATE: objek immutable; ganti = upload baru).

CREATE TABLE stored_objects (
    id           uuid        NOT NULL DEFAULT gen_random_uuid(),
    tenant_id    uuid        NOT NULL REFERENCES tenants (id) ON DELETE CASCADE,
    content_type text        NOT NULL,
    byte_size    integer     NOT NULL,
    sha256       bytea       NOT NULL,
    bytes        bytea       NOT NULL,
    uploaded_by  uuid        REFERENCES users (id) ON DELETE SET NULL,
    created_at   timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (id),
    CONSTRAINT uq_stored_objects_tenant_id UNIQUE (tenant_id, id),
    CONSTRAINT ck_stored_objects_type CHECK (content_type IN ('image/png', 'image/jpeg', 'image/webp')),
    CONSTRAINT ck_stored_objects_size CHECK (byte_size > 0 AND byte_size = octet_length(bytes)),
    CONSTRAINT ck_stored_objects_sha CHECK (octet_length(sha256) = 32)
);
CREATE INDEX ix_stored_objects_tenant_created ON stored_objects (tenant_id, created_at DESC);
-- Kolom bytes besar: simpan di TOAST tanpa kompresi (gambar sudah terkompresi).
ALTER TABLE stored_objects ALTER COLUMN bytes SET STORAGE EXTERNAL;

REVOKE UPDATE ON stored_objects FROM umkm_app_role;

ALTER TABLE stored_objects ENABLE ROW LEVEL SECURITY;
ALTER TABLE stored_objects FORCE ROW LEVEL SECURITY;
CREATE POLICY tenant_isolation ON stored_objects
    USING (tenant_id = app.current_tenant_id())
    WITH CHECK (tenant_id = app.current_tenant_id());
