-- P6-3 (FR-CNV-003, AC-CNV-003): Viewer membaca percakapan dengan data sensitif dimasking.
-- Tampilan tanpa masking hanya untuk role ber-`conversations.reply` (staf yang menangani chat).
INSERT INTO role_permissions (role_id, permission_id)
SELECT r.id, p.id
FROM roles r JOIN permissions p ON p.permission_key = 'conversations.read'
WHERE r.tenant_id IS NULL AND r.name = 'Viewer'
ON CONFLICT DO NOTHING;
