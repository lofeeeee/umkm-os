-- P1-4: resolusi tenant dari hostname (PRD.md §4.2, TEN-009).
-- Dipanggil SEBELUM tenant context diketahui, sehingga harus dapat melihat semua
-- baris tenant_domains walau RLS aktif (P1-7). SECURITY DEFINER dengan permukaan
-- minimal: input hostname persis, output hanya id + status (tanpa data tenant lain).
CREATE FUNCTION app.resolve_host(p_hostname text)
    RETURNS TABLE (tenant_id uuid, domain_status text, tenant_status text)
    LANGUAGE sql STABLE SECURITY DEFINER
    SET search_path = pg_catalog, public
AS $$
    SELECT d.tenant_id, d.status, t.status
    FROM public.tenant_domains d
    JOIN public.tenants t ON t.id = d.tenant_id
    WHERE lower(d.hostname) = lower(p_hostname)
$$;

REVOKE ALL ON FUNCTION app.resolve_host(text) FROM PUBLIC;
COMMENT ON FUNCTION app.resolve_host(text) IS
    'Host -> tenant untuk resolver API; EXECUTE diberikan ke role aplikasi di P1-7';
