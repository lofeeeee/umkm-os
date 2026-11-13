DELETE FROM role_permissions
WHERE role_id IN (SELECT id FROM roles WHERE tenant_id IS NULL AND name = 'Viewer')
  AND permission_id IN (SELECT id FROM permissions WHERE permission_key = 'conversations.read');
