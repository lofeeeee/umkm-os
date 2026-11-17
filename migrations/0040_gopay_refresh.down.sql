DROP FUNCTION IF EXISTS app.gopay_refresh_due_tenants(integer);
ALTER TABLE gopay_sessions
    DROP CONSTRAINT IF EXISTS ck_gopay_sessions_last_error,
    DROP CONSTRAINT IF EXISTS ck_gopay_sessions_failures,
    DROP COLUMN IF EXISTS last_error,
    DROP COLUMN IF EXISTS refresh_failures,
    DROP COLUMN IF EXISTS refreshed_at;
