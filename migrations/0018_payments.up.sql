-- P3-7/P3-8: bukti bayar pembeli (data pelanggan, terpisah dari stored_objects agar tidak
-- pernah bisa dirujuk sebagai aset publik website/produk) + kebijakan separation-of-duties.

ALTER TABLE payments DROP COLUMN proof_object_id;

-- Urutan event stabil: beberapa event dalam satu transaksi berbagi now().
ALTER TABLE order_events ADD COLUMN seq bigint GENERATED ALWAYS AS IDENTITY;
CREATE INDEX ix_order_events_seq ON order_events (tenant_id, order_id, seq);

CREATE TABLE payment_proofs (
    tenant_id    uuid        NOT NULL REFERENCES tenants (id) ON DELETE CASCADE,
    payment_id   uuid        NOT NULL,
    content_type text        NOT NULL,
    byte_size    integer     NOT NULL,
    sha256       bytea       NOT NULL,
    bytes        bytea       NOT NULL,
    created_at   timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (tenant_id, payment_id),
    CONSTRAINT fk_payment_proofs_payment FOREIGN KEY (tenant_id, payment_id)
        REFERENCES payments (tenant_id, id) ON DELETE CASCADE,
    CONSTRAINT ck_payment_proofs_type CHECK (content_type IN ('image/png', 'image/jpeg', 'image/webp')),
    CONSTRAINT ck_payment_proofs_size CHECK (byte_size > 0 AND byte_size = octet_length(bytes)),
    CONSTRAINT ck_payment_proofs_sha CHECK (octet_length(sha256) = 32)
);
ALTER TABLE payment_proofs ALTER COLUMN bytes SET STORAGE EXTERNAL;
ALTER TABLE payment_proofs ENABLE ROW LEVEL SECURITY;
ALTER TABLE payment_proofs FORCE ROW LEVEL SECURITY;
CREATE POLICY tenant_isolation ON payment_proofs
    USING (tenant_id = app.current_tenant_id()) WITH CHECK (tenant_id = app.current_tenant_id());
-- Ganti bukti = hapus + sisip (immutable seperti stored_objects).
REVOKE UPDATE ON payment_proofs FROM umkm_app_role;

-- PRD §4.7: "pemohon tidak boleh menyetujui permintaannya sendiri jika SoD diaktifkan".
-- Pemohon approval order = staf yang mengonfirmasi pembayaran (yang memasukkan order ke antrean approval).
ALTER TABLE commerce_settings ADD COLUMN separation_of_duties boolean NOT NULL DEFAULT true;

-- Anonimisasi customer juga menghapus bukti bayarnya (ADR-0004: bukti = data pelanggan).
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
    ), proofs AS (
        DELETE FROM public.payment_proofs pp USING public.payments p, due
        WHERE pp.tenant_id = p.tenant_id AND pp.payment_id = p.id
          AND p.tenant_id = due.tenant_id
          AND p.order_id IN (SELECT o2.id FROM public.orders o2 WHERE o2.tenant_id = due.tenant_id AND o2.customer_id = due.id)
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
