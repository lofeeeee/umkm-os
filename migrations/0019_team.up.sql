-- P4-1: menerima undangan. Token datang sebelum tenant diketahui → lookup lintas tenant HANYA
-- lewat hash token (SECURITY DEFINER), mengembalikan tenant + email untuk diproses di tenant context.
CREATE FUNCTION app.find_invitation(p_token_hash bytea)
    RETURNS TABLE (id uuid, tenant_id uuid, email text)
    LANGUAGE sql STABLE SECURITY DEFINER
    SET search_path = pg_catalog, public
AS $$
    SELECT i.id, i.tenant_id, i.email FROM public.invitations i
    JOIN public.tenants t ON t.id = i.tenant_id AND t.status = 'active'
    WHERE i.token_hash = p_token_hash AND i.accepted_at IS NULL AND i.revoked_at IS NULL
      AND i.expires_at > now()
$$;
REVOKE ALL ON FUNCTION app.find_invitation(bytea) FROM PUBLIC;
GRANT EXECUTE ON FUNCTION app.find_invitation(bytea) TO umkm_app_role;
