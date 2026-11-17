-- GoPay: penjaga sesi (keep-alive) agar pemilik tidak perlu login ulang terus-menerus.
-- - refreshed_at      : kapan token terakhir berhasil diperbarui (NULL = belum pernah sejak terhubung).
-- - refresh_failures  : jumlah gagal-sementara beruntun (jaringan/5xx); direset saat berhasil.
-- - last_error        : kode singkat kegagalan terakhir (tanpa isi token), mis. `status_503`.
ALTER TABLE gopay_sessions
    ADD COLUMN refreshed_at     timestamptz,
    ADD COLUMN refresh_failures integer NOT NULL DEFAULT 0,
    ADD COLUMN last_error       text,
    ADD CONSTRAINT ck_gopay_sessions_failures CHECK (refresh_failures >= 0),
    ADD CONSTRAINT ck_gopay_sessions_last_error CHECK (last_error IS NULL OR char_length(last_error) <= 40);

-- Sesi aktif yang perlu diperbarui sekarang (lintas-tenant, untuk worker): token tinggal < 30 menit,
-- atau sudah ≥ 6 jam tidak diperbarui (refresh penjaga, sama seperti gateway asal). Terdekat dulu.
CREATE FUNCTION app.gopay_refresh_due_tenants(p_limit integer) RETURNS SETOF uuid
    LANGUAGE sql SECURITY DEFINER
    SET search_path = pg_catalog, public
AS $$
    SELECT tenant_id FROM public.gopay_sessions
    WHERE status = 'active'
      AND (expires_at < now() + interval '30 minutes'
           OR coalesce(refreshed_at, created_at) < now() - interval '6 hours')
    ORDER BY expires_at
    LIMIT greatest(1, least(p_limit, 1000));
$$;
REVOKE ALL ON FUNCTION app.gopay_refresh_due_tenants(integer) FROM PUBLIC;
GRANT EXECUTE ON FUNCTION app.gopay_refresh_due_tenants(integer) TO umkm_app_role;
