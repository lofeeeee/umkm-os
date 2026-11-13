DROP FUNCTION IF EXISTS app.platform_decide_invoice(uuid, uuid, boolean, text);
DROP FUNCTION IF EXISTS app.platform_invoice_proof(uuid);
DROP FUNCTION IF EXISTS app.platform_list_invoices(text);
DROP TABLE IF EXISTS subscription_invoices;
DROP FUNCTION IF EXISTS app.guard_invoice_insert();
DROP FUNCTION IF EXISTS app.guard_invoice_update();
