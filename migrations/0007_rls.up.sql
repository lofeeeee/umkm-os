-- P1-7: Row-Level Security ENABLE + FORCE (PRD.md §5.7, §6.3).
--
-- Role aplikasi: `umkm_app_role` (NOLOGIN, tanpa BYPASSRLS, bukan superuser).
-- User login aplikasi (`umkm_app`) dibuat di luar migration (ops/postgres/init) dengan
-- password dari secret runtime, lalu menjadi anggota role ini. Migration tetap
-- dijalankan oleh owner skema.

DO $$
BEGIN
    BEGIN
        CREATE ROLE umkm_app_role NOLOGIN;
    EXCEPTION WHEN duplicate_object THEN
        NULL; -- sudah ada (role cluster-level; migration DB lain bisa berjalan paralel)
    END;
    -- Role yang sudah ada bisa salah atribut (dibuat/diubah di luar migration): paksa
    -- aman. Hanya saat salah, agar migration paralel tidak saling menimpa baris pg_authid.
    IF EXISTS (SELECT 1 FROM pg_roles WHERE rolname = 'umkm_app_role'
               AND (rolsuper OR rolbypassrls OR rolcanlogin OR rolcreatedb OR rolcreaterole OR rolreplication)) THEN
        ALTER ROLE umkm_app_role NOLOGIN NOSUPERUSER NOBYPASSRLS NOCREATEDB NOCREATEROLE NOREPLICATION;
    END IF;
    IF EXISTS (SELECT 1 FROM pg_roles WHERE rolname = 'umkm_app') THEN
        GRANT umkm_app_role TO umkm_app;
    END IF;
END $$;

GRANT USAGE ON SCHEMA public, app TO umkm_app_role;
GRANT EXECUTE ON FUNCTION app.current_tenant_id() TO umkm_app_role;
GRANT EXECUTE ON FUNCTION app.resolve_host(text) TO umkm_app_role;

-- Tabel global (bukan milik tenant)
GRANT SELECT, INSERT, UPDATE ON users, user_sessions, user_tokens, user_consents, mfa_credentials TO umkm_app_role;
GRANT DELETE ON mfa_credentials TO umkm_app_role;
GRANT SELECT ON permissions, reserved_subdomains TO umkm_app_role;
-- Infrastruktur platform (BUILD_PLAN.md §3.4): worker memproses lintas tenant.
GRANT SELECT, INSERT, UPDATE ON notification_outbox TO umkm_app_role;

-- Tabel milik tenant
GRANT SELECT, INSERT, UPDATE, DELETE ON
    tenants, tenant_memberships, tenant_domains, invitations, roles, role_permissions, business_profiles
TO umkm_app_role;
-- _sqlx_migrations sengaja TIDAK di-grant.

-- ---------------------------------------------------------------- policies
ALTER TABLE tenants ENABLE ROW LEVEL SECURITY;
ALTER TABLE tenants FORCE ROW LEVEL SECURITY;
CREATE POLICY tenant_isolation ON tenants
    USING (id = app.current_tenant_id())
    WITH CHECK (id = app.current_tenant_id());

ALTER TABLE tenant_memberships ENABLE ROW LEVEL SECURITY;
ALTER TABLE tenant_memberships FORCE ROW LEVEL SECURITY;
CREATE POLICY tenant_isolation ON tenant_memberships
    USING (tenant_id = app.current_tenant_id())
    WITH CHECK (tenant_id = app.current_tenant_id());

ALTER TABLE tenant_domains ENABLE ROW LEVEL SECURITY;
ALTER TABLE tenant_domains FORCE ROW LEVEL SECURITY;
CREATE POLICY tenant_isolation ON tenant_domains
    USING (tenant_id = app.current_tenant_id())
    WITH CHECK (tenant_id = app.current_tenant_id());

ALTER TABLE invitations ENABLE ROW LEVEL SECURITY;
ALTER TABLE invitations FORCE ROW LEVEL SECURITY;
CREATE POLICY tenant_isolation ON invitations
    USING (tenant_id = app.current_tenant_id())
    WITH CHECK (tenant_id = app.current_tenant_id());

ALTER TABLE business_profiles ENABLE ROW LEVEL SECURITY;
ALTER TABLE business_profiles FORCE ROW LEVEL SECURITY;
CREATE POLICY tenant_isolation ON business_profiles
    USING (tenant_id = app.current_tenant_id())
    WITH CHECK (tenant_id = app.current_tenant_id());

-- roles: preset sistem (tenant_id NULL) terbaca semua; hanya role kustom tenant sendiri
-- yang dapat ditulis. Preset sistem tidak dapat diubah oleh role aplikasi.
ALTER TABLE roles ENABLE ROW LEVEL SECURITY;
ALTER TABLE roles FORCE ROW LEVEL SECURITY;
CREATE POLICY roles_read ON roles FOR SELECT
    USING (tenant_id IS NULL OR tenant_id = app.current_tenant_id());
CREATE POLICY roles_write ON roles FOR ALL
    USING (tenant_id = app.current_tenant_id())
    WITH CHECK (tenant_id = app.current_tenant_id());

-- role_permissions mengikuti visibilitas/kepemilikan role-nya.
ALTER TABLE role_permissions ENABLE ROW LEVEL SECURITY;
ALTER TABLE role_permissions FORCE ROW LEVEL SECURITY;
CREATE POLICY role_permissions_read ON role_permissions FOR SELECT
    USING (EXISTS (SELECT 1 FROM roles r WHERE r.id = role_id
                   AND (r.tenant_id IS NULL OR r.tenant_id = app.current_tenant_id())));
CREATE POLICY role_permissions_write ON role_permissions FOR ALL
    USING (EXISTS (SELECT 1 FROM roles r WHERE r.id = role_id AND r.tenant_id = app.current_tenant_id()))
    WITH CHECK (EXISTS (SELECT 1 FROM roles r WHERE r.id = role_id AND r.tenant_id = app.current_tenant_id()));

-- ---------------------------------------------------------------- lintas tenant yang sah
-- Daftar membership milik user sesi (AUTH-008). Input = user_id yang sudah diverifikasi
-- dari sesi; output hanya ringkasan tenant tempat ia anggota aktif.
CREATE FUNCTION app.user_memberships(p_user_id uuid)
    RETURNS TABLE (tenant_id uuid, name text, slug text, role_name text)
    LANGUAGE sql STABLE SECURITY DEFINER
    SET search_path = pg_catalog, public
AS $$
    SELECT t.id, t.name, t.slug, r.name
    FROM public.tenant_memberships m
    JOIN public.tenants t ON t.id = m.tenant_id
    JOIN public.roles r ON r.id = m.role_id
    WHERE m.user_id = p_user_id AND m.status = 'active' AND t.status = 'active'
    ORDER BY t.name, t.id
$$;
REVOKE ALL ON FUNCTION app.user_memberships(uuid) FROM PUBLIC;
GRANT EXECUTE ON FUNCTION app.user_memberships(uuid) TO umkm_app_role;

-- Tabel baru yang dibuat migration berikutnya otomatis memberi hak ke role aplikasi;
-- RLS-nya tetap wajib (dijaga tes katalog `every_tenant_table_has_forced_rls`).
ALTER DEFAULT PRIVILEGES IN SCHEMA public GRANT SELECT, INSERT, UPDATE, DELETE ON TABLES TO umkm_app_role;
