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
ALTER TABLE commerce_settings DROP COLUMN IF EXISTS separation_of_duties;
DROP TABLE IF EXISTS payment_proofs;
ALTER TABLE payments ADD COLUMN proof_object_id uuid,
    ADD CONSTRAINT fk_payments_proof FOREIGN KEY (tenant_id, proof_object_id)
        REFERENCES stored_objects (tenant_id, id) ON DELETE SET NULL (proof_object_id);
DROP INDEX IF EXISTS ix_order_events_seq;
ALTER TABLE order_events DROP COLUMN IF EXISTS seq;
