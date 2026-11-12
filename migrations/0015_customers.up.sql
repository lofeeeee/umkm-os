-- P3-3: Customer & CRM per tenant (PRD.md §4.6, ADR-0005: guest + akun opsional, linking setelah
-- verifikasi). Customer BUKAN users platform: tabel, sesi, dan cookie terpisah.

-- Permission baru (product-decision-log 2026-11-12, Gate D): 33 → 37.
INSERT INTO permissions (permission_key)
VALUES ('customers.read'), ('customers.manage'), ('payments.verify'), ('payments.refund');

INSERT INTO role_permissions (role_id, permission_id)
SELECT r.id, p.id
FROM (VALUES
    ('Owner', 'customers.read'), ('Owner', 'customers.manage'), ('Owner', 'payments.verify'), ('Owner', 'payments.refund'),
    ('Manager', 'customers.read'), ('Manager', 'customers.manage'), ('Manager', 'payments.verify'),
    ('Support Agent', 'customers.read'),
    ('Viewer', 'customers.read')
) AS m(role_name, permission_key)
JOIN roles r ON r.tenant_id IS NULL AND r.name = m.role_name
JOIN permissions p ON p.permission_key = m.permission_key;

-- Akun customer opsional, per tenant (email unik per tenant, bukan global).
CREATE TABLE customer_accounts (
    id                uuid        NOT NULL DEFAULT gen_random_uuid(),
    tenant_id         uuid        NOT NULL REFERENCES tenants (id) ON DELETE CASCADE,
    email             text        NOT NULL,
    password_hash     text        NOT NULL,
    email_verified_at timestamptz,
    -- D-12: versi privacy/terms yang disetujui saat daftar (teks placeholder sampai go-live).
    privacy_version   text        NOT NULL,
    terms_version     text        NOT NULL,
    consented_at      timestamptz NOT NULL DEFAULT now(),
    created_at        timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (id),
    CONSTRAINT uq_customer_accounts_tenant_id UNIQUE (tenant_id, id),
    CONSTRAINT ck_customer_accounts_email CHECK (length(email) BETWEEN 3 AND 254 AND email LIKE '%_@_%' AND email = lower(btrim(email))),
    CONSTRAINT ck_customer_accounts_password_hash CHECK (password_hash LIKE '$argon2id$%')
);
CREATE UNIQUE INDEX uq_customer_accounts_email ON customer_accounts (tenant_id, email);

CREATE TABLE customers (
    id                    uuid        NOT NULL DEFAULT gen_random_uuid(),
    tenant_id             uuid        NOT NULL REFERENCES tenants (id) ON DELETE CASCADE,
    account_id            uuid,
    name                  text        NOT NULL,
    email                 text,
    phone                 text,
    tags                  text[]      NOT NULL DEFAULT '{}',
    internal_note         text,
    created_at            timestamptz NOT NULL DEFAULT now(),
    updated_at            timestamptz NOT NULL DEFAULT now(),
    deletion_requested_at timestamptz,
    anonymized_at         timestamptz,
    PRIMARY KEY (id),
    CONSTRAINT uq_customers_tenant_id UNIQUE (tenant_id, id),
    CONSTRAINT fk_customers_account FOREIGN KEY (tenant_id, account_id)
        REFERENCES customer_accounts (tenant_id, id) ON DELETE SET NULL (account_id),
    CONSTRAINT ck_customers_name CHECK (length(btrim(name)) BETWEEN 1 AND 120),
    CONSTRAINT ck_customers_email CHECK (email IS NULL OR (length(email) <= 254 AND email = lower(btrim(email)))),
    CONSTRAINT ck_customers_phone CHECK (phone IS NULL OR phone ~ '^\+[1-9][0-9]{6,14}$'),
    CONSTRAINT ck_customers_tags CHECK (cardinality(tags) <= 20),
    CONSTRAINT ck_customers_note CHECK (internal_note IS NULL OR length(internal_note) <= 2000),
    -- Kontak wajib selagi belum dianonimkan (dipakai untuk konfirmasi order).
    CONSTRAINT ck_customers_contact CHECK (anonymized_at IS NOT NULL OR email IS NOT NULL OR phone IS NOT NULL)
);
CREATE INDEX ix_customers_email ON customers (tenant_id, email);
CREATE INDEX ix_customers_account ON customers (tenant_id, account_id);
CREATE INDEX ix_customers_created ON customers (tenant_id, created_at DESC);

CREATE TABLE customer_addresses (
    id          uuid     NOT NULL DEFAULT gen_random_uuid(),
    tenant_id   uuid     NOT NULL REFERENCES tenants (id) ON DELETE CASCADE,
    customer_id uuid     NOT NULL,
    position    smallint NOT NULL,
    label       text,
    address     text     NOT NULL,
    note        text,
    PRIMARY KEY (id),
    CONSTRAINT fk_customer_addresses_customer FOREIGN KEY (tenant_id, customer_id)
        REFERENCES customers (tenant_id, id) ON DELETE CASCADE,
    CONSTRAINT uq_customer_addresses_position UNIQUE (tenant_id, customer_id, position),
    CONSTRAINT ck_customer_addresses_position CHECK (position BETWEEN 0 AND 4),
    CONSTRAINT ck_customer_addresses_address CHECK (length(btrim(address)) BETWEEN 1 AND 500),
    CONSTRAINT ck_customer_addresses_label CHECK (label IS NULL OR length(label) <= 60),
    CONSTRAINT ck_customer_addresses_note CHECK (note IS NULL OR length(note) <= 200)
);

-- Sesi customer: hash token saja; cookie host-only di subdomain toko (ADR-0005).
CREATE TABLE customer_sessions (
    id         uuid        NOT NULL DEFAULT gen_random_uuid(),
    tenant_id  uuid        NOT NULL REFERENCES tenants (id) ON DELETE CASCADE,
    account_id uuid        NOT NULL,
    token_hash bytea       NOT NULL,
    created_at timestamptz NOT NULL DEFAULT now(),
    expires_at timestamptz NOT NULL,
    revoked_at timestamptz,
    PRIMARY KEY (id),
    CONSTRAINT fk_customer_sessions_account FOREIGN KEY (tenant_id, account_id)
        REFERENCES customer_accounts (tenant_id, id) ON DELETE CASCADE,
    CONSTRAINT ck_customer_sessions_hash CHECK (length(token_hash) = 32),
    CONSTRAINT ck_customer_sessions_expiry CHECK (expires_at > created_at)
);
CREATE UNIQUE INDEX uq_customer_sessions_hash ON customer_sessions (token_hash);

CREATE TABLE customer_tokens (
    id          uuid        NOT NULL DEFAULT gen_random_uuid(),
    tenant_id   uuid        NOT NULL REFERENCES tenants (id) ON DELETE CASCADE,
    account_id  uuid        NOT NULL,
    purpose     text        NOT NULL,
    token_hash  bytea       NOT NULL,
    expires_at  timestamptz NOT NULL,
    consumed_at timestamptz,
    created_at  timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (id),
    CONSTRAINT fk_customer_tokens_account FOREIGN KEY (tenant_id, account_id)
        REFERENCES customer_accounts (tenant_id, id) ON DELETE CASCADE,
    CONSTRAINT ck_customer_tokens_purpose CHECK (purpose IN ('email_verification')),
    CONSTRAINT ck_customer_tokens_hash CHECK (length(token_hash) = 32),
    CONSTRAINT ck_customer_tokens_expiry CHECK (expires_at > created_at)
);
CREATE UNIQUE INDEX uq_customer_tokens_hash ON customer_tokens (token_hash);

DO $$
DECLARE t text;
BEGIN
    FOREACH t IN ARRAY ARRAY['customer_accounts', 'customers', 'customer_addresses', 'customer_sessions', 'customer_tokens'] LOOP
        EXECUTE format('ALTER TABLE %I ENABLE ROW LEVEL SECURITY', t);
        EXECUTE format('ALTER TABLE %I FORCE ROW LEVEL SECURITY', t);
        EXECUTE format('CREATE POLICY tenant_isolation ON %I USING (tenant_id = app.current_tenant_id()) WITH CHECK (tenant_id = app.current_tenant_id())', t);
    END LOOP;
END $$;

-- Sesi customer dicari dari hash token sebelum tenant diketahui? Tidak: Host menentukan tenant
-- lebih dulu (ADR-0014), lalu lookup di tenant context. Tidak perlu fungsi SECURITY DEFINER.

-- AC-CRM-006 / ADR-0012: permintaan hapus → anonimisasi setelah grace (≥ 30 hari).
-- Order tetap ada (snapshot tanpa PII menyusul P3-4). Lintas tenant, dipanggil worker.
CREATE FUNCTION app.purge_customers(p_grace interval) RETURNS bigint
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
REVOKE ALL ON FUNCTION app.purge_customers(interval) FROM PUBLIC;
GRANT EXECUTE ON FUNCTION app.purge_customers(interval) TO umkm_app_role;
