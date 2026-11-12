DROP FUNCTION IF EXISTS app.purge_customers(interval);
DROP TABLE IF EXISTS customer_tokens;
DROP TABLE IF EXISTS customer_sessions;
DROP TABLE IF EXISTS customer_addresses;
DROP TABLE IF EXISTS customers;
DROP TABLE IF EXISTS customer_accounts;
DELETE FROM role_permissions WHERE permission_id IN (SELECT id FROM permissions
    WHERE permission_key IN ('customers.read', 'customers.manage', 'payments.verify', 'payments.refund'));
DELETE FROM permissions WHERE permission_key IN ('customers.read', 'customers.manage', 'payments.verify', 'payments.refund');
