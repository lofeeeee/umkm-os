-- P10 (ADR-0020): fungsi job lintas-tenant untuk poller pembayaran GoPay. SECURITY DEFINER
-- (pola sama app.expire_orders) agar worker — yang berjalan sebagai umkm_app_role tanpa konteks
-- tenant — bisa menemukan toko dengan charge pending & mengedaluwarsakan charge kedaluwarsa.

-- Daftar tenant yang punya charge 'pending' belum kedaluwarsa (untuk dipoll).
CREATE FUNCTION app.gopay_due_tenants(p_limit integer) RETURNS SETOF uuid
    LANGUAGE sql SECURITY DEFINER
    SET search_path = pg_catalog, public
AS $$
    SELECT DISTINCT tenant_id FROM public.gopay_charges
    WHERE status = 'pending' AND expires_at > now()
    LIMIT greatest(1, least(p_limit, 1000));
$$;
REVOKE ALL ON FUNCTION app.gopay_due_tenants(integer) FROM PUBLIC;
GRANT EXECUTE ON FUNCTION app.gopay_due_tenants(integer) TO umkm_app_role;

-- Edaluwarsakan charge pending yang lewat batas waktu (semua tenant). Kembalikan jumlahnya.
CREATE FUNCTION app.gopay_expire_charges(p_batch integer) RETURNS integer
    LANGUAGE plpgsql SECURITY DEFINER
    SET search_path = pg_catalog, public
AS $$
DECLARE n integer;
BEGIN
    WITH due AS (
        SELECT tenant_id, order_id FROM public.gopay_charges
        WHERE status = 'pending' AND expires_at <= now()
        ORDER BY expires_at
        LIMIT greatest(1, least(p_batch, 1000))
        FOR UPDATE SKIP LOCKED
    )
    UPDATE public.gopay_charges c SET status = 'expired'
        FROM due WHERE c.tenant_id = due.tenant_id AND c.order_id = due.order_id;
    GET DIAGNOSTICS n = ROW_COUNT;
    RETURN n;
END $$;
REVOKE ALL ON FUNCTION app.gopay_expire_charges(integer) FROM PUBLIC;
GRANT EXECUTE ON FUNCTION app.gopay_expire_charges(integer) TO umkm_app_role;

-- Auto-konfirmasi sistem (GoPay) memutuskan payment tanpa aktor pengguna. Ikat status 'terputus'
-- ke decided_at (selalu diisi), bukan decided_by (NULL untuk keputusan sistem).
ALTER TABLE payments DROP CONSTRAINT ck_payments_decided;
ALTER TABLE payments ADD CONSTRAINT ck_payments_decided
    CHECK ((status IN ('confirmed', 'rejected')) = (decided_at IS NOT NULL));
