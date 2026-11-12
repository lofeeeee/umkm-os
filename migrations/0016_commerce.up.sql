-- P3-4: cart → order, snapshot item, event, payment QRIS statis, refund (PRD.md §4.7, ADR-0004).
-- Harga/total dihitung server (crates/domain/src/commerce/pricing.rs); state machine di
-- crates/domain/src/commerce/state.rs, dicerminkan trigger di bawah (pertahanan kedua).

-- Pengaturan toko untuk checkout. QRIS hanya dapat diganti `tenant.update` (diaudit).
CREATE TABLE commerce_settings (
    tenant_id               uuid          PRIMARY KEY REFERENCES tenants (id) ON DELETE CASCADE,
    qris_object_id          uuid,
    qris_receiver_name      text,
    checkout_enabled        boolean       NOT NULL DEFAULT false,
    delivery_fee            numeric(14,2) NOT NULL DEFAULT 0,
    order_approval_required boolean       NOT NULL DEFAULT false,
    reservation_ttl_minutes integer       NOT NULL DEFAULT 60,
    updated_at              timestamptz   NOT NULL DEFAULT now(),
    CONSTRAINT fk_commerce_settings_qris FOREIGN KEY (tenant_id, qris_object_id)
        REFERENCES stored_objects (tenant_id, id) ON DELETE RESTRICT,
    CONSTRAINT ck_commerce_settings_receiver CHECK (qris_receiver_name IS NULL OR length(btrim(qris_receiver_name)) BETWEEN 1 AND 80),
    -- Checkout hanya bisa aktif bila QRIS diatur (409 PAYMENT_NOT_CONFIGURED di API).
    CONSTRAINT ck_commerce_settings_enabled CHECK (NOT checkout_enabled OR (qris_object_id IS NOT NULL AND qris_receiver_name IS NOT NULL)),
    CONSTRAINT ck_commerce_settings_fee CHECK (delivery_fee BETWEEN 0 AND 10000000),
    CONSTRAINT ck_commerce_settings_ttl CHECK (reservation_ttl_minutes BETWEEN 15 AND 1440)
);

CREATE TABLE orders (
    id                uuid          NOT NULL DEFAULT gen_random_uuid(),
    tenant_id         uuid          NOT NULL REFERENCES tenants (id) ON DELETE CASCADE,
    code              text          NOT NULL,
    customer_id       uuid          NOT NULL,
    status            text          NOT NULL,
    fulfillment       text          NOT NULL,
    -- Snapshot kontak saat checkout; dikosongkan saat customer dianonimkan (ADR-0005).
    customer_name     text,
    customer_email    text,
    customer_phone    text,
    delivery_address  text,
    customer_note     text,
    subtotal          numeric(14,2) NOT NULL,
    delivery_fee      numeric(14,2) NOT NULL,
    total             numeric(14,2) NOT NULL,
    currency          text          NOT NULL DEFAULT 'IDR',
    access_token_hash bytea         NOT NULL,
    idempotency_key   text          NOT NULL,
    request_hash      bytea         NOT NULL,
    -- D-12: versi privacy/terms transaksi + waktu persetujuan (AC-ORD-010).
    privacy_version   text          NOT NULL,
    terms_version     text          NOT NULL,
    consented_at      timestamptz   NOT NULL,
    expires_at        timestamptz   NOT NULL,
    created_at        timestamptz   NOT NULL DEFAULT now(),
    updated_at        timestamptz   NOT NULL DEFAULT now(),
    PRIMARY KEY (id),
    CONSTRAINT uq_orders_tenant_id UNIQUE (tenant_id, id),
    CONSTRAINT uq_orders_code UNIQUE (tenant_id, code),
    -- AC-ORD-006: kunci idempotensi unik per tenant.
    CONSTRAINT uq_orders_idempotency UNIQUE (tenant_id, idempotency_key),
    CONSTRAINT fk_orders_customer FOREIGN KEY (tenant_id, customer_id)
        REFERENCES customers (tenant_id, id) ON DELETE RESTRICT,
    CONSTRAINT ck_orders_status CHECK (status IN ('PENDING', 'AWAITING_PAYMENT', 'PAID', 'APPROVAL_PENDING', 'APPROVED',
        'PROCESSING', 'READY_FOR_PICKUP', 'SHIPPED', 'COMPLETED', 'REJECTED', 'CANCELLED', 'EXPIRED', 'REFUNDED')),
    CONSTRAINT ck_orders_fulfillment CHECK (fulfillment IN ('pickup', 'delivery')),
    CONSTRAINT ck_orders_address CHECK ((fulfillment = 'delivery') = (delivery_address IS NOT NULL) OR customer_name IS NULL),
    CONSTRAINT ck_orders_note CHECK (customer_note IS NULL OR length(customer_note) <= 500),
    CONSTRAINT ck_orders_amounts CHECK (subtotal >= 0 AND delivery_fee >= 0 AND total = subtotal + delivery_fee),
    CONSTRAINT ck_orders_currency CHECK (currency = 'IDR'),
    CONSTRAINT ck_orders_token CHECK (length(access_token_hash) = 32 AND length(request_hash) = 32),
    CONSTRAINT ck_orders_idempotency_key CHECK (length(idempotency_key) BETWEEN 8 AND 100),
    CONSTRAINT ck_orders_code CHECK (code ~ '^[A-Z0-9-]{6,20}$')
);
CREATE UNIQUE INDEX uq_orders_access_token ON orders (access_token_hash);
CREATE INDEX ix_orders_tenant_status ON orders (tenant_id, status, created_at DESC);
CREATE INDEX ix_orders_customer ON orders (tenant_id, customer_id, created_at DESC);

-- AC-ORD-003: snapshot immutable (tidak ikut berubah saat katalog diubah).
CREATE TABLE order_items (
    id           uuid          NOT NULL DEFAULT gen_random_uuid(),
    tenant_id    uuid          NOT NULL REFERENCES tenants (id) ON DELETE CASCADE,
    order_id     uuid          NOT NULL,
    position     smallint      NOT NULL,
    product_id   uuid,
    variant_id   uuid,
    product_name text          NOT NULL,
    variant_name text          NOT NULL,
    sku          text          NOT NULL,
    unit_price   numeric(14,2) NOT NULL,
    quantity     integer       NOT NULL,
    line_total   numeric(14,2) NOT NULL,
    PRIMARY KEY (id),
    CONSTRAINT uq_order_items_position UNIQUE (tenant_id, order_id, position),
    CONSTRAINT fk_order_items_order FOREIGN KEY (tenant_id, order_id)
        REFERENCES orders (tenant_id, id) ON DELETE CASCADE,
    -- Varian dengan riwayat order tidak dapat dihapus dari katalog (409 variant_in_use).
    CONSTRAINT fk_order_items_variant FOREIGN KEY (tenant_id, variant_id)
        REFERENCES product_variants (tenant_id, id) ON DELETE RESTRICT,
    CONSTRAINT ck_order_items_quantity CHECK (quantity BETWEEN 1 AND 999),
    CONSTRAINT ck_order_items_amounts CHECK (unit_price >= 0 AND line_total = unit_price * quantity)
);

CREATE TABLE order_events (
    id            uuid        NOT NULL DEFAULT gen_random_uuid(),
    tenant_id     uuid        NOT NULL REFERENCES tenants (id) ON DELETE CASCADE,
    order_id      uuid        NOT NULL,
    from_status   text,
    to_status     text        NOT NULL,
    actor_type    text        NOT NULL,
    actor_user_id uuid,
    reason        text,
    created_at    timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (id),
    CONSTRAINT fk_order_events_order FOREIGN KEY (tenant_id, order_id)
        REFERENCES orders (tenant_id, id) ON DELETE CASCADE,
    CONSTRAINT ck_order_events_actor CHECK (actor_type IN ('staff', 'customer', 'system')
        AND (actor_type <> 'staff' OR actor_user_id IS NOT NULL)),
    CONSTRAINT ck_order_events_reason CHECK (reason IS NULL OR length(reason) <= 500)
);
CREATE INDEX ix_order_events_order ON order_events (tenant_id, order_id, created_at);

CREATE TABLE payments (
    id               uuid          NOT NULL DEFAULT gen_random_uuid(),
    tenant_id        uuid          NOT NULL REFERENCES tenants (id) ON DELETE CASCADE,
    order_id         uuid          NOT NULL,
    method           text          NOT NULL DEFAULT 'qris_static',
    status           text          NOT NULL DEFAULT 'pending',
    amount           numeric(14,2) NOT NULL,
    amount_received  numeric(14,2),
    payer_reference  text,
    proof_object_id  uuid,
    verifier_note    text,
    rejection_reason text,
    claimed_at       timestamptz,
    decided_at       timestamptz,
    decided_by       uuid,
    created_at       timestamptz   NOT NULL DEFAULT now(),
    PRIMARY KEY (id),
    CONSTRAINT uq_payments_tenant_id UNIQUE (tenant_id, id),
    CONSTRAINT uq_payments_order UNIQUE (tenant_id, order_id),
    CONSTRAINT fk_payments_order FOREIGN KEY (tenant_id, order_id)
        REFERENCES orders (tenant_id, id) ON DELETE CASCADE,
    CONSTRAINT fk_payments_proof FOREIGN KEY (tenant_id, proof_object_id)
        REFERENCES stored_objects (tenant_id, id) ON DELETE SET NULL (proof_object_id),
    CONSTRAINT ck_payments_method CHECK (method IN ('qris_static')),
    CONSTRAINT ck_payments_status CHECK (status IN ('pending', 'claimed', 'confirmed', 'rejected', 'expired')),
    CONSTRAINT ck_payments_amounts CHECK (amount > 0 AND (amount_received IS NULL OR amount_received > 0)),
    CONSTRAINT ck_payments_confirmed CHECK ((status = 'confirmed') = (amount_received IS NOT NULL)),
    CONSTRAINT ck_payments_decided CHECK ((status IN ('confirmed', 'rejected')) = (decided_by IS NOT NULL)),
    CONSTRAINT ck_payments_rejection CHECK ((status = 'rejected') = (rejection_reason IS NOT NULL)),
    CONSTRAINT ck_payments_texts CHECK ((payer_reference IS NULL OR length(payer_reference) <= 120)
        AND (verifier_note IS NULL OR length(verifier_note) <= 500)
        AND (rejection_reason IS NULL OR length(btrim(rejection_reason)) BETWEEN 1 AND 500))
);

CREATE TABLE refunds (
    id            uuid          NOT NULL DEFAULT gen_random_uuid(),
    tenant_id     uuid          NOT NULL REFERENCES tenants (id) ON DELETE CASCADE,
    payment_id    uuid          NOT NULL,
    amount        numeric(14,2) NOT NULL,
    reason        text          NOT NULL,
    actor_user_id uuid          NOT NULL,
    created_at    timestamptz   NOT NULL DEFAULT now(),
    PRIMARY KEY (id),
    CONSTRAINT fk_refunds_payment FOREIGN KEY (tenant_id, payment_id)
        REFERENCES payments (tenant_id, id) ON DELETE CASCADE,
    CONSTRAINT ck_refunds_amount CHECK (amount > 0),
    CONSTRAINT ck_refunds_reason CHECK (length(btrim(reason)) BETWEEN 1 AND 500)
);

-- Cermin OrderStatus::can_transition (diuji paritas 13 × 13 × 2 di tests).
CREATE FUNCTION app.order_transition_allowed(p_from text, p_to text, p_fulfillment text) RETURNS boolean
    LANGUAGE sql IMMUTABLE AS $$
    SELECT CASE
        WHEN p_from = 'PENDING' THEN p_to IN ('AWAITING_PAYMENT', 'CANCELLED')
        WHEN p_from = 'AWAITING_PAYMENT' THEN p_to IN ('PAID', 'CANCELLED', 'EXPIRED')
        WHEN p_from = 'PAID' THEN p_to IN ('PROCESSING', 'APPROVAL_PENDING', 'REFUNDED')
        WHEN p_from = 'APPROVAL_PENDING' THEN p_to IN ('APPROVED', 'REJECTED')
        WHEN p_from = 'APPROVED' THEN p_to IN ('PROCESSING', 'REFUNDED')
        WHEN p_from = 'PROCESSING' THEN p_to = 'REFUNDED'
            OR (p_to = 'READY_FOR_PICKUP' AND p_fulfillment = 'pickup')
            OR (p_to = 'SHIPPED' AND p_fulfillment = 'delivery')
        WHEN p_from IN ('READY_FOR_PICKUP', 'SHIPPED') THEN p_to IN ('COMPLETED', 'REFUNDED')
        WHEN p_from IN ('COMPLETED', 'REJECTED') THEN p_to = 'REFUNDED'
        ELSE false
    END
$$;

CREATE FUNCTION app.check_order_transition() RETURNS trigger
LANGUAGE plpgsql AS $$
BEGIN
    IF NEW.status IS DISTINCT FROM OLD.status
       AND NOT app.order_transition_allowed(OLD.status, NEW.status, OLD.fulfillment) THEN
        RAISE EXCEPTION 'transisi order % -> % tidak diizinkan', OLD.status, NEW.status
            USING ERRCODE = 'check_violation', CONSTRAINT = 'ck_orders_transition';
    END IF;
    IF NEW.fulfillment IS DISTINCT FROM OLD.fulfillment OR NEW.total IS DISTINCT FROM OLD.total
       OR NEW.customer_id IS DISTINCT FROM OLD.customer_id THEN
        RAISE EXCEPTION 'fulfillment/total/customer order tidak dapat diubah'
            USING ERRCODE = 'check_violation', CONSTRAINT = 'ck_orders_immutable';
    END IF;
    NEW.updated_at := now();
    RETURN NEW;
END $$;
CREATE TRIGGER trg_orders_transition BEFORE UPDATE ON orders
    FOR EACH ROW EXECUTE FUNCTION app.check_order_transition();
-- Order baru hanya boleh mulai dari status awal yang sah.
CREATE FUNCTION app.check_order_initial() RETURNS trigger
LANGUAGE plpgsql AS $$
BEGIN
    IF NEW.status NOT IN ('PENDING', 'AWAITING_PAYMENT') THEN
        RAISE EXCEPTION 'order baru harus PENDING/AWAITING_PAYMENT'
            USING ERRCODE = 'check_violation', CONSTRAINT = 'ck_orders_initial';
    END IF;
    RETURN NEW;
END $$;
CREATE TRIGGER trg_orders_initial BEFORE INSERT ON orders
    FOR EACH ROW EXECUTE FUNCTION app.check_order_initial();

DO $$
DECLARE t text;
BEGIN
    FOREACH t IN ARRAY ARRAY['commerce_settings', 'orders', 'order_items', 'order_events', 'payments', 'refunds'] LOOP
        EXECUTE format('ALTER TABLE %I ENABLE ROW LEVEL SECURITY', t);
        EXECUTE format('ALTER TABLE %I FORCE ROW LEVEL SECURITY', t);
        EXECUTE format('CREATE POLICY tenant_isolation ON %I USING (tenant_id = app.current_tenant_id()) WITH CHECK (tenant_id = app.current_tenant_id())', t);
    END LOOP;
END $$;

-- Snapshot, event, refund: append-only bagi aplikasi. Order/payment tidak dihapus aplikasi.
REVOKE UPDATE, DELETE ON order_items, order_events, refunds FROM umkm_app_role;
REVOKE DELETE ON orders, payments FROM umkm_app_role;

-- Anonimisasi customer (ADR-0005/0012) kini juga mengosongkan snapshot kontak order.
CREATE OR REPLACE FUNCTION app.purge_customers(p_grace interval) RETURNS bigint
    LANGUAGE plpgsql SECURITY DEFINER
    SET search_path = pg_catalog, public
AS $$
DECLARE n bigint;
BEGIN
    IF p_grace < interval '30 days' THEN
        RAISE EXCEPTION 'purge_customers: grace % < 30 hari', p_grace USING ERRCODE = 'invalid_parameter_value';
    END IF;
    WITH due AS (
        SELECT tenant_id, id, account_id FROM public.customers
        WHERE deletion_requested_at < now() - p_grace AND anonymized_at IS NULL
        FOR UPDATE
    ), ord AS (
        UPDATE public.orders o SET customer_name = NULL, customer_email = NULL, customer_phone = NULL,
            delivery_address = NULL, customer_note = NULL
        FROM due WHERE o.tenant_id = due.tenant_id AND o.customer_id = due.id
    ), addr AS (
        DELETE FROM public.customer_addresses a USING due
        WHERE a.tenant_id = due.tenant_id AND a.customer_id = due.id
    ), acc AS (
        DELETE FROM public.customer_accounts c USING due
        WHERE c.tenant_id = due.tenant_id AND c.id = due.account_id
    )
    UPDATE public.customers c SET name = 'Pelanggan dihapus', email = NULL, phone = NULL,
        tags = '{}', internal_note = NULL, account_id = NULL, anonymized_at = now(), updated_at = now()
    FROM due WHERE c.tenant_id = due.tenant_id AND c.id = due.id;
    GET DIAGNOSTICS n = ROW_COUNT;
    RETURN n;
END $$;
