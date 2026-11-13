-- P7-1: konfigurasi paket & langganan (PRD.md §4.15, §12; FR-SUB-001; ADR-0016, ADR-0008).
-- Batas paket = DATA (`plans.limits_json`), bukan kode: perubahan berlaku tanpa deploy (AC-SUB-001).

CREATE TABLE plans (
    code         text        PRIMARY KEY,
    name         text        NOT NULL,
    -- Harga per periode dalam Rupiah utuh.
    price_idr    integer     NOT NULL,
    period_days  integer     NOT NULL DEFAULT 30,
    -- Paket yang dipakai saat uji coba toko baru (tepat satu).
    trial_days   integer,
    limits_json  jsonb       NOT NULL,
    active       boolean     NOT NULL DEFAULT true,
    position     integer     NOT NULL DEFAULT 0,
    updated_at   timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT ck_plans_code CHECK (code ~ '^[a-z][a-z0-9_]{1,30}$'),
    CONSTRAINT ck_plans_name CHECK (length(btrim(name)) BETWEEN 1 AND 60),
    CONSTRAINT ck_plans_price CHECK (price_idr >= 0),
    CONSTRAINT ck_plans_period CHECK (period_days BETWEEN 1 AND 366),
    CONSTRAINT ck_plans_trial CHECK (trial_days IS NULL OR trial_days BETWEEN 1 AND 90),
    CONSTRAINT ck_plans_limits CHECK (jsonb_typeof(limits_json) = 'object')
);
CREATE UNIQUE INDEX uq_plans_trial ON plans ((true)) WHERE trial_days IS NOT NULL;
-- Katalog platform: aplikasi hanya membaca; perubahan oleh operator/platform admin.
REVOKE INSERT, UPDATE, DELETE ON plans FROM umkm_app_role;

-- Nilai awal ADR-0016. null = tak terbatas (fair-use).
INSERT INTO plans (code, name, price_idr, trial_days, position, limits_json) VALUES
 ('starter', 'Starter', 50000, 14, 1, '{
    "max_members": 2, "max_products": 30, "platform_ai_tokens_monthly": 0, "byok_allowed": true,
    "storage_bytes": 52428800, "conversations_monthly": 100, "custom_domain": false}'),
 ('pro', 'Pro', 100000, NULL, 2, '{
    "max_members": 10, "max_products": 1000, "platform_ai_tokens_monthly": 1000000, "byok_allowed": true,
    "storage_bytes": 1073741824, "conversations_monthly": null, "custom_domain": false}');

CREATE TABLE subscriptions (
    tenant_id          uuid        PRIMARY KEY REFERENCES tenants (id) ON DELETE CASCADE,
    plan_code          text        NOT NULL REFERENCES plans (code) ON DELETE RESTRICT,
    -- trialing: uji coba; active: periode berbayar; expired: uji coba/periode habis (website & checkout mati).
    status             text        NOT NULL,
    trial_ends_at      timestamptz,
    current_period_end timestamptz,
    created_at         timestamptz NOT NULL DEFAULT now(),
    updated_at         timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT ck_subscriptions_status CHECK (status IN ('trialing', 'active', 'expired')),
    CONSTRAINT ck_subscriptions_trial CHECK (status <> 'trialing' OR trial_ends_at IS NOT NULL),
    CONSTRAINT ck_subscriptions_active CHECK (status <> 'active' OR current_period_end IS NOT NULL)
);
ALTER TABLE subscriptions ENABLE ROW LEVEL SECURITY;
ALTER TABLE subscriptions FORCE ROW LEVEL SECURITY;
CREATE POLICY tenant_isolation ON subscriptions
    USING (tenant_id = app.current_tenant_id()) WITH CHECK (tenant_id = app.current_tenant_id());
-- Status langganan hanya diubah lewat verifikasi pembayaran platform (P7-3), bukan oleh anggota tenant.
REVOKE INSERT, UPDATE, DELETE ON subscriptions FROM umkm_app_role;

-- Toko baru otomatis uji coba paket trial (SECURITY DEFINER: role aplikasi tidak bisa menulis subscriptions).
CREATE FUNCTION app.start_trial() RETURNS trigger
    LANGUAGE plpgsql SECURITY DEFINER
    SET search_path = pg_catalog, public
AS $$
BEGIN
    INSERT INTO public.subscriptions (tenant_id, plan_code, status, trial_ends_at)
    SELECT NEW.id, p.code, 'trialing', now() + make_interval(days => p.trial_days)
      FROM public.plans p WHERE p.trial_days IS NOT NULL;
    IF NOT FOUND THEN
        RAISE EXCEPTION 'tidak ada paket uji coba' USING ERRCODE = 'check_violation';
    END IF;
    RETURN NEW;
END $$;
REVOKE ALL ON FUNCTION app.start_trial() FROM PUBLIC;
CREATE TRIGGER trg_tenants_start_trial AFTER INSERT ON tenants
    FOR EACH ROW EXECUTE FUNCTION app.start_trial();

-- Tenant yang sudah ada: mulai uji coba dari sekarang.
INSERT INTO subscriptions (tenant_id, plan_code, status, trial_ends_at)
SELECT t.id, p.code, 'trialing', now() + make_interval(days => p.trial_days)
  FROM tenants t CROSS JOIN plans p WHERE p.trial_days IS NOT NULL;
