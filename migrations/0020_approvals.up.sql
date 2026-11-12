-- P4-2: approval engine reusable (PRD.md §4.9, FR-APR-001..003). Aturan keputusan di Rust
-- (crates/domain/src/approvals.rs); DB menjaga bentuk, kepemilikan, dan riwayat append-only.

CREATE TABLE approval_policies (
    tenant_id     uuid        NOT NULL REFERENCES tenants (id) ON DELETE CASCADE,
    entity_type   text        NOT NULL,
    enabled       boolean     NOT NULL DEFAULT true,
    -- [{role_ids:[], user_ids:[]}] 1..5 langkah berurutan
    steps         jsonb       NOT NULL,
    timeout_hours integer     NOT NULL DEFAULT 48,
    updated_at    timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (tenant_id, entity_type),
    CONSTRAINT ck_approval_policies_type CHECK (entity_type IN
        ('order', 'refund', 'stock_adjustment', 'price_change', 'ai_response', 'business_config')),
    CONSTRAINT ck_approval_policies_steps CHECK (jsonb_typeof(steps) = 'array' AND jsonb_array_length(steps) BETWEEN 1 AND 5),
    CONSTRAINT ck_approval_policies_timeout CHECK (timeout_hours BETWEEN 1 AND 720)
);

CREATE TABLE approval_requests (
    id              uuid        NOT NULL DEFAULT gen_random_uuid(),
    tenant_id       uuid        NOT NULL REFERENCES tenants (id) ON DELETE CASCADE,
    entity_type     text        NOT NULL,
    entity_id       uuid,
    -- Perubahan yang diusulkan (diterapkan executor setelah langkah terakhir disetujui)
    action          text        NOT NULL,
    payload         jsonb       NOT NULL DEFAULT '{}',
    summary         text        NOT NULL,
    status          text        NOT NULL DEFAULT 'pending',
    current_step    integer     NOT NULL DEFAULT 0,
    steps           jsonb       NOT NULL,
    requested_by    uuid        NOT NULL REFERENCES users (id) ON DELETE RESTRICT,
    expires_at      timestamptz NOT NULL,
    decided_at      timestamptz,
    executed_at     timestamptz,
    execution_error text,
    created_at      timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (id),
    CONSTRAINT uq_approval_requests_tenant_id UNIQUE (tenant_id, id),
    CONSTRAINT ck_approval_requests_type CHECK (entity_type IN
        ('order', 'refund', 'stock_adjustment', 'price_change', 'ai_response', 'business_config')),
    CONSTRAINT ck_approval_requests_status CHECK (status IN ('pending', 'approved', 'rejected', 'expired', 'cancelled')),
    CONSTRAINT ck_approval_requests_step CHECK (current_step >= 0 AND current_step < jsonb_array_length(steps) + 1),
    CONSTRAINT ck_approval_requests_decided CHECK ((status = 'pending') = (decided_at IS NULL)),
    CONSTRAINT ck_approval_requests_action CHECK (action ~ '^[a-z_]+\.[a-z_]+$'),
    CONSTRAINT ck_approval_requests_summary CHECK (length(summary) BETWEEN 1 AND 300),
    CONSTRAINT ck_approval_requests_payload CHECK (jsonb_typeof(payload) = 'object' AND octet_length(payload::text) <= 65536)
);
-- Satu permintaan tertunda per entity + aksi (mencegah antrean ganda).
CREATE UNIQUE INDEX uq_approval_requests_pending ON approval_requests (tenant_id, entity_type, COALESCE(entity_id, '00000000-0000-0000-0000-000000000000'::uuid), action)
    WHERE status = 'pending';
CREATE INDEX ix_approval_requests_status ON approval_requests (tenant_id, status, created_at DESC);
CREATE INDEX ix_approval_requests_expiry ON approval_requests (expires_at) WHERE status = 'pending';

CREATE TABLE approval_decisions (
    id                uuid        NOT NULL DEFAULT gen_random_uuid(),
    seq               bigint      GENERATED ALWAYS AS IDENTITY,
    tenant_id         uuid        NOT NULL REFERENCES tenants (id) ON DELETE CASCADE,
    request_id        uuid        NOT NULL,
    step              integer,
    decision          text        NOT NULL,
    actor_user_id     uuid,
    on_behalf_user_id uuid,
    reason            text,
    created_at        timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (id),
    CONSTRAINT fk_approval_decisions_request FOREIGN KEY (tenant_id, request_id)
        REFERENCES approval_requests (tenant_id, id) ON DELETE CASCADE,
    CONSTRAINT ck_approval_decisions_decision CHECK (decision IN ('requested', 'approved', 'rejected', 'expired', 'cancelled', 'executed', 'execution_failed')),
    CONSTRAINT ck_approval_decisions_reason CHECK (reason IS NULL OR length(reason) <= 500)
);
CREATE INDEX ix_approval_decisions_request ON approval_decisions (tenant_id, request_id, seq);
-- Satu keputusan per langkah (pertahanan kedua di samping FOR UPDATE): keputusan paralel ganda mustahil.
CREATE UNIQUE INDEX uq_approval_decisions_step ON approval_decisions (tenant_id, request_id, step)
    WHERE decision IN ('approved', 'rejected');

-- Delegasi: `to_user` boleh memutus atas nama `from_user` selama berlaku.
CREATE TABLE approval_delegations (
    id         uuid        NOT NULL DEFAULT gen_random_uuid(),
    tenant_id  uuid        NOT NULL REFERENCES tenants (id) ON DELETE CASCADE,
    from_user  uuid        NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    to_user    uuid        NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    starts_at  timestamptz NOT NULL DEFAULT now(),
    ends_at    timestamptz NOT NULL,
    revoked_at timestamptz,
    created_at timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (id),
    CONSTRAINT ck_approval_delegations_users CHECK (from_user <> to_user),
    CONSTRAINT ck_approval_delegations_window CHECK (ends_at > starts_at AND ends_at <= starts_at + interval '90 days')
);
CREATE INDEX ix_approval_delegations_to ON approval_delegations (tenant_id, to_user) WHERE revoked_at IS NULL;

DO $$
DECLARE t text;
BEGIN
    FOREACH t IN ARRAY ARRAY['approval_policies', 'approval_requests', 'approval_decisions', 'approval_delegations'] LOOP
        EXECUTE format('ALTER TABLE %I ENABLE ROW LEVEL SECURITY', t);
        EXECUTE format('ALTER TABLE %I FORCE ROW LEVEL SECURITY', t);
        EXECUTE format('CREATE POLICY tenant_isolation ON %I USING (tenant_id = app.current_tenant_id()) WITH CHECK (tenant_id = app.current_tenant_id())', t);
    END LOOP;
END $$;
REVOKE UPDATE, DELETE ON approval_decisions FROM umkm_app_role;
REVOKE DELETE ON approval_requests FROM umkm_app_role;

-- FR-APR-002 timeout: permintaan tertunda yang lewat batas → expired (lintas tenant, worker).
CREATE FUNCTION app.expire_approvals(p_batch integer) RETURNS integer
    LANGUAGE plpgsql SECURITY DEFINER
    SET search_path = pg_catalog, public
AS $$
DECLARE r record; n integer := 0;
BEGIN
    FOR r IN
        SELECT id, tenant_id, current_step FROM public.approval_requests
        WHERE status = 'pending' AND expires_at <= now()
        ORDER BY expires_at LIMIT greatest(1, least(p_batch, 1000))
        FOR UPDATE SKIP LOCKED
    LOOP
        UPDATE public.approval_requests SET status = 'expired', decided_at = now() WHERE id = r.id;
        INSERT INTO public.approval_decisions (tenant_id, request_id, step, decision, reason)
            VALUES (r.tenant_id, r.id, r.current_step, 'expired', 'Batas waktu persetujuan habis');
        n := n + 1;
    END LOOP;
    RETURN n;
END $$;
REVOKE ALL ON FUNCTION app.expire_approvals(integer) FROM PUBLIC;
GRANT EXECUTE ON FUNCTION app.expire_approvals(integer) TO umkm_app_role;
