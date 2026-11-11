-- P0-4: fondasi tenant context (PRD.md §5.7).
-- Semua policy RLS memakai fungsi ini agar cara membaca context seragam.
CREATE SCHEMA IF NOT EXISTS app;

-- Tenant aktif dari setting transaction-local `app.tenant_id`.
-- NULL bila tidak di-set -> policy `tenant_id = app.current_tenant_id()` menolak semua baris (fail-closed).
CREATE FUNCTION app.current_tenant_id() RETURNS uuid
    LANGUAGE sql STABLE PARALLEL SAFE
AS $$ SELECT NULLIF(current_setting('app.tenant_id', true), '')::uuid $$;

COMMENT ON FUNCTION app.current_tenant_id() IS
    'Tenant context transaction-local; di-set via set_config(''app.tenant_id'', $1, true). PRD.md §5.7';
