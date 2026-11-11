DELETE FROM role_permissions
WHERE role_id IN (SELECT id FROM roles WHERE tenant_id IS NULL AND is_system AND name <> 'Owner');
