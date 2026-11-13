ALTER TABLE tenants DROP COLUMN IF EXISTS require_admin_mfa;
ALTER TABLE user_sessions DROP COLUMN IF EXISTS mfa_verified_at;
DROP TABLE IF EXISTS mfa_challenges;
DROP TABLE IF EXISTS mfa_recovery_codes;
ALTER TABLE mfa_credentials
    DROP CONSTRAINT IF EXISTS ck_mfa_secret_len,
    DROP CONSTRAINT IF EXISTS ck_mfa_dek,
    DROP CONSTRAINT IF EXISTS ck_mfa_kek,
    DROP COLUMN IF EXISTS last_used_step,
    DROP COLUMN IF EXISTS wrapped_dek,
    DROP COLUMN IF EXISTS kek_version;
