-- P3-2: Inventory ledger + reservasi (PRD.md §4.5, §11.1, FR-INV-001..005).
-- Aturan level (stok negatif, low-stock) di Rust (crates/domain/src/inventory.rs);
-- DB menjaga invariant terakhir (CHECK) dan append-only ledger.

CREATE TABLE inventory_levels (
    tenant_id      uuid        NOT NULL REFERENCES tenants (id) ON DELETE CASCADE,
    variant_id     uuid        NOT NULL,
    on_hand        integer     NOT NULL DEFAULT 0,
    reserved       integer     NOT NULL DEFAULT 0,
    allow_negative boolean     NOT NULL DEFAULT false,
    min_stock      integer,
    updated_at     timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (tenant_id, variant_id),
    CONSTRAINT fk_inventory_levels_variant FOREIGN KEY (tenant_id, variant_id)
        REFERENCES product_variants (tenant_id, id) ON DELETE CASCADE,
    CONSTRAINT ck_inventory_levels_reserved CHECK (reserved >= 0),
    -- AC-INV-004: tanpa izin eksplisit, stok tersedia tidak pernah negatif (pertahanan terakhir).
    CONSTRAINT ck_inventory_levels_available CHECK (allow_negative OR on_hand - reserved >= 0),
    CONSTRAINT ck_inventory_levels_min_stock CHECK (min_stock IS NULL OR min_stock BETWEEN 0 AND 1000000)
);

-- Setiap varian punya level (stok awal 0).
CREATE FUNCTION app.create_inventory_level() RETURNS trigger
LANGUAGE plpgsql AS $$
BEGIN
    INSERT INTO inventory_levels (tenant_id, variant_id) VALUES (NEW.tenant_id, NEW.id);
    RETURN NEW;
END $$;
CREATE TRIGGER trg_product_variants_level AFTER INSERT ON product_variants
    FOR EACH ROW EXECUTE FUNCTION app.create_inventory_level();
INSERT INTO inventory_levels (tenant_id, variant_id) SELECT tenant_id, id FROM product_variants;

CREATE TABLE inventory_movements (
    id            uuid        NOT NULL DEFAULT gen_random_uuid(),
    tenant_id     uuid        NOT NULL REFERENCES tenants (id) ON DELETE CASCADE,
    variant_id    uuid        NOT NULL,
    kind          text        NOT NULL,
    delta         integer     NOT NULL,
    on_hand_after integer     NOT NULL,
    reason        text        NOT NULL,
    reference     text,
    actor_user_id uuid,
    created_at    timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (id),
    -- Varian yang punya riwayat stok tidak dapat dihapus (catalog: 409 variant_in_use).
    CONSTRAINT fk_inventory_movements_variant FOREIGN KEY (tenant_id, variant_id)
        REFERENCES product_variants (tenant_id, id) ON DELETE RESTRICT,
    CONSTRAINT ck_inventory_movements_kind CHECK (kind IN ('stock_in', 'stock_out', 'adjustment', 'sale')),
    CONSTRAINT ck_inventory_movements_delta CHECK (delta <> 0),
    CONSTRAINT ck_inventory_movements_reason CHECK (length(btrim(reason)) BETWEEN 1 AND 200),
    CONSTRAINT ck_inventory_movements_reference CHECK (reference IS NULL OR length(reference) <= 100)
);
CREATE INDEX ix_inventory_movements_variant ON inventory_movements (tenant_id, variant_id, created_at DESC);

CREATE TABLE inventory_reservations (
    id          uuid        NOT NULL DEFAULT gen_random_uuid(),
    tenant_id   uuid        NOT NULL REFERENCES tenants (id) ON DELETE CASCADE,
    variant_id  uuid        NOT NULL,
    quantity    integer     NOT NULL,
    reference   text        NOT NULL,
    status      text        NOT NULL DEFAULT 'active',
    expires_at  timestamptz NOT NULL,
    created_at  timestamptz NOT NULL DEFAULT now(),
    resolved_at timestamptz,
    PRIMARY KEY (id),
    CONSTRAINT uq_inventory_reservations_tenant_id UNIQUE (tenant_id, id),
    CONSTRAINT fk_inventory_reservations_variant FOREIGN KEY (tenant_id, variant_id)
        REFERENCES product_variants (tenant_id, id) ON DELETE RESTRICT,
    CONSTRAINT ck_inventory_reservations_quantity CHECK (quantity BETWEEN 1 AND 1000000),
    CONSTRAINT ck_inventory_reservations_status CHECK (status IN ('active', 'committed', 'released', 'expired')),
    CONSTRAINT ck_inventory_reservations_resolved CHECK ((status = 'active') = (resolved_at IS NULL)),
    CONSTRAINT ck_inventory_reservations_reference CHECK (length(reference) BETWEEN 1 AND 100)
);
CREATE INDEX ix_inventory_reservations_expiry ON inventory_reservations (expires_at) WHERE status = 'active';
CREATE INDEX ix_inventory_reservations_reference ON inventory_reservations (tenant_id, reference);

DO $$
DECLARE t text;
BEGIN
    FOREACH t IN ARRAY ARRAY['inventory_levels', 'inventory_movements', 'inventory_reservations'] LOOP
        EXECUTE format('ALTER TABLE %I ENABLE ROW LEVEL SECURITY', t);
        EXECUTE format('ALTER TABLE %I FORCE ROW LEVEL SECURITY', t);
        EXECUTE format('CREATE POLICY tenant_isolation ON %I USING (tenant_id = app.current_tenant_id()) WITH CHECK (tenant_id = app.current_tenant_id())', t);
    END LOOP;
END $$;

-- AC-INV-001: movement tidak dapat diubah/dihapus aplikasi. Level tidak dihapus aplikasi.
REVOKE UPDATE, DELETE ON inventory_movements FROM umkm_app_role;
REVOKE DELETE ON inventory_levels, inventory_reservations FROM umkm_app_role;

-- AC-INV-003: reservasi aktif yang lewat expires_at dilepas (lintas tenant, dipanggil worker).
-- SKIP LOCKED: reservasi yang sedang diproses checkout/konfirmasi tidak ditunggu.
CREATE FUNCTION app.expire_reservations(p_batch integer) RETURNS integer
    LANGUAGE plpgsql SECURITY DEFINER
    SET search_path = pg_catalog, public
AS $$
DECLARE r record; n integer := 0;
BEGIN
    FOR r IN
        SELECT id, tenant_id, variant_id, quantity FROM public.inventory_reservations
        WHERE status = 'active' AND expires_at <= now()
        ORDER BY expires_at
        LIMIT greatest(1, least(p_batch, 1000))
        FOR UPDATE SKIP LOCKED
    LOOP
        UPDATE public.inventory_levels SET reserved = reserved - r.quantity, updated_at = now()
            WHERE tenant_id = r.tenant_id AND variant_id = r.variant_id;
        UPDATE public.inventory_reservations SET status = 'expired', resolved_at = now() WHERE id = r.id;
        n := n + 1;
    END LOOP;
    RETURN n;
END $$;
REVOKE ALL ON FUNCTION app.expire_reservations(integer) FROM PUBLIC;
GRANT EXECUTE ON FUNCTION app.expire_reservations(integer) TO umkm_app_role;
