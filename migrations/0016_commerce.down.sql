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
DROP TABLE IF EXISTS refunds;
DROP TABLE IF EXISTS payments;
DROP TABLE IF EXISTS order_events;
DROP TABLE IF EXISTS order_items;
DROP TABLE IF EXISTS orders;
DROP FUNCTION IF EXISTS app.check_order_initial();
DROP FUNCTION IF EXISTS app.check_order_transition();
DROP FUNCTION IF EXISTS app.order_transition_allowed(text, text, text);
DROP TABLE IF EXISTS commerce_settings;
