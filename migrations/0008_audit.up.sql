-- P1-8: audit log append-only (PRD.md §6.9). Retensi 1 tahun (ADR-0012) via purge P1-9.
-- Tanpa FK ke tenants/users: jejak audit harus bertahan walau entitas dihapus.
CREATE TABLE audit_logs (
    id            uuid        PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id     uuid,
    actor_type    text        NOT NULL,
    actor_user_id uuid,
    action        text        NOT NULL,
    resource_type text        NOT NULL,
    resource_id   text,
    request_id    text,
    ip            text,
    summary       jsonb       NOT NULL DEFAULT '{}'::jsonb,
    created_at    timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT ck_audit_actor_type CHECK (actor_type IN ('user', 'anonymous', 'system', 'platform')),
    CONSTRAINT ck_audit_action CHECK (action ~ '^[a-z_]+(\.[a-z_]+)+$'),
    CONSTRAINT ck_audit_resource_type CHECK (resource_type ~ '^[a-z_]+$'),
    CONSTRAINT ck_audit_summary CHECK (jsonb_typeof(summary) = 'object' AND length(summary::text) <= 8192),
    CONSTRAINT ck_audit_ip CHECK (ip IS NULL OR length(ip) <= 64)
);
CREATE INDEX ix_audit_tenant_time ON audit_logs (tenant_id, created_at DESC, id DESC);
CREATE INDEX ix_audit_actor_time ON audit_logs (actor_user_id, created_at DESC);
CREATE INDEX ix_audit_created ON audit_logs (created_at);

-- Append-only: UPDATE selalu ditolak; DELETE hanya oleh purge retensi (flag sesi
-- `app.audit_purge` diset oleh fungsi SECURITY DEFINER milik owner, P1-9).
CREATE FUNCTION app.audit_append_only() RETURNS trigger
    LANGUAGE plpgsql
AS $$
BEGIN
    IF TG_OP = 'DELETE' AND current_setting('app.audit_purge', true) = 'on' THEN
        RETURN OLD;
    END IF;
    RAISE EXCEPTION 'audit_logs is append-only (%)', TG_OP USING ERRCODE = 'insufficient_privilege';
END $$;
CREATE TRIGGER trg_audit_append_only BEFORE UPDATE OR DELETE ON audit_logs
    FOR EACH ROW EXECUTE FUNCTION app.audit_append_only();
CREATE TRIGGER trg_audit_no_truncate BEFORE TRUNCATE ON audit_logs
    FOR EACH STATEMENT EXECUTE FUNCTION app.audit_append_only();

-- Hak role aplikasi: hanya baca + tambah (default privileges 0007 memberi lebih).
REVOKE UPDATE, DELETE, TRUNCATE ON audit_logs FROM umkm_app_role;
GRANT SELECT, INSERT ON audit_logs TO umkm_app_role;

ALTER TABLE audit_logs ENABLE ROW LEVEL SECURITY;
ALTER TABLE audit_logs FORCE ROW LEVEL SECURITY;
-- Tenant hanya membaca audit miliknya. Baris platform (tenant_id NULL) tidak terbaca
-- role aplikasi; akses platform admin menyusul (PRD.md §6.9 "Platform admin access").
CREATE POLICY audit_read ON audit_logs FOR SELECT
    USING (tenant_id = app.current_tenant_id());
-- Insert: event platform (tanpa tenant) atau tenant sesuai context; tidak bisa
-- menulis atas nama tenant lain.
CREATE POLICY audit_insert ON audit_logs FOR INSERT
    WITH CHECK (tenant_id IS NULL OR tenant_id = app.current_tenant_id());
