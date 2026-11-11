-- P1-3: data minimum agar registrasi dapat membuat tenant lengkap (PRD.md §4.2, §13.3).

-- 6 preset role sistem (PRD.md §3.2). Role hanya preset; permission = sumber otorisasi.
-- Owner = seluruh permission (PRD.md §3.2 "seluruh pengaturan tenant").
-- Mapping 5 preset lain ditetapkan di P1-5 (RBAC) — sampai itu tanpa permission
-- (deny-by-default).
INSERT INTO roles (tenant_id, name, is_system)
SELECT NULL, r, true FROM unnest(ARRAY[
    'Owner', 'Manager', 'Admin', 'Support Agent', 'Catalog Manager', 'Viewer'
]) AS r;

INSERT INTO role_permissions (role_id, permission_id)
SELECT r.id, p.id FROM roles r CROSS JOIN permissions p
WHERE r.tenant_id IS NULL AND r.name = 'Owner';

-- BR-005: setiap tenant memiliki business profile. Kolom lengkap PRD.md §4.4 di P2-5.
CREATE TABLE business_profiles (
    tenant_id  uuid        PRIMARY KEY REFERENCES tenants (id) ON DELETE CASCADE,
    name       text        NOT NULL,
    created_at timestamptz NOT NULL DEFAULT now(),
    updated_at timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT ck_business_profiles_name CHECK (length(btrim(name)) BETWEEN 1 AND 120)
);
CREATE TRIGGER trg_business_profiles_updated_at BEFORE UPDATE ON business_profiles
    FOR EACH ROW EXECUTE FUNCTION app.set_updated_at();
