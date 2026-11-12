-- P3-1: Katalog produk (PRD.md §4.5, FR-CAT-001..004). Validasi lengkap di Rust
-- (crates/domain/src/catalog.rs); DB menjaga kepemilikan tenant, keunikan, dan batas angka.

CREATE TABLE product_categories (
    id         uuid        NOT NULL DEFAULT gen_random_uuid(),
    tenant_id  uuid        NOT NULL REFERENCES tenants (id) ON DELETE CASCADE,
    parent_id  uuid,
    name       text        NOT NULL,
    position   integer     NOT NULL DEFAULT 0,
    created_at timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (id),
    CONSTRAINT uq_product_categories_tenant_id UNIQUE (tenant_id, id),
    -- AC-CAT-002: parent wajib milik tenant yang sama; induk berisi subkategori tidak dapat dihapus.
    CONSTRAINT fk_product_categories_parent FOREIGN KEY (tenant_id, parent_id)
        REFERENCES product_categories (tenant_id, id) ON DELETE RESTRICT,
    CONSTRAINT ck_product_categories_name CHECK (length(btrim(name)) BETWEEN 1 AND 60),
    CONSTRAINT ck_product_categories_not_self CHECK (parent_id IS DISTINCT FROM id)
);
CREATE UNIQUE INDEX uq_product_categories_name ON product_categories
    (tenant_id, COALESCE(parent_id, '00000000-0000-0000-0000-000000000000'::uuid), lower(name));

-- Hierarki maksimal 2 tingkat (kategori + subkategori, PRD.md §4.5) → siklus mustahil.
-- Advisory lock per tenant menserialkan perubahan hierarki agar dua UPDATE paralel
-- (A→B dan B→A) tidak sama-sama lolos pemeriksaan.
CREATE FUNCTION app.check_category_depth() RETURNS trigger
LANGUAGE plpgsql AS $$
BEGIN
    IF NEW.parent_id IS NULL AND TG_OP = 'INSERT' THEN
        RETURN NEW;
    END IF;
    PERFORM pg_advisory_xact_lock(hashtextextended('product_categories:' || NEW.tenant_id::text, 0));
    IF NEW.parent_id IS NOT NULL THEN
        IF EXISTS (SELECT 1 FROM product_categories
                   WHERE tenant_id = NEW.tenant_id AND id = NEW.parent_id AND parent_id IS NOT NULL) THEN
            RAISE EXCEPTION 'kategori induk sudah merupakan subkategori'
                USING ERRCODE = 'check_violation', CONSTRAINT = 'ck_product_categories_depth';
        END IF;
        IF EXISTS (SELECT 1 FROM product_categories
                   WHERE tenant_id = NEW.tenant_id AND parent_id = NEW.id) THEN
            RAISE EXCEPTION 'kategori yang memiliki subkategori tidak dapat dijadikan subkategori'
                USING ERRCODE = 'check_violation', CONSTRAINT = 'ck_product_categories_depth';
        END IF;
    END IF;
    RETURN NEW;
END $$;

CREATE TRIGGER trg_product_categories_depth
    BEFORE INSERT OR UPDATE OF parent_id ON product_categories
    FOR EACH ROW EXECUTE FUNCTION app.check_category_depth();

CREATE TABLE products (
    id          uuid        NOT NULL DEFAULT gen_random_uuid(),
    tenant_id   uuid        NOT NULL REFERENCES tenants (id) ON DELETE CASCADE,
    category_id uuid,
    name        text        NOT NULL,
    slug        text        NOT NULL,
    description text,
    status      text        NOT NULL DEFAULT 'draft',
    available   boolean     NOT NULL DEFAULT true,
    -- [{name, values[]}], divalidasi Rust (maks 3 opsi × 20 nilai)
    options     jsonb       NOT NULL DEFAULT '[]',
    created_at  timestamptz NOT NULL DEFAULT now(),
    updated_at  timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (id),
    CONSTRAINT uq_products_tenant_id UNIQUE (tenant_id, id),
    CONSTRAINT uq_products_slug UNIQUE (tenant_id, slug),
    -- Menghapus kategori hanya melepas produk dari kategori itu.
    CONSTRAINT fk_products_category FOREIGN KEY (tenant_id, category_id)
        REFERENCES product_categories (tenant_id, id) ON DELETE SET NULL (category_id),
    CONSTRAINT ck_products_status CHECK (status IN ('draft', 'active', 'archived')),
    CONSTRAINT ck_products_name CHECK (length(btrim(name)) BETWEEN 1 AND 120),
    CONSTRAINT ck_products_slug CHECK (slug ~ '^[a-z0-9]+(-[a-z0-9]+)*$' AND length(slug) <= 80),
    CONSTRAINT ck_products_description CHECK (description IS NULL OR length(description) <= 5000),
    CONSTRAINT ck_products_options CHECK (jsonb_typeof(options) = 'array' AND jsonb_array_length(options) <= 3)
);
CREATE INDEX ix_products_tenant_status ON products (tenant_id, status, created_at DESC);

CREATE TABLE product_variants (
    id            uuid          NOT NULL DEFAULT gen_random_uuid(),
    tenant_id     uuid          NOT NULL REFERENCES tenants (id) ON DELETE CASCADE,
    product_id    uuid          NOT NULL,
    sku           text          NOT NULL,
    name          text          NOT NULL,
    option_values jsonb         NOT NULL DEFAULT '[]',
    price         numeric(14,2) NOT NULL,
    promo_price   numeric(14,2),
    is_active     boolean       NOT NULL DEFAULT true,
    position      smallint      NOT NULL,
    created_at    timestamptz   NOT NULL DEFAULT now(),
    PRIMARY KEY (id),
    CONSTRAINT uq_product_variants_tenant_id UNIQUE (tenant_id, id),
    -- AC-CAT-003: SKU unik per tenant (Rust menormalkan huruf besar).
    CONSTRAINT uq_product_variants_sku UNIQUE (tenant_id, sku) DEFERRABLE INITIALLY DEFERRED,
    CONSTRAINT uq_product_variants_position UNIQUE (product_id, position) DEFERRABLE INITIALLY DEFERRED,
    CONSTRAINT fk_product_variants_product FOREIGN KEY (tenant_id, product_id)
        REFERENCES products (tenant_id, id) ON DELETE CASCADE,
    CONSTRAINT ck_product_variants_sku CHECK (sku ~ '^[A-Z0-9._-]{1,64}$'),
    CONSTRAINT ck_product_variants_name CHECK (length(name) BETWEEN 1 AND 200),
    CONSTRAINT ck_product_variants_option_values CHECK (jsonb_typeof(option_values) = 'array'),
    -- AC-CAT-004: harga tidak negatif, promo di bawah harga normal.
    CONSTRAINT ck_product_variants_price CHECK (price BETWEEN 0 AND 1000000000),
    CONSTRAINT ck_product_variants_promo CHECK (promo_price IS NULL OR (promo_price >= 0 AND promo_price < price)),
    CONSTRAINT ck_product_variants_position CHECK (position BETWEEN 0 AND 99)
);
CREATE INDEX ix_product_variants_product ON product_variants (tenant_id, product_id, position);

CREATE TABLE product_images (
    tenant_id  uuid     NOT NULL REFERENCES tenants (id) ON DELETE CASCADE,
    product_id uuid     NOT NULL,
    position   smallint NOT NULL,
    object_id  uuid     NOT NULL,
    PRIMARY KEY (tenant_id, product_id, position),
    CONSTRAINT fk_product_images_product FOREIGN KEY (tenant_id, product_id)
        REFERENCES products (tenant_id, id) ON DELETE CASCADE,
    -- Foto wajib objek storage milik tenant yang sama; objek dipakai tidak bisa dihapus (ADR-0009).
    CONSTRAINT fk_product_images_object FOREIGN KEY (tenant_id, object_id)
        REFERENCES stored_objects (tenant_id, id) ON DELETE RESTRICT,
    CONSTRAINT uq_product_images_object UNIQUE (tenant_id, product_id, object_id),
    CONSTRAINT ck_product_images_position CHECK (position BETWEEN 0 AND 9)
);

DO $$
DECLARE t text;
BEGIN
    FOREACH t IN ARRAY ARRAY['product_categories', 'products', 'product_variants', 'product_images'] LOOP
        EXECUTE format('ALTER TABLE %I ENABLE ROW LEVEL SECURITY', t);
        EXECUTE format('ALTER TABLE %I FORCE ROW LEVEL SECURITY', t);
        EXECUTE format('CREATE POLICY tenant_isolation ON %I USING (tenant_id = app.current_tenant_id()) WITH CHECK (tenant_id = app.current_tenant_id())', t);
    END LOOP;
END $$;
