DROP TABLE IF EXISTS business_profiles;
DELETE FROM role_permissions WHERE role_id IN (SELECT id FROM roles WHERE tenant_id IS NULL AND is_system);
DELETE FROM roles WHERE tenant_id IS NULL AND is_system;
