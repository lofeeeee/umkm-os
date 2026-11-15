-- P10 (ADR-0020): integrasi GoPay Merchant (GoBiz) untuk konfirmasi pembayaran QRIS otomatis.
-- Sesi GoBiz per toko (token terenkripsi envelope, pola sama seperti ai_provider_secrets) +
-- "charge" per pesanan (QRIS dinamis bernominal unik untuk pencocokan pembayaran masuk).

-- Sesi GoBiz: satu per toko (tenant). Token access/refresh disimpan terenkripsi (ciphertext).
-- static_qris = payload EMVCo QRIS statis toko (semi-publik, bukan rahasia) untuk disintesis
-- jadi QRIS dinamis per pesanan.
CREATE TABLE gopay_sessions (
    tenant_id       uuid        NOT NULL,
    phone_e164      text        NOT NULL,
    merchant_id     text,
    outlet_name     text,
    static_qris     text,
    kek_version     smallint    NOT NULL,
    wrapped_dek     bytea       NOT NULL,
    ciphertext      bytea       NOT NULL,
    status          text        NOT NULL DEFAULT 'active',
    expires_at      timestamptz NOT NULL,
    created_at      timestamptz NOT NULL DEFAULT now(),
    updated_at      timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (tenant_id),
    CONSTRAINT fk_gopay_sessions_tenant FOREIGN KEY (tenant_id)
        REFERENCES tenants (id) ON DELETE CASCADE,
    CONSTRAINT ck_gopay_sessions_kek CHECK (kek_version > 0),
    -- nonce 24 + DEK 32 + tag 16
    CONSTRAINT ck_gopay_sessions_dek CHECK (octet_length(wrapped_dek) = 72),
    CONSTRAINT ck_gopay_sessions_ct CHECK (octet_length(ciphertext) BETWEEN 41 AND 8192),
    CONSTRAINT ck_gopay_sessions_status CHECK (status IN ('active', 'needs_relogin')),
    CONSTRAINT ck_gopay_sessions_qris CHECK (static_qris IS NULL OR octet_length(static_qris) BETWEEN 20 AND 1024)
);
CREATE INDEX ix_gopay_sessions_kek ON gopay_sessions (kek_version);

-- Charge QRIS dinamis per pesanan. total_rupiah = base + kode unik (1..999).
CREATE TABLE gopay_charges (
    tenant_id       uuid        NOT NULL,
    order_id        uuid        NOT NULL,
    base_rupiah     bigint      NOT NULL,
    unique_code     smallint    NOT NULL,
    total_rupiah    bigint      NOT NULL,
    qris_payload    text        NOT NULL,
    status          text        NOT NULL DEFAULT 'pending',
    matched_txn_id  text,
    matched_at      timestamptz,
    issued_at       timestamptz NOT NULL DEFAULT now(),
    expires_at      timestamptz NOT NULL,
    PRIMARY KEY (tenant_id, order_id),
    CONSTRAINT fk_gopay_charges_order FOREIGN KEY (tenant_id, order_id)
        REFERENCES orders (tenant_id, id) ON DELETE CASCADE,
    CONSTRAINT ck_gopay_charges_amounts CHECK (
        base_rupiah > 0
        AND unique_code BETWEEN 1 AND 999
        AND total_rupiah = base_rupiah + unique_code
    ),
    CONSTRAINT ck_gopay_charges_status CHECK (status IN ('pending', 'matched', 'expired')),
    CONSTRAINT ck_gopay_charges_payload CHECK (octet_length(qris_payload) BETWEEN 20 AND 1024),
    CONSTRAINT ck_gopay_charges_matched CHECK (
        (status = 'matched') = (matched_txn_id IS NOT NULL)
    )
);
-- Nominal unik: tak boleh dua charge 'pending' bernominal sama per toko (pencocokan jadi ambigu).
CREATE UNIQUE INDEX uq_gopay_charges_pending_amount
    ON gopay_charges (tenant_id, total_rupiah) WHERE status = 'pending';
-- Anti double-claim: satu transaksi GoBiz hanya boleh melunasi satu charge.
CREATE UNIQUE INDEX uq_gopay_charges_txn
    ON gopay_charges (tenant_id, matched_txn_id) WHERE matched_txn_id IS NOT NULL;

DO $$
DECLARE t text;
BEGIN
    FOREACH t IN ARRAY ARRAY['gopay_sessions', 'gopay_charges'] LOOP
        EXECUTE format('ALTER TABLE %I ENABLE ROW LEVEL SECURITY', t);
        EXECUTE format('ALTER TABLE %I FORCE ROW LEVEL SECURITY', t);
        EXECUTE format('CREATE POLICY tenant_isolation ON %I USING (tenant_id = app.current_tenant_id()) WITH CHECK (tenant_id = app.current_tenant_id())', t);
    END LOOP;
END $$;
