-- P1-1: skema identity & tenant (PRD.md §5.4, §5.6, §3, §4.1, §4.2).
-- RLS untuk tabel tenant-owned ditambahkan di P1-7 (0004_rls).
-- Tambahan di luar tabel inventory PRD.md §5.4 (baseline konseptual):
--   user_tokens          token verifikasi email & reset password (AUTH-003, AUTH-005)
--   user_consents        persetujuan privacy/terms berversi (D-12)
--   reserved_subdomains  subdomain cadangan (TEN-006, ADR-0001)
--   users.platform_role  role platform terpisah dari role tenant (PRD.md §3.1)

CREATE FUNCTION app.set_updated_at() RETURNS trigger
    LANGUAGE plpgsql
AS $$
BEGIN
    NEW.updated_at := now();
    RETURN NEW;
END $$;

-- ---------------------------------------------------------------- users (global)
CREATE TABLE users (
    id                uuid        PRIMARY KEY DEFAULT gen_random_uuid(),
    email             text        NOT NULL,
    password_hash     text        NOT NULL,
    status            text        NOT NULL DEFAULT 'active',
    platform_role     text,
    email_verified_at timestamptz,
    created_at        timestamptz NOT NULL DEFAULT now(),
    updated_at        timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT ck_users_email CHECK (length(email) BETWEEN 3 AND 254 AND email LIKE '%_@_%' AND email = btrim(email)),
    CONSTRAINT ck_users_password_hash CHECK (password_hash LIKE '$argon2id$%'),
    CONSTRAINT ck_users_status CHECK (status IN ('active', 'suspended', 'deleted')),
    CONSTRAINT ck_users_platform_role CHECK (platform_role IN ('super_admin', 'support', 'finance'))
);
-- AUTH-002: unik global, case-insensitive (PRD.md §5.6)
CREATE UNIQUE INDEX uq_users_email ON users (lower(email));
CREATE TRIGGER trg_users_updated_at BEFORE UPDATE ON users
    FOR EACH ROW EXECUTE FUNCTION app.set_updated_at();

-- ---------------------------------------------------------------- reserved_subdomains (global)
CREATE TABLE reserved_subdomains (
    label      text        PRIMARY KEY,
    reason     text        NOT NULL,
    created_at timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT ck_reserved_label CHECK (label = lower(label) AND label ~ '^[a-z0-9-]{1,63}$')
);
INSERT INTO reserved_subdomains (label, reason)
SELECT l, 'ADR-0001' FROM unnest(ARRAY[
    'app', 'api', 'www', 'admin', 'platform', 'mail', 'smtp', 'imap', 'pop', 'ftp',
    'static', 'cdn', 'assets', 'status', 'docs', 'help', 'support', 'blog', 'billing',
    'auth', 'login', 'dashboard', 'metrics', 'grafana', 'prometheus', 'ns', 'ns1', 'ns2',
    'dev', 'staging', 'test'
]) AS l;

-- ---------------------------------------------------------------- tenants
CREATE TABLE tenants (
    id         uuid        PRIMARY KEY DEFAULT gen_random_uuid(),
    name       text        NOT NULL,
    slug       text        NOT NULL,
    status     text        NOT NULL DEFAULT 'active',
    created_at timestamptz NOT NULL DEFAULT now(),
    updated_at timestamptz NOT NULL DEFAULT now(),
    -- ADR-0012: pemicu retensi file & data pelanggan (+30 hari)
    closed_at  timestamptz,
    CONSTRAINT ck_tenants_name CHECK (length(btrim(name)) BETWEEN 1 AND 120),
    -- ADR-0001 / TEN-005: a-z 0-9 -, 3..63, tidak diawali/diakhiri '-'
    CONSTRAINT ck_tenants_slug CHECK (slug ~ '^[a-z0-9][a-z0-9-]{1,61}[a-z0-9]$'),
    CONSTRAINT ck_tenants_status CHECK (status IN ('active', 'suspended', 'closed')),
    CONSTRAINT ck_tenants_closed CHECK ((status = 'closed') = (closed_at IS NOT NULL))
);
CREATE UNIQUE INDEX uq_tenants_slug ON tenants (slug);
CREATE TRIGGER trg_tenants_updated_at BEFORE UPDATE ON tenants
    FOR EACH ROW EXECUTE FUNCTION app.set_updated_at();

-- TEN-006: slug tidak boleh subdomain cadangan
CREATE FUNCTION app.reject_reserved_slug() RETURNS trigger
    LANGUAGE plpgsql
AS $$
BEGIN
    IF EXISTS (SELECT 1 FROM reserved_subdomains WHERE label = NEW.slug) THEN
        RAISE EXCEPTION 'slug % is reserved', NEW.slug USING ERRCODE = 'check_violation';
    END IF;
    RETURN NEW;
END $$;
CREATE TRIGGER trg_tenants_reserved_slug BEFORE INSERT OR UPDATE OF slug ON tenants
    FOR EACH ROW EXECUTE FUNCTION app.reject_reserved_slug();

-- ---------------------------------------------------------------- permissions (global)
CREATE TABLE permissions (
    id             uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    permission_key text NOT NULL,
    description    text NOT NULL DEFAULT '',
    CONSTRAINT ck_permissions_key CHECK (permission_key ~ '^[a-z_]+\.[a-z_]+$')
);
CREATE UNIQUE INDEX uq_permissions_key ON permissions (permission_key);
-- PRD.md §3.3 (33 key)
INSERT INTO permissions (permission_key)
SELECT k FROM unnest(ARRAY[
    'tenant.read', 'tenant.update',
    'members.read', 'members.invite', 'members.manage',
    'website.read', 'website.edit', 'website.publish',
    'products.read', 'products.create', 'products.update', 'products.archive',
    'inventory.read', 'inventory.adjust',
    'orders.read', 'orders.create', 'orders.approve', 'orders.reject', 'orders.cancel',
    'tickets.read', 'tickets.assign', 'tickets.reply', 'tickets.close',
    'conversations.read', 'conversations.reply', 'conversations.takeover',
    'ai_config.read', 'ai_config.manage', 'ai_provider.manage',
    'approvals.read', 'approvals.decide',
    'analytics.read', 'audit.read'
]) AS k;

-- ---------------------------------------------------------------- roles
-- tenant_id NULL = preset sistem (global); selain itu role kustom milik tenant.
CREATE TABLE roles (
    id         uuid        PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id  uuid        REFERENCES tenants (id) ON DELETE CASCADE,
    name       text        NOT NULL,
    is_system  boolean     NOT NULL DEFAULT false,
    created_at timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT ck_roles_name CHECK (length(btrim(name)) BETWEEN 1 AND 60),
    CONSTRAINT ck_roles_system_global CHECK (is_system = (tenant_id IS NULL))
);
CREATE UNIQUE INDEX uq_roles_tenant_name ON roles (tenant_id, lower(name)) NULLS NOT DISTINCT;
CREATE INDEX ix_roles_tenant ON roles (tenant_id);

CREATE TABLE role_permissions (
    role_id       uuid NOT NULL REFERENCES roles (id) ON DELETE CASCADE,
    permission_id uuid NOT NULL REFERENCES permissions (id) ON DELETE RESTRICT,
    PRIMARY KEY (role_id, permission_id)
);
CREATE INDEX ix_role_permissions_permission ON role_permissions (permission_id);

-- Role pada membership/invitation harus preset sistem atau milik tenant yang sama.
CREATE FUNCTION app.check_role_tenant() RETURNS trigger
    LANGUAGE plpgsql
AS $$
DECLARE
    role_tenant uuid;
BEGIN
    SELECT tenant_id INTO role_tenant FROM roles WHERE id = NEW.role_id;
    IF role_tenant IS NOT NULL AND role_tenant <> NEW.tenant_id THEN
        RAISE EXCEPTION 'role % belongs to another tenant', NEW.role_id USING ERRCODE = 'foreign_key_violation';
    END IF;
    RETURN NEW;
END $$;

-- ---------------------------------------------------------------- tenant_memberships
CREATE TABLE tenant_memberships (
    id         uuid        PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id  uuid        NOT NULL REFERENCES tenants (id) ON DELETE CASCADE,
    user_id    uuid        NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    role_id    uuid        NOT NULL REFERENCES roles (id) ON DELETE RESTRICT,
    status     text        NOT NULL DEFAULT 'active',
    created_at timestamptz NOT NULL DEFAULT now(),
    updated_at timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT ck_memberships_status CHECK (status IN ('active', 'suspended', 'removed')),
    CONSTRAINT uq_memberships_tenant_id UNIQUE (tenant_id, id)
);
CREATE UNIQUE INDEX uq_membership_user_tenant ON tenant_memberships (tenant_id, user_id);
CREATE INDEX ix_memberships_user ON tenant_memberships (user_id);
CREATE INDEX ix_memberships_role ON tenant_memberships (role_id);
CREATE TRIGGER trg_memberships_updated_at BEFORE UPDATE ON tenant_memberships
    FOR EACH ROW EXECUTE FUNCTION app.set_updated_at();
CREATE TRIGGER trg_memberships_role_tenant BEFORE INSERT OR UPDATE OF role_id, tenant_id ON tenant_memberships
    FOR EACH ROW EXECUTE FUNCTION app.check_role_tenant();

-- ---------------------------------------------------------------- tenant_domains
CREATE TABLE tenant_domains (
    id          uuid        PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id   uuid        NOT NULL REFERENCES tenants (id) ON DELETE CASCADE,
    hostname    text        NOT NULL,
    type        text        NOT NULL,
    status      text        NOT NULL DEFAULT 'pending_verification',
    verified_at timestamptz,
    created_at  timestamptz NOT NULL DEFAULT now(),
    updated_at  timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT ck_domains_hostname CHECK (
        hostname = lower(hostname) AND length(hostname) <= 253
        AND hostname ~ '^([a-z0-9]([a-z0-9-]{0,61}[a-z0-9])?\.)+[a-z]{2,63}$'),
    CONSTRAINT ck_domains_type CHECK (type IN ('subdomain', 'custom')),
    CONSTRAINT ck_domains_status CHECK (status IN ('pending_verification', 'active', 'suspended', 'disabled')),
    -- Custom domain hanya aktif setelah verifikasi (PRD.md §9.7)
    CONSTRAINT ck_domains_custom_verified CHECK (type <> 'custom' OR status <> 'active' OR verified_at IS NOT NULL),
    CONSTRAINT uq_domains_tenant_id UNIQUE (tenant_id, id)
);
CREATE UNIQUE INDEX uq_tenant_domains_hostname ON tenant_domains (lower(hostname));
CREATE INDEX ix_domains_tenant ON tenant_domains (tenant_id);
CREATE TRIGGER trg_domains_updated_at BEFORE UPDATE ON tenant_domains
    FOR EACH ROW EXECUTE FUNCTION app.set_updated_at();

-- ---------------------------------------------------------------- invitations
CREATE TABLE invitations (
    id          uuid        PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id   uuid        NOT NULL REFERENCES tenants (id) ON DELETE CASCADE,
    email       text        NOT NULL,
    role_id     uuid        NOT NULL REFERENCES roles (id) ON DELETE RESTRICT,
    -- Hash token (SHA-256), bukan plaintext (PRD.md §4.1)
    token_hash  bytea       NOT NULL,
    invited_by  uuid        REFERENCES users (id) ON DELETE SET NULL,
    expires_at  timestamptz NOT NULL,
    accepted_at timestamptz,
    revoked_at  timestamptz,
    created_at  timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT ck_invitations_email CHECK (length(email) BETWEEN 3 AND 254 AND email LIKE '%_@_%'),
    CONSTRAINT ck_invitations_token_hash CHECK (length(token_hash) = 32),
    CONSTRAINT ck_invitations_expiry CHECK (expires_at > created_at),
    -- AUTH-010: sekali pakai — tidak bisa sekaligus diterima dan dicabut
    CONSTRAINT ck_invitations_single_outcome CHECK (accepted_at IS NULL OR revoked_at IS NULL),
    CONSTRAINT uq_invitations_tenant_id UNIQUE (tenant_id, id)
);
CREATE UNIQUE INDEX uq_invitations_token_hash ON invitations (token_hash);
-- Satu undangan tertunda per email per tenant
CREATE UNIQUE INDEX uq_invitations_pending ON invitations (tenant_id, lower(email))
    WHERE accepted_at IS NULL AND revoked_at IS NULL;
CREATE INDEX ix_invitations_role ON invitations (role_id);
CREATE INDEX ix_invitations_invited_by ON invitations (invited_by);
CREATE TRIGGER trg_invitations_role_tenant BEFORE INSERT OR UPDATE OF role_id, tenant_id ON invitations
    FOR EACH ROW EXECUTE FUNCTION app.check_role_tenant();

-- ---------------------------------------------------------------- user_sessions (global per user)
CREATE TABLE user_sessions (
    id           uuid        PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id      uuid        NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    token_hash   bytea       NOT NULL,
    user_agent   text,
    created_at   timestamptz NOT NULL DEFAULT now(),
    last_seen_at timestamptz NOT NULL DEFAULT now(),
    expires_at   timestamptz NOT NULL,
    revoked_at   timestamptz,
    CONSTRAINT ck_sessions_token_hash CHECK (length(token_hash) = 32),
    CONSTRAINT ck_sessions_expiry CHECK (expires_at > created_at),
    CONSTRAINT ck_sessions_user_agent CHECK (user_agent IS NULL OR length(user_agent) <= 512)
);
CREATE UNIQUE INDEX uq_sessions_token_hash ON user_sessions (token_hash);
-- AUTH-006: daftar/cabut sesi aktif per user
CREATE INDEX ix_sessions_user_active ON user_sessions (user_id) WHERE revoked_at IS NULL;

-- ---------------------------------------------------------------- user_tokens
CREATE TABLE user_tokens (
    id          uuid        PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id     uuid        NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    purpose     text        NOT NULL,
    token_hash  bytea       NOT NULL,
    expires_at  timestamptz NOT NULL,
    consumed_at timestamptz,
    created_at  timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT ck_user_tokens_purpose CHECK (purpose IN ('email_verification', 'password_reset')),
    CONSTRAINT ck_user_tokens_hash CHECK (length(token_hash) = 32),
    CONSTRAINT ck_user_tokens_expiry CHECK (expires_at > created_at)
);
CREATE UNIQUE INDEX uq_user_tokens_hash ON user_tokens (token_hash);
CREATE INDEX ix_user_tokens_user_purpose ON user_tokens (user_id, purpose) WHERE consumed_at IS NULL;

-- ---------------------------------------------------------------- user_consents (D-12)
CREATE TABLE user_consents (
    id          uuid        PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id     uuid        NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    document    text        NOT NULL,
    version     text        NOT NULL,
    accepted_at timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT ck_consents_document CHECK (document IN ('privacy_policy', 'terms_of_service')),
    CONSTRAINT ck_consents_version CHECK (length(version) BETWEEN 1 AND 40)
);
CREATE UNIQUE INDEX uq_consents_user_doc_version ON user_consents (user_id, document, version);

-- ---------------------------------------------------------------- mfa_credentials
CREATE TABLE mfa_credentials (
    id               uuid        PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id          uuid        NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    kind             text        NOT NULL DEFAULT 'totp',
    -- Envelope-encrypted (P5-2 / PRD.md §6.7); tidak pernah plaintext
    encrypted_secret bytea       NOT NULL,
    confirmed_at     timestamptz,
    created_at       timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT ck_mfa_kind CHECK (kind IN ('totp')),
    CONSTRAINT ck_mfa_secret CHECK (length(encrypted_secret) >= 16)
);
CREATE UNIQUE INDEX uq_mfa_user_kind ON mfa_credentials (user_id, kind);
