DROP FUNCTION IF EXISTS app.purge_outbox(timestamptz);
DROP FUNCTION IF EXISTS app.purge_auth_artifacts(timestamptz);
DROP FUNCTION IF EXISTS app.purge_closed_tenants(interval);
DROP FUNCTION IF EXISTS app.purge_audit(timestamptz);
