-- P9-7: varian lebar gambar (srcset) dibuat sekali saat pertama diminta, lalu disajikan dari sini.
-- Ikut terhapus bersama objek asli; RLS per tenant seperti stored_objects; immutable (tanpa UPDATE).
CREATE TABLE stored_object_variants (
    tenant_id    uuid        NOT NULL,
    object_id    uuid        NOT NULL,
    width        integer     NOT NULL,
    content_type text        NOT NULL,
    bytes        bytea       NOT NULL,
    created_at   timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (tenant_id, object_id, width),
    FOREIGN KEY (tenant_id, object_id) REFERENCES stored_objects (tenant_id, id) ON DELETE CASCADE,
    CONSTRAINT ck_variant_width CHECK (width IN (320, 640, 960, 1280)),
    CONSTRAINT ck_variant_type CHECK (content_type IN ('image/png', 'image/jpeg')),
    CONSTRAINT ck_variant_bytes CHECK (octet_length(bytes) > 0)
);
ALTER TABLE stored_object_variants ALTER COLUMN bytes SET STORAGE EXTERNAL;

REVOKE UPDATE ON stored_object_variants FROM umkm_app_role;

ALTER TABLE stored_object_variants ENABLE ROW LEVEL SECURITY;
ALTER TABLE stored_object_variants FORCE ROW LEVEL SECURITY;
CREATE POLICY tenant_isolation ON stored_object_variants
    USING (tenant_id = app.current_tenant_id())
    WITH CHECK (tenant_id = app.current_tenant_id());
