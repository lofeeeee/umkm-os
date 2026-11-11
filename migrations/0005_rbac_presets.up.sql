-- P1-5: permission preset role sistem (keputusan produk 2026-11-11, product-decision-log).
-- Owner sudah mendapat seluruh 33 permission di 0003.
INSERT INTO role_permissions (role_id, permission_id)
SELECT r.id, p.id
FROM (VALUES
    -- Manager (26)
    ('Manager', 'tenant.read'), ('Manager', 'members.read'),
    ('Manager', 'website.read'), ('Manager', 'website.edit'),
    ('Manager', 'products.read'), ('Manager', 'products.create'), ('Manager', 'products.update'), ('Manager', 'products.archive'),
    ('Manager', 'inventory.read'), ('Manager', 'inventory.adjust'),
    ('Manager', 'orders.read'), ('Manager', 'orders.create'), ('Manager', 'orders.approve'), ('Manager', 'orders.reject'), ('Manager', 'orders.cancel'),
    ('Manager', 'tickets.read'), ('Manager', 'tickets.assign'), ('Manager', 'tickets.reply'), ('Manager', 'tickets.close'),
    ('Manager', 'conversations.read'), ('Manager', 'conversations.reply'), ('Manager', 'conversations.takeover'),
    ('Manager', 'approvals.read'), ('Manager', 'approvals.decide'),
    ('Manager', 'analytics.read'), ('Manager', 'ai_config.read'),
    -- Support Agent (9)
    ('Support Agent', 'tenant.read'),
    ('Support Agent', 'tickets.read'), ('Support Agent', 'tickets.reply'), ('Support Agent', 'tickets.close'),
    ('Support Agent', 'conversations.read'), ('Support Agent', 'conversations.reply'), ('Support Agent', 'conversations.takeover'),
    ('Support Agent', 'orders.read'), ('Support Agent', 'products.read'),
    -- Catalog Manager (7)
    ('Catalog Manager', 'tenant.read'),
    ('Catalog Manager', 'products.read'), ('Catalog Manager', 'products.create'), ('Catalog Manager', 'products.update'), ('Catalog Manager', 'products.archive'),
    ('Catalog Manager', 'inventory.read'), ('Catalog Manager', 'inventory.adjust'),
    -- Viewer (7)
    ('Viewer', 'tenant.read'), ('Viewer', 'website.read'), ('Viewer', 'products.read'), ('Viewer', 'inventory.read'),
    ('Viewer', 'orders.read'), ('Viewer', 'tickets.read'), ('Viewer', 'analytics.read'),
    -- Admin (1)
    ('Admin', 'tenant.read')
) AS m(role_name, permission_key)
JOIN roles r ON r.tenant_id IS NULL AND r.is_system AND r.name = m.role_name
JOIN permissions p ON p.permission_key = m.permission_key;
