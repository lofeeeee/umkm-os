-- P5-7/P5-8: antrean indeks pengetahuan lintas tenant untuk worker. Hanya mengembalikan pasangan id;
-- pengindeksan sendiri berjalan di transaksi RLS tenant masing-masing.
CREATE FUNCTION app.pending_knowledge(p_batch integer)
RETURNS TABLE (tenant_id uuid, document_id uuid)
    LANGUAGE sql STABLE SECURITY DEFINER
    SET search_path = pg_catalog, public
AS $$
    SELECT d.tenant_id, d.id FROM public.knowledge_documents d
    WHERE d.status = 'pending' AND d.attempts < 5
    ORDER BY d.updated_at LIMIT greatest(1, least(p_batch, 100))
$$;
REVOKE ALL ON FUNCTION app.pending_knowledge(integer) FROM PUBLIC;
GRANT EXECUTE ON FUNCTION app.pending_knowledge(integer) TO umkm_app_role;
