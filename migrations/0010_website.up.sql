-- P2-1: Website builder (PRD.md §4.3, §5.4; ADR-0007, ADR-0014).
--
-- website_templates      katalog global berversi (hanya-baca bagi role aplikasi)
-- tenant_websites        satu website per tenant, mengacu ke VERSI template tertentu (AC-WEB-003)
-- website_pages          halaman + draft (dapat diubah) + pointer versi terbit
-- website_page_versions  versi terbit IMMUTABLE (role aplikasi hanya SELECT/INSERT)
--
-- Validasi isi content_json ada di Rust (crates/domain/src/website/schema.rs); database
-- menjaga bentuk minimum, kepemilikan tenant (composite FK), dan imutabilitas.

CREATE TABLE website_templates (
    id              uuid        PRIMARY KEY DEFAULT gen_random_uuid(),
    key             text        NOT NULL,
    version         integer     NOT NULL,
    name            text        NOT NULL,
    category        text        NOT NULL,
    status          text        NOT NULL DEFAULT 'active',
    default_content jsonb       NOT NULL,
    created_at      timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT uq_website_templates_key_version UNIQUE (key, version),
    CONSTRAINT ck_website_templates_key CHECK (key ~ '^[a-z0-9_-]{1,40}$'),
    CONSTRAINT ck_website_templates_version CHECK (version >= 1),
    CONSTRAINT ck_website_templates_status CHECK (status IN ('active', 'retired')),
    CONSTRAINT ck_website_templates_content CHECK (jsonb_typeof(default_content) = 'object')
);

CREATE TABLE tenant_websites (
    id          uuid        NOT NULL DEFAULT gen_random_uuid(),
    tenant_id   uuid        NOT NULL REFERENCES tenants (id) ON DELETE CASCADE,
    template_id uuid        NOT NULL REFERENCES website_templates (id),
    status      text        NOT NULL DEFAULT 'draft',
    created_at  timestamptz NOT NULL DEFAULT now(),
    updated_at  timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (id),
    CONSTRAINT uq_tenant_websites_tenant UNIQUE (tenant_id),
    CONSTRAINT uq_tenant_websites_tenant_id UNIQUE (tenant_id, id),
    CONSTRAINT ck_tenant_websites_status CHECK (status IN ('draft', 'published'))
);
CREATE TRIGGER trg_tenant_websites_updated_at BEFORE UPDATE ON tenant_websites
    FOR EACH ROW EXECUTE FUNCTION app.set_updated_at();

CREATE TABLE website_pages (
    id                   uuid        NOT NULL DEFAULT gen_random_uuid(),
    tenant_id            uuid        NOT NULL,
    website_id           uuid        NOT NULL,
    slug                 text        NOT NULL,
    title                text        NOT NULL,
    draft_content        jsonb       NOT NULL,
    draft_revision       integer     NOT NULL DEFAULT 1,
    draft_updated_by     uuid        REFERENCES users (id) ON DELETE SET NULL,
    published_version_id uuid,
    created_at           timestamptz NOT NULL DEFAULT now(),
    updated_at           timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (id),
    CONSTRAINT uq_website_pages_tenant_id UNIQUE (tenant_id, id),
    CONSTRAINT uq_website_pages_slug UNIQUE (tenant_id, website_id, slug),
    CONSTRAINT fk_website_pages_website FOREIGN KEY (tenant_id, website_id)
        REFERENCES tenant_websites (tenant_id, id) ON DELETE CASCADE,
    CONSTRAINT ck_website_pages_slug CHECK (slug ~ '^[a-z0-9](?:[a-z0-9-]{0,38}[a-z0-9])?$'),
    CONSTRAINT ck_website_pages_title CHECK (length(btrim(title)) BETWEEN 1 AND 120),
    CONSTRAINT ck_website_pages_draft CHECK (jsonb_typeof(draft_content) = 'object'),
    CONSTRAINT ck_website_pages_revision CHECK (draft_revision >= 1)
);
CREATE INDEX ix_website_pages_website ON website_pages (tenant_id, website_id);
CREATE TRIGGER trg_website_pages_updated_at BEFORE UPDATE ON website_pages
    FOR EACH ROW EXECUTE FUNCTION app.set_updated_at();

CREATE TABLE website_page_versions (
    id           uuid        NOT NULL DEFAULT gen_random_uuid(),
    tenant_id    uuid        NOT NULL,
    page_id      uuid        NOT NULL,
    version      integer     NOT NULL,
    content_json jsonb       NOT NULL,
    published_by uuid        REFERENCES users (id) ON DELETE SET NULL,
    created_at   timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (id),
    CONSTRAINT uq_website_page_versions_version UNIQUE (page_id, version),
    -- target composite FK pointer terbit: versi harus milik halaman + tenant yang sama
    CONSTRAINT uq_website_page_versions_owner UNIQUE (tenant_id, page_id, id),
    CONSTRAINT fk_website_page_versions_page FOREIGN KEY (tenant_id, page_id)
        REFERENCES website_pages (tenant_id, id) ON DELETE CASCADE,
    CONSTRAINT ck_website_page_versions_version CHECK (version >= 1),
    CONSTRAINT ck_website_page_versions_content CHECK (jsonb_typeof(content_json) = 'object')
);

-- Pointer versi terbit (ADR-0014 butir 3; per halaman karena versi bersifat per halaman).
-- Composite: versi lintas halaman/tenant tidak dapat ditunjuk (AC-WEB-014).
ALTER TABLE website_pages ADD CONSTRAINT fk_website_pages_published
    FOREIGN KEY (tenant_id, id, published_version_id)
    REFERENCES website_page_versions (tenant_id, page_id, id)
    DEFERRABLE INITIALLY DEFERRED;

-- ---------------------------------------------------------------- privileges
-- Default privileges (0007) memberi SELECT/INSERT/UPDATE/DELETE; persempit.
REVOKE INSERT, UPDATE, DELETE ON website_templates FROM umkm_app_role;
REVOKE UPDATE, DELETE ON website_page_versions FROM umkm_app_role; -- immutable (AC-WEB-013)

-- ---------------------------------------------------------------- RLS
ALTER TABLE tenant_websites ENABLE ROW LEVEL SECURITY;
ALTER TABLE tenant_websites FORCE ROW LEVEL SECURITY;
CREATE POLICY tenant_isolation ON tenant_websites
    USING (tenant_id = app.current_tenant_id())
    WITH CHECK (tenant_id = app.current_tenant_id());

ALTER TABLE website_pages ENABLE ROW LEVEL SECURITY;
ALTER TABLE website_pages FORCE ROW LEVEL SECURITY;
CREATE POLICY tenant_isolation ON website_pages
    USING (tenant_id = app.current_tenant_id())
    WITH CHECK (tenant_id = app.current_tenant_id());

ALTER TABLE website_page_versions ENABLE ROW LEVEL SECURITY;
ALTER TABLE website_page_versions FORCE ROW LEVEL SECURITY;
CREATE POLICY tenant_isolation ON website_page_versions
    USING (tenant_id = app.current_tenant_id())
    WITH CHECK (tenant_id = app.current_tenant_id());

-- ---------------------------------------------------------------- seed template v1
-- Konten default wajib lolos validator Rust (diuji di crates/infrastructure/tests/website.rs).
INSERT INTO website_templates (key, version, name, category, default_content) VALUES
('modern_store', 1, 'Toko Modern', 'retail', '{
  "schema_version": 1, "template": "modern_store",
  "theme": {"primary": "#2563EB", "font": "inter"},
  "sections": [
    {"id": "header-1", "type": "header", "enabled": true, "data": {"title": "Nama Toko", "nav": [{"label": "Produk", "href": "#products-1"}, {"label": "Kontak", "href": "#contact-1"}]}},
    {"id": "hero-1", "type": "hero", "enabled": true, "data": {"title": "Selamat Datang", "subtitle": "Produk pilihan untuk Anda", "cta": {"label": "Lihat Produk", "href": "#products-1"}}},
    {"id": "products-1", "type": "product_grid", "enabled": true, "data": {"title": "Produk Kami", "limit": 8}},
    {"id": "promo-1", "type": "promotion", "enabled": false, "data": {"title": "Promo Bulan Ini", "body": "Tulis promo Anda di sini"}},
    {"id": "about-1", "type": "about", "enabled": true, "data": {"title": "Tentang Kami", "body": "Ceritakan bisnis Anda di sini."}},
    {"id": "testimonials-1", "type": "testimonials", "enabled": false, "data": {"title": "Kata Pelanggan", "items": [{"name": "Pelanggan", "quote": "Pelayanannya ramah dan cepat."}]}},
    {"id": "hours-1", "type": "business_hours", "enabled": true, "data": {"title": "Jam Buka"}},
    {"id": "location-1", "type": "location", "enabled": true, "data": {"title": "Lokasi", "show_map": false}},
    {"id": "contact-1", "type": "contact", "enabled": true, "data": {"title": "Hubungi Kami", "show_phone": true, "show_email": true, "show_whatsapp": true}},
    {"id": "footer-1", "type": "footer", "enabled": true, "data": {"text": "Terima kasih telah berkunjung"}}
  ]
}'),
('warung_hangat', 1, 'Warung Hangat', 'kuliner', '{
  "schema_version": 1, "template": "warung_hangat",
  "theme": {"primary": "#B45309", "font": "nunito"},
  "sections": [
    {"id": "header-1", "type": "header", "enabled": true, "data": {"title": "Nama Warung", "nav": [{"label": "Menu", "href": "#products-1"}, {"label": "Lokasi", "href": "#location-1"}]}},
    {"id": "hero-1", "type": "hero", "enabled": true, "data": {"title": "Masakan Rumahan", "subtitle": "Hangat, segar, setiap hari", "cta": {"label": "Lihat Menu", "href": "#products-1"}}},
    {"id": "products-1", "type": "product_grid", "enabled": true, "data": {"title": "Menu", "limit": 12}},
    {"id": "hours-1", "type": "business_hours", "enabled": true, "data": {"title": "Jam Buka"}},
    {"id": "location-1", "type": "location", "enabled": true, "data": {"title": "Lokasi", "show_map": true}},
    {"id": "contact-1", "type": "contact", "enabled": true, "data": {"title": "Pesan Lewat WhatsApp", "show_whatsapp": true}},
    {"id": "footer-1", "type": "footer", "enabled": true, "data": {"text": "Sampai jumpa lagi"}}
  ]
}');
