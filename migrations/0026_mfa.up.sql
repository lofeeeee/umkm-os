-- MFA TOTP (AUTH-011/012, Gate F 2026-11-12). Rahasia TOTP dienkripsi envelope (ADR-0011):
-- encrypted_secret = nonce || ciphertext, wrapped_dek = DEK terbungkus KEK versi kek_version.
ALTER TABLE mfa_credentials
    ADD COLUMN kek_version    smallint NOT NULL,
    ADD COLUMN wrapped_dek    bytea    NOT NULL,
    -- anti-replay: langkah waktu TOTP terakhir yang diterima
    ADD COLUMN last_used_step bigint,
    ADD CONSTRAINT ck_mfa_kek CHECK (kek_version > 0),
    ADD CONSTRAINT ck_mfa_dek CHECK (octet_length(wrapped_dek) = 72),
    ADD CONSTRAINT ck_mfa_secret_len CHECK (octet_length(encrypted_secret) <= 256);

-- Kode pemulihan sekali pakai; hanya hash SHA-256 yang disimpan.
CREATE TABLE mfa_recovery_codes (
    id         uuid        PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id    uuid        NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    code_hash  bytea       NOT NULL,
    used_at    timestamptz,
    created_at timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT ck_mfa_recovery_hash CHECK (octet_length(code_hash) = 32)
);
CREATE UNIQUE INDEX uq_mfa_recovery_user_hash ON mfa_recovery_codes (user_id, code_hash);

-- Tantangan login langkah kedua: password benar → token tantangan (bukan sesi).
CREATE TABLE mfa_challenges (
    id          uuid        PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id     uuid        NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    token_hash  bytea       NOT NULL,
    attempts    smallint    NOT NULL DEFAULT 0,
    expires_at  timestamptz NOT NULL,
    consumed_at timestamptz,
    created_at  timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT ck_mfa_challenge_hash CHECK (octet_length(token_hash) = 32),
    CONSTRAINT ck_mfa_challenge_attempts CHECK (attempts BETWEEN 0 AND 10),
    CONSTRAINT ck_mfa_challenge_expiry CHECK (expires_at > created_at)
);
CREATE UNIQUE INDEX uq_mfa_challenges_token ON mfa_challenges (token_hash);
CREATE INDEX ix_mfa_challenges_user ON mfa_challenges (user_id);

-- Sesi yang lolos langkah kedua (login MFA atau baru mengaktifkan MFA).
ALTER TABLE user_sessions ADD COLUMN mfa_verified_at timestamptz;

-- AUTH-012: Owner dapat mewajibkan MFA untuk role admin tenant (Owner/Manager/Admin).
ALTER TABLE tenants ADD COLUMN require_admin_mfa boolean NOT NULL DEFAULT false;

GRANT SELECT, INSERT, UPDATE, DELETE ON mfa_recovery_codes, mfa_challenges TO umkm_app_role;
