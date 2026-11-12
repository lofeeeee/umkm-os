-- P5-1: AI platform (PRD.md §4.10–4.11, §5.4; ADR-0011 platform + BYOK).
-- Batas parameter dicerminkan dari crates/domain/src/ai (AC-AIC-001); rahasia hanya ciphertext envelope.

CREATE TABLE ai_providers (
    id             uuid        NOT NULL DEFAULT gen_random_uuid(),
    tenant_id      uuid        NOT NULL REFERENCES tenants (id) ON DELETE CASCADE,
    name           text        NOT NULL,
    source         text        NOT NULL,
    provider_type  text        NOT NULL,
    -- NULL untuk provider platform (URL milik operator, tidak pernah disimpan per tenant).
    base_url       text,
    model          text        NOT NULL,
    status         text        NOT NULL DEFAULT 'active',
    capabilities   jsonb       NOT NULL DEFAULT '{"chat": true}',
    -- Tampilan masked kunci BYOK (mis. ••••abcd); kunci asli hanya di ai_provider_secrets.
    key_hint       text,
    created_at     timestamptz NOT NULL DEFAULT now(),
    updated_at     timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (id),
    CONSTRAINT uq_ai_providers_tenant_id UNIQUE (tenant_id, id),
    CONSTRAINT uq_ai_providers_name UNIQUE (tenant_id, name),
    CONSTRAINT ck_ai_providers_name CHECK (length(btrim(name)) BETWEEN 1 AND 60),
    CONSTRAINT ck_ai_providers_source CHECK (source IN ('platform', 'byok')),
    CONSTRAINT ck_ai_providers_type CHECK (provider_type IN ('openai', 'openrouter', 'groq', 'together', 'custom', 'platform')),
    CONSTRAINT ck_ai_providers_platform CHECK ((source = 'platform') = (provider_type = 'platform')),
    -- BYOK wajib HTTPS (PRD.md §6.5 #2); validasi DNS/IP dilakukan SSRF guard saat simpan & saat panggil.
    CONSTRAINT ck_ai_providers_url CHECK (
        (source = 'platform' AND base_url IS NULL)
        OR (source = 'byok' AND base_url ~ '^https://[^\s/?#@]+(/[^\s?#]*)?$' AND length(base_url) <= 300)),
    CONSTRAINT ck_ai_providers_model CHECK (model ~ '^[A-Za-z0-9._/:@-]{1,120}$'),
    CONSTRAINT ck_ai_providers_status CHECK (status IN ('active', 'disabled')),
    CONSTRAINT ck_ai_providers_caps CHECK (jsonb_typeof(capabilities) = 'object'),
    CONSTRAINT ck_ai_providers_hint CHECK (key_hint IS NULL OR length(key_hint) <= 12)
);

-- Rahasia BYOK: envelope (DEK dibungkus KEK berversi). AAD = ai_provider|<tenant>|<provider>.
CREATE TABLE ai_provider_secrets (
    tenant_id    uuid        NOT NULL,
    provider_id  uuid        NOT NULL,
    kek_version  smallint    NOT NULL,
    wrapped_dek  bytea       NOT NULL,
    ciphertext   bytea       NOT NULL,
    created_at   timestamptz NOT NULL DEFAULT now(),
    rotated_at   timestamptz,
    PRIMARY KEY (tenant_id, provider_id),
    CONSTRAINT fk_ai_provider_secrets_provider FOREIGN KEY (tenant_id, provider_id)
        REFERENCES ai_providers (tenant_id, id) ON DELETE CASCADE,
    CONSTRAINT ck_ai_provider_secrets_kek CHECK (kek_version > 0),
    -- nonce 24 + DEK 32 + tag 16
    CONSTRAINT ck_ai_provider_secrets_dek CHECK (octet_length(wrapped_dek) = 72),
    CONSTRAINT ck_ai_provider_secrets_ct CHECK (octet_length(ciphertext) BETWEEN 41 AND 2048)
);
CREATE INDEX ix_ai_provider_secrets_kek ON ai_provider_secrets (kek_version);

CREATE TABLE chatbot_configs (
    tenant_id            uuid          NOT NULL REFERENCES tenants (id) ON DELETE CASCADE,
    enabled              boolean       NOT NULL DEFAULT false,
    mode                 text          NOT NULL DEFAULT 'hybrid',
    primary_provider_id  uuid,
    fallback_provider_id uuid,
    temperature          numeric(3,2)  NOT NULL DEFAULT 0.30,
    max_output_tokens    integer       NOT NULL DEFAULT 512,
    timeout_ms           integer       NOT NULL DEFAULT 20000,
    system_prompt        text          NOT NULL DEFAULT '',
    welcome_message      text          NOT NULL DEFAULT '',
    fallback_message     text          NOT NULL DEFAULT 'Maaf, pertanyaan Anda akan kami teruskan ke tim kami.',
    allowed_topics       text[]        NOT NULL DEFAULT '{}',
    restricted_topics    text[]        NOT NULL DEFAULT '{}',
    handover_policy      jsonb         NOT NULL DEFAULT '{}',
    operating_schedule   jsonb         NOT NULL DEFAULT '{}',
    -- Budget opsional milik tenant (BYOK); kuota platform diatur operator (ADR-0011).
    monthly_token_budget bigint,
    updated_at           timestamptz   NOT NULL DEFAULT now(),
    updated_by_user_id   uuid,
    PRIMARY KEY (tenant_id),
    CONSTRAINT fk_chatbot_primary FOREIGN KEY (tenant_id, primary_provider_id)
        REFERENCES ai_providers (tenant_id, id) ON DELETE SET NULL (primary_provider_id),
    CONSTRAINT fk_chatbot_fallback FOREIGN KEY (tenant_id, fallback_provider_id)
        REFERENCES ai_providers (tenant_id, id) ON DELETE SET NULL (fallback_provider_id),
    CONSTRAINT ck_chatbot_mode CHECK (mode IN ('auto', 'assisted', 'hybrid', 'human_only')),
    CONSTRAINT ck_chatbot_fallback_distinct CHECK (fallback_provider_id IS NULL OR fallback_provider_id <> primary_provider_id),
    CONSTRAINT ck_chatbot_temperature CHECK (temperature BETWEEN 0 AND 2),
    CONSTRAINT ck_chatbot_max_tokens CHECK (max_output_tokens BETWEEN 16 AND 4096),
    CONSTRAINT ck_chatbot_timeout CHECK (timeout_ms BETWEEN 1000 AND 60000),
    CONSTRAINT ck_chatbot_prompt CHECK (length(system_prompt) <= 4000),
    CONSTRAINT ck_chatbot_welcome CHECK (length(welcome_message) <= 500),
    CONSTRAINT ck_chatbot_fallback_msg CHECK (length(btrim(fallback_message)) BETWEEN 1 AND 500),
    CONSTRAINT ck_chatbot_topics CHECK (cardinality(allowed_topics) <= 30 AND cardinality(restricted_topics) <= 30),
    CONSTRAINT ck_chatbot_budget CHECK (monthly_token_budget IS NULL OR monthly_token_budget > 0),
    CONSTRAINT ck_chatbot_json CHECK (jsonb_typeof(handover_policy) = 'object' AND jsonb_typeof(operating_schedule) = 'object'),
    -- Mengaktifkan AI (selain Human Only) butuh provider utama.
    CONSTRAINT ck_chatbot_enabled_provider CHECK (NOT enabled OR mode = 'human_only' OR primary_provider_id IS NOT NULL)
);

CREATE TABLE chatbot_tools (
    tenant_id  uuid        NOT NULL REFERENCES tenants (id) ON DELETE CASCADE,
    tool_name  text        NOT NULL,
    enabled    boolean     NOT NULL,
    updated_at timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (tenant_id, tool_name),
    CONSTRAINT ck_chatbot_tools_name CHECK (tool_name IN (
        'search_products', 'get_product_detail', 'check_stock', 'get_business_hours', 'get_business_location',
        'search_faq', 'get_customer_orders', 'get_order_status', 'create_order_draft', 'request_human_support'))
);

-- Metering & jejak per panggilan (P5-8). conversation_id diisi P6 (FK menyusul bersama tabel conversations).
CREATE TABLE ai_runs (
    id                 uuid        NOT NULL DEFAULT gen_random_uuid(),
    tenant_id          uuid        NOT NULL REFERENCES tenants (id) ON DELETE CASCADE,
    conversation_id    uuid,
    provider_id        uuid,
    source             text        NOT NULL,
    model              text        NOT NULL,
    purpose            text        NOT NULL DEFAULT 'chat',
    status             text        NOT NULL,
    decision           text,
    prompt_tokens      integer     NOT NULL DEFAULT 0,
    completion_tokens  integer     NOT NULL DEFAULT 0,
    latency_ms         integer     NOT NULL DEFAULT 0,
    error_code         text,
    created_at         timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (id),
    CONSTRAINT uq_ai_runs_tenant_id UNIQUE (tenant_id, id),
    CONSTRAINT fk_ai_runs_provider FOREIGN KEY (tenant_id, provider_id)
        REFERENCES ai_providers (tenant_id, id) ON DELETE SET NULL (provider_id),
    CONSTRAINT ck_ai_runs_source CHECK (source IN ('platform', 'byok')),
    CONSTRAINT ck_ai_runs_purpose CHECK (purpose IN ('chat', 'embedding', 'test')),
    CONSTRAINT ck_ai_runs_status CHECK (status IN ('ok', 'blocked', 'error', 'timeout', 'quota_exceeded', 'circuit_open')),
    CONSTRAINT ck_ai_runs_decision CHECK (decision IS NULL OR decision IN ('send_auto', 'draft_for_admin', 'handover', 'human_queue')),
    CONSTRAINT ck_ai_runs_tokens CHECK (prompt_tokens >= 0 AND completion_tokens >= 0 AND latency_ms >= 0),
    CONSTRAINT ck_ai_runs_error CHECK (error_code IS NULL OR error_code ~ '^[a-z_]{1,40}$')
);
CREATE INDEX ix_ai_runs_usage ON ai_runs (tenant_id, source, created_at);

CREATE TABLE ai_tool_calls (
    id             uuid        NOT NULL DEFAULT gen_random_uuid(),
    tenant_id      uuid        NOT NULL,
    ai_run_id      uuid        NOT NULL,
    tool_name      text        NOT NULL,
    result_status  text        NOT NULL,
    -- Ringkasan argumen tervalidasi saja (tanpa PII mentah); bukan output LLM mentah.
    args_summary   jsonb       NOT NULL DEFAULT '{}',
    created_at     timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (id),
    CONSTRAINT fk_ai_tool_calls_run FOREIGN KEY (tenant_id, ai_run_id)
        REFERENCES ai_runs (tenant_id, id) ON DELETE CASCADE,
    -- Nama tool dari LLM yang tidak dikenal dicatat sebagai 'unknown' (bukan teks bebas).
    CONSTRAINT ck_ai_tool_calls_name CHECK (tool_name IN (
        'search_products', 'get_product_detail', 'check_stock', 'get_business_hours', 'get_business_location',
        'search_faq', 'get_customer_orders', 'get_order_status', 'create_order_draft', 'request_human_support', 'unknown')),
    CONSTRAINT ck_ai_tool_calls_status CHECK (result_status IN ('ok', 'denied', 'invalid', 'error')),
    CONSTRAINT ck_ai_tool_calls_args CHECK (jsonb_typeof(args_summary) = 'object' AND octet_length(args_summary::text) <= 2000)
);
CREATE INDEX ix_ai_tool_calls_run ON ai_tool_calls (tenant_id, ai_run_id);

DO $$
DECLARE t text;
BEGIN
    FOREACH t IN ARRAY ARRAY['ai_providers', 'ai_provider_secrets', 'chatbot_configs', 'chatbot_tools', 'ai_runs', 'ai_tool_calls'] LOOP
        EXECUTE format('ALTER TABLE %I ENABLE ROW LEVEL SECURITY', t);
        EXECUTE format('ALTER TABLE %I FORCE ROW LEVEL SECURITY', t);
        EXECUTE format('CREATE POLICY tenant_isolation ON %I USING (tenant_id = app.current_tenant_id()) WITH CHECK (tenant_id = app.current_tenant_id())', t);
    END LOOP;
END $$;
-- Jejak metering append-only.
REVOKE UPDATE, DELETE ON ai_runs, ai_tool_calls FROM umkm_app_role;
