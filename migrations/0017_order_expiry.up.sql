-- P3-5: expiry order AWAITING_PAYMENT (AC-INV-003 untuk order, ADR-0004).
-- Reservasi milik order hanya dilepas bersama ordernya (satu sumber kebenaran): klaim bayar
-- memperpanjang order + reservasinya; app.expire_reservations melewati reservasi order yang ada.

CREATE OR REPLACE FUNCTION app.expire_reservations(p_batch integer) RETURNS integer
    LANGUAGE plpgsql SECURITY DEFINER
    SET search_path = pg_catalog, public
AS $$
DECLARE r record; n integer := 0;
BEGIN
    FOR r IN
        SELECT s.id, s.tenant_id, s.variant_id, s.quantity FROM public.inventory_reservations s
        WHERE s.status = 'active' AND s.expires_at <= now()
          AND NOT EXISTS (SELECT 1 FROM public.orders o
                          WHERE o.tenant_id = s.tenant_id AND s.reference = 'order:' || o.id::text)
        ORDER BY s.expires_at
        LIMIT greatest(1, least(p_batch, 1000))
        FOR UPDATE OF s SKIP LOCKED
    LOOP
        UPDATE public.inventory_levels SET reserved = reserved - r.quantity, updated_at = now()
            WHERE tenant_id = r.tenant_id AND variant_id = r.variant_id;
        UPDATE public.inventory_reservations SET status = 'expired', resolved_at = now() WHERE id = r.id;
        n := n + 1;
    END LOOP;
    RETURN n;
END $$;

-- Order belum dibayar yang lewat expires_at → EXPIRED; payment expired; reservasi dilepas.
-- SKIP LOCKED: order yang sedang dikonfirmasi staf tidak disentuh (konfirmasi menang).
CREATE FUNCTION app.expire_orders(p_batch integer) RETURNS integer
    LANGUAGE plpgsql SECURITY DEFINER
    SET search_path = pg_catalog, public
AS $$
DECLARE o record; r record; n integer := 0;
BEGIN
    FOR o IN
        SELECT ord.id, ord.tenant_id FROM public.orders ord
        WHERE ord.status = 'AWAITING_PAYMENT' AND ord.expires_at <= now()
        ORDER BY ord.expires_at
        LIMIT greatest(1, least(p_batch, 1000))
        FOR UPDATE SKIP LOCKED
    LOOP
        FOR r IN
            SELECT id, variant_id, quantity FROM public.inventory_reservations
            WHERE tenant_id = o.tenant_id AND reference = 'order:' || o.id::text AND status = 'active'
            ORDER BY variant_id FOR UPDATE
        LOOP
            UPDATE public.inventory_levels SET reserved = reserved - r.quantity, updated_at = now()
                WHERE tenant_id = o.tenant_id AND variant_id = r.variant_id;
            UPDATE public.inventory_reservations SET status = 'expired', resolved_at = now() WHERE id = r.id;
        END LOOP;
        UPDATE public.payments SET status = 'expired'
            WHERE tenant_id = o.tenant_id AND order_id = o.id AND status IN ('pending', 'claimed');
        UPDATE public.orders SET status = 'EXPIRED' WHERE id = o.id;
        INSERT INTO public.order_events (tenant_id, order_id, from_status, to_status, actor_type, reason)
            VALUES (o.tenant_id, o.id, 'AWAITING_PAYMENT', 'EXPIRED', 'system', 'Batas waktu pembayaran habis');
        n := n + 1;
    END LOOP;
    RETURN n;
END $$;
REVOKE ALL ON FUNCTION app.expire_orders(integer) FROM PUBLIC;
GRANT EXECUTE ON FUNCTION app.expire_orders(integer) TO umkm_app_role;
