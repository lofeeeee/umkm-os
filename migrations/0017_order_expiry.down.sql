DROP FUNCTION IF EXISTS app.expire_orders(integer);
CREATE OR REPLACE FUNCTION app.expire_reservations(p_batch integer) RETURNS integer
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
