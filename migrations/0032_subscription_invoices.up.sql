-- P7-3: tagihan langganan + pembayaran manual (PRD.md §4.15, ADR-0016). Toko membuat tagihan & mengunggah bukti;
-- platform admin (super_admin, sesi lolos MFA — diperiksa API) memverifikasi lewat fungsi SECURITY DEFINER sempit,
-- satu-satunya jalur lintas tenant. Langganan hanya berubah di dalam fungsi tersebut.

CREATE TABLE subscription_invoices (
    id                 uuid        NOT NULL DEFAULT gen_random_uuid(),
    tenant_id          uuid        NOT NULL REFERENCES tenants (id) ON DELETE CASCADE,
    code               text        NOT NULL,
    plan_code          text        NOT NULL REFERENCES plans (code) ON DELETE RESTRICT,
    -- Snapshot harga & periode saat tagihan dibuat.
    amount_idr         integer     NOT NULL,
    period_days        integer     NOT NULL,
    status             text        NOT NULL DEFAULT 'awaiting_payment',
    proof_content_type text,
    proof_bytes        bytea,
    proof_sha256       bytea,
    submitted_at       timestamptz,
    decided_at         timestamptz,
    decided_by         uuid REFERENCES users (id) ON DELETE SET NULL,
    rejection_reason   text,
    period_start       timestamptz,
    period_end         timestamptz,
    created_by         uuid REFERENCES users (id) ON DELETE SET NULL,
    created_at         timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (id),
    CONSTRAINT uq_subscription_invoices_tenant_id UNIQUE (tenant_id, id),
    CONSTRAINT uq_subscription_invoices_code UNIQUE (code),
    CONSTRAINT ck_si_code CHECK (code ~ '^INV-[A-Z0-9]{10}$'),
    CONSTRAINT ck_si_status CHECK (status IN ('awaiting_payment', 'submitted', 'paid', 'rejected', 'cancelled')),
    CONSTRAINT ck_si_amount CHECK (amount_idr >= 0 AND period_days BETWEEN 1 AND 366),
    CONSTRAINT ck_si_proof CHECK ((proof_bytes IS NULL) = (proof_content_type IS NULL)
        AND (proof_content_type IS NULL OR proof_content_type IN ('image/png', 'image/jpeg', 'image/webp'))
        AND (proof_bytes IS NULL OR octet_length(proof_bytes) BETWEEN 1 AND 2097152)),
    CONSTRAINT ck_si_submitted CHECK (status NOT IN ('submitted', 'paid') OR (proof_bytes IS NOT NULL AND submitted_at IS NOT NULL)),
    CONSTRAINT ck_si_decided CHECK ((status IN ('paid', 'rejected')) = (decided_at IS NOT NULL)),
    CONSTRAINT ck_si_paid CHECK (status <> 'paid' OR (period_start IS NOT NULL AND period_end > period_start)),
    CONSTRAINT ck_si_reason CHECK (status <> 'rejected' OR length(btrim(coalesce(rejection_reason, ''))) BETWEEN 1 AND 300)
);
ALTER TABLE subscription_invoices ALTER COLUMN proof_bytes SET STORAGE EXTERNAL;
-- Satu tagihan terbuka per tenant.
CREATE UNIQUE INDEX uq_si_open ON subscription_invoices (tenant_id) WHERE status IN ('awaiting_payment', 'submitted');
CREATE INDEX ix_si_tenant ON subscription_invoices (tenant_id, created_at DESC);
CREATE INDEX ix_si_queue ON subscription_invoices (status, submitted_at);

ALTER TABLE subscription_invoices ENABLE ROW LEVEL SECURITY;
ALTER TABLE subscription_invoices FORCE ROW LEVEL SECURITY;
CREATE POLICY tenant_isolation ON subscription_invoices
    USING (tenant_id = app.current_tenant_id()) WITH CHECK (tenant_id = app.current_tenant_id());
REVOKE DELETE ON subscription_invoices FROM umkm_app_role;

-- Toko tidak dapat menandai tagihannya sendiri lunas: keputusan hanya lewat app.platform_decide_invoice.
CREATE FUNCTION app.guard_invoice_update() RETURNS trigger
LANGUAGE plpgsql AS $$
BEGIN
    IF current_setting('app.platform_decision', true) IS DISTINCT FROM 'on' AND (
        NEW.status IN ('paid', 'rejected') OR NEW.decided_at IS DISTINCT FROM OLD.decided_at
        OR NEW.decided_by IS DISTINCT FROM OLD.decided_by OR NEW.period_start IS DISTINCT FROM OLD.period_start
        OR NEW.period_end IS DISTINCT FROM OLD.period_end OR NEW.amount_idr IS DISTINCT FROM OLD.amount_idr
        OR NEW.plan_code IS DISTINCT FROM OLD.plan_code OR NEW.period_days IS DISTINCT FROM OLD.period_days
        OR OLD.status IN ('paid', 'rejected', 'cancelled')) THEN
        RAISE EXCEPTION 'perubahan tagihan ini hanya oleh platform'
            USING ERRCODE = 'insufficient_privilege', CONSTRAINT = 'ck_si_platform_only';
    END IF;
    RETURN NEW;
END $$;
CREATE TRIGGER trg_si_guard BEFORE UPDATE ON subscription_invoices
    FOR EACH ROW EXECUTE FUNCTION app.guard_invoice_update();
CREATE FUNCTION app.guard_invoice_insert() RETURNS trigger
LANGUAGE plpgsql AS $$
BEGIN
    IF NEW.status <> 'awaiting_payment' OR NEW.decided_at IS NOT NULL OR NEW.period_start IS NOT NULL THEN
        RAISE EXCEPTION 'tagihan baru harus menunggu pembayaran'
            USING ERRCODE = 'insufficient_privilege', CONSTRAINT = 'ck_si_platform_only';
    END IF;
    -- Harga & periode selalu dari katalog, bukan dari klien.
    SELECT price_idr, period_days INTO NEW.amount_idr, NEW.period_days FROM plans WHERE code = NEW.plan_code AND active;
    IF NOT FOUND THEN
        RAISE EXCEPTION 'paket tidak tersedia' USING ERRCODE = 'check_violation', CONSTRAINT = 'ck_si_plan_active';
    END IF;
    RETURN NEW;
END $$;
CREATE TRIGGER trg_si_insert BEFORE INSERT ON subscription_invoices
    FOR EACH ROW EXECUTE FUNCTION app.guard_invoice_insert();

-- Antrean platform (lintas tenant, tanpa isi bukti).
CREATE FUNCTION app.platform_list_invoices(p_status text)
    RETURNS TABLE (id uuid, tenant_id uuid, tenant_name text, tenant_slug text, code text, plan_code text,
                   amount_idr integer, period_days integer, status text, submitted_at timestamptz,
                   decided_at timestamptz, rejection_reason text, created_at timestamptz)
    LANGUAGE sql STABLE SECURITY DEFINER
    SET search_path = pg_catalog, public
AS $$
    SELECT i.id, i.tenant_id, t.name, t.slug, i.code, i.plan_code, i.amount_idr, i.period_days, i.status,
           i.submitted_at, i.decided_at, i.rejection_reason, i.created_at
      FROM subscription_invoices i JOIN tenants t ON t.id = i.tenant_id
     WHERE p_status IS NULL OR i.status = p_status
     ORDER BY coalesce(i.submitted_at, i.created_at)
     LIMIT 200
$$;

CREATE FUNCTION app.platform_invoice_proof(p_id uuid)
    RETURNS TABLE (content_type text, bytes bytea)
    LANGUAGE sql STABLE SECURITY DEFINER
    SET search_path = pg_catalog, public
AS $$
    SELECT proof_content_type, proof_bytes FROM subscription_invoices WHERE id = p_id AND proof_bytes IS NOT NULL
$$;

-- Keputusan atomik: tagihan 'submitted' → paid (langganan aktif/diperpanjang) atau rejected.
-- Perpanjangan: paket sama & masih aktif → dari akhir periode berjalan; selain itu mulai sekarang.
CREATE FUNCTION app.platform_decide_invoice(p_id uuid, p_actor uuid, p_approve boolean, p_reason text)
    RETURNS TABLE (tenant_id uuid, status text, period_end timestamptz)
    LANGUAGE plpgsql SECURITY DEFINER
    SET search_path = pg_catalog, public
AS $$
DECLARE inv record; sub record; v_start timestamptz; v_end timestamptz;
BEGIN
    SELECT * INTO inv FROM subscription_invoices i WHERE i.id = p_id FOR UPDATE;
    IF NOT FOUND THEN
        RAISE EXCEPTION 'tagihan tidak ada' USING ERRCODE = 'no_data_found';
    END IF;
    IF inv.status <> 'submitted' THEN
        RAISE EXCEPTION 'tagihan tidak menunggu verifikasi' USING ERRCODE = 'check_violation', CONSTRAINT = 'ck_si_not_submitted';
    END IF;
    PERFORM set_config('app.platform_decision', 'on', true);
    PERFORM set_config('app.tenant_id', inv.tenant_id::text, true);
    IF p_approve THEN
        SELECT * INTO sub FROM subscriptions s WHERE s.tenant_id = inv.tenant_id FOR UPDATE;
        v_start := now();
        IF sub.status = 'active' AND sub.plan_code = inv.plan_code AND sub.current_period_end > now() THEN
            v_start := sub.current_period_end;
        END IF;
        v_end := v_start + make_interval(days => inv.period_days);
        UPDATE subscriptions s SET plan_code = inv.plan_code, status = 'active', current_period_end = v_end,
               updated_at = now()
         WHERE s.tenant_id = inv.tenant_id;
        UPDATE subscription_invoices i SET status = 'paid', decided_at = now(), decided_by = p_actor,
               period_start = v_start, period_end = v_end
         WHERE i.id = p_id;
    ELSE
        UPDATE subscription_invoices i SET status = 'rejected', decided_at = now(), decided_by = p_actor,
               rejection_reason = p_reason
         WHERE i.id = p_id;
    END IF;
    PERFORM set_config('app.platform_decision', 'off', true);
    RETURN QUERY SELECT inv.tenant_id, CASE WHEN p_approve THEN 'paid' ELSE 'rejected' END, v_end;
END $$;

REVOKE ALL ON FUNCTION app.platform_list_invoices(text), app.platform_invoice_proof(uuid),
    app.platform_decide_invoice(uuid, uuid, boolean, text) FROM PUBLIC;
GRANT EXECUTE ON FUNCTION app.platform_list_invoices(text), app.platform_invoice_proof(uuid),
    app.platform_decide_invoice(uuid, uuid, boolean, text) TO umkm_app_role;
