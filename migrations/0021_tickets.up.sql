-- P4-3: ticketing (PRD.md §4.8, FR-TKT-001..005). Lifecycle & SLA di Rust
-- (crates/domain/src/tickets.rs), dicerminkan trigger; riwayat append-only.

CREATE TABLE tickets (
    id                     uuid        NOT NULL DEFAULT gen_random_uuid(),
    tenant_id              uuid        NOT NULL REFERENCES tenants (id) ON DELETE CASCADE,
    code                   text        NOT NULL,
    source                 text        NOT NULL,
    customer_id            uuid,
    order_id               uuid,
    -- Conversation dibuat di P6 (entity berbeda; satu conversation → ≥1 ticket, FR-TKT-006).
    conversation_id        uuid,
    subject                text        NOT NULL,
    description            text,
    priority               text        NOT NULL DEFAULT 'normal',
    status                 text        NOT NULL DEFAULT 'OPEN',
    assignee_user_id       uuid,
    tags                   text[]      NOT NULL DEFAULT '{}',
    first_response_due_at  timestamptz NOT NULL,
    resolution_due_at      timestamptz NOT NULL,
    first_responded_at     timestamptz,
    resolved_at            timestamptz,
    closed_at              timestamptz,
    reopen_count           integer     NOT NULL DEFAULT 0,
    created_by_user_id     uuid,
    created_at             timestamptz NOT NULL DEFAULT now(),
    updated_at             timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (id),
    CONSTRAINT uq_tickets_tenant_id UNIQUE (tenant_id, id),
    CONSTRAINT uq_tickets_code UNIQUE (tenant_id, code),
    -- AC-TKT-001: rujukan sumber wajib milik tenant yang sama.
    CONSTRAINT fk_tickets_customer FOREIGN KEY (tenant_id, customer_id) REFERENCES customers (tenant_id, id) ON DELETE SET NULL (customer_id),
    CONSTRAINT fk_tickets_order FOREIGN KEY (tenant_id, order_id) REFERENCES orders (tenant_id, id) ON DELETE SET NULL (order_id),
    CONSTRAINT ck_tickets_source CHECK (source IN ('chat', 'order', 'manual', 'system')),
    CONSTRAINT ck_tickets_source_ref CHECK ((source <> 'order' OR order_id IS NOT NULL) AND (source <> 'chat' OR conversation_id IS NOT NULL)),
    CONSTRAINT ck_tickets_priority CHECK (priority IN ('low', 'normal', 'high', 'urgent')),
    CONSTRAINT ck_tickets_status CHECK (status IN ('OPEN', 'ASSIGNED', 'IN_PROGRESS', 'WAITING_CUSTOMER', 'WAITING_INTERNAL', 'RESOLVED', 'CLOSED')),
    CONSTRAINT ck_tickets_subject CHECK (length(btrim(subject)) BETWEEN 1 AND 200),
    CONSTRAINT ck_tickets_description CHECK (description IS NULL OR length(description) <= 5000),
    CONSTRAINT ck_tickets_tags CHECK (cardinality(tags) <= 10),
    -- AC-TKT-002: resolved_at terisi tepat saat RESOLVED/CLOSED.
    CONSTRAINT ck_tickets_resolved CHECK ((status IN ('RESOLVED', 'CLOSED')) = (resolved_at IS NOT NULL)),
    CONSTRAINT ck_tickets_closed CHECK ((status = 'CLOSED') = (closed_at IS NOT NULL)),
    CONSTRAINT ck_tickets_assigned CHECK (status <> 'ASSIGNED' OR assignee_user_id IS NOT NULL),
    CONSTRAINT ck_tickets_code CHECK (code ~ '^TKT-[A-Z0-9]{10}$')
);
CREATE INDEX ix_tickets_status ON tickets (tenant_id, status, updated_at DESC);
CREATE INDEX ix_tickets_assignee ON tickets (tenant_id, assignee_user_id);
CREATE INDEX ix_tickets_order ON tickets (tenant_id, order_id);
CREATE INDEX ix_tickets_customer ON tickets (tenant_id, customer_id);

-- AC-TKT-003: assignee wajib anggota aktif tenant yang sama.
CREATE FUNCTION app.check_ticket_assignee() RETURNS trigger
LANGUAGE plpgsql AS $$
BEGIN
    IF NEW.assignee_user_id IS NOT NULL AND (TG_OP = 'INSERT' OR NEW.assignee_user_id IS DISTINCT FROM OLD.assignee_user_id)
       AND NOT EXISTS (SELECT 1 FROM tenant_memberships m
                       WHERE m.tenant_id = NEW.tenant_id AND m.user_id = NEW.assignee_user_id AND m.status = 'active') THEN
        RAISE EXCEPTION 'assignee bukan anggota aktif tenant'
            USING ERRCODE = 'check_violation', CONSTRAINT = 'ck_tickets_assignee_member';
    END IF;
    NEW.updated_at := now();
    RETURN NEW;
END $$;
CREATE TRIGGER trg_tickets_assignee BEFORE INSERT OR UPDATE ON tickets
    FOR EACH ROW EXECUTE FUNCTION app.check_ticket_assignee();

-- Cermin TicketStatus::can_transition (paritas diuji).
CREATE FUNCTION app.ticket_transition_allowed(p_from text, p_to text) RETURNS boolean
    LANGUAGE sql IMMUTABLE AS $$
    SELECT CASE p_from
        WHEN 'OPEN' THEN p_to IN ('ASSIGNED', 'IN_PROGRESS', 'RESOLVED', 'CLOSED')
        WHEN 'ASSIGNED' THEN p_to IN ('OPEN', 'IN_PROGRESS', 'WAITING_CUSTOMER', 'WAITING_INTERNAL', 'RESOLVED')
        WHEN 'IN_PROGRESS' THEN p_to IN ('WAITING_CUSTOMER', 'WAITING_INTERNAL', 'RESOLVED')
        WHEN 'WAITING_CUSTOMER' THEN p_to IN ('IN_PROGRESS', 'RESOLVED', 'CLOSED')
        WHEN 'WAITING_INTERNAL' THEN p_to IN ('IN_PROGRESS', 'RESOLVED')
        WHEN 'RESOLVED' THEN p_to IN ('CLOSED', 'IN_PROGRESS')
        WHEN 'CLOSED' THEN p_to = 'IN_PROGRESS'
        ELSE false
    END
$$;
CREATE FUNCTION app.check_ticket_transition() RETURNS trigger
LANGUAGE plpgsql AS $$
BEGIN
    IF NEW.status IS DISTINCT FROM OLD.status AND NOT app.ticket_transition_allowed(OLD.status, NEW.status) THEN
        RAISE EXCEPTION 'transisi ticket % -> % tidak diizinkan', OLD.status, NEW.status
            USING ERRCODE = 'check_violation', CONSTRAINT = 'ck_tickets_transition';
    END IF;
    RETURN NEW;
END $$;
CREATE TRIGGER trg_tickets_transition BEFORE UPDATE ON tickets
    FOR EACH ROW EXECUTE FUNCTION app.check_ticket_transition();
CREATE FUNCTION app.check_ticket_initial() RETURNS trigger
LANGUAGE plpgsql AS $$
BEGIN
    IF NEW.status NOT IN ('OPEN', 'ASSIGNED') THEN
        RAISE EXCEPTION 'ticket baru harus OPEN/ASSIGNED' USING ERRCODE = 'check_violation', CONSTRAINT = 'ck_tickets_initial';
    END IF;
    RETURN NEW;
END $$;
CREATE TRIGGER trg_tickets_initial BEFORE INSERT ON tickets
    FOR EACH ROW EXECUTE FUNCTION app.check_ticket_initial();

CREATE TABLE ticket_messages (
    id             uuid        NOT NULL DEFAULT gen_random_uuid(),
    seq            bigint      GENERATED ALWAYS AS IDENTITY,
    tenant_id      uuid        NOT NULL REFERENCES tenants (id) ON DELETE CASCADE,
    ticket_id      uuid        NOT NULL,
    -- AC-TKT-004: internal note tidak pernah dikirim ke customer.
    visibility     text        NOT NULL,
    author_type    text        NOT NULL,
    author_user_id uuid,
    body           text        NOT NULL,
    created_at     timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (id),
    CONSTRAINT uq_ticket_messages_tenant_id UNIQUE (tenant_id, id),
    CONSTRAINT fk_ticket_messages_ticket FOREIGN KEY (tenant_id, ticket_id) REFERENCES tickets (tenant_id, id) ON DELETE CASCADE,
    CONSTRAINT ck_ticket_messages_visibility CHECK (visibility IN ('public', 'internal')),
    CONSTRAINT ck_ticket_messages_author CHECK (author_type IN ('staff', 'customer', 'system')
        AND (author_type <> 'staff' OR author_user_id IS NOT NULL)
        AND (author_type <> 'customer' OR visibility = 'public')),
    CONSTRAINT ck_ticket_messages_body CHECK (length(btrim(body)) BETWEEN 1 AND 5000)
);
CREATE INDEX ix_ticket_messages_ticket ON ticket_messages (tenant_id, ticket_id, seq);

CREATE TABLE ticket_attachments (
    id           uuid        NOT NULL DEFAULT gen_random_uuid(),
    tenant_id    uuid        NOT NULL REFERENCES tenants (id) ON DELETE CASCADE,
    ticket_id    uuid        NOT NULL,
    visibility   text        NOT NULL,
    content_type text        NOT NULL,
    byte_size    integer     NOT NULL,
    sha256       bytea       NOT NULL,
    bytes        bytea       NOT NULL,
    uploaded_by  text        NOT NULL,
    created_at   timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (id),
    CONSTRAINT fk_ticket_attachments_ticket FOREIGN KEY (tenant_id, ticket_id) REFERENCES tickets (tenant_id, id) ON DELETE CASCADE,
    CONSTRAINT ck_ticket_attachments_visibility CHECK (visibility IN ('public', 'internal')),
    CONSTRAINT ck_ticket_attachments_type CHECK (content_type IN ('image/png', 'image/jpeg', 'image/webp')),
    CONSTRAINT ck_ticket_attachments_size CHECK (byte_size > 0 AND byte_size = octet_length(bytes)),
    CONSTRAINT ck_ticket_attachments_uploader CHECK (uploaded_by IN ('staff', 'customer'))
);
ALTER TABLE ticket_attachments ALTER COLUMN bytes SET STORAGE EXTERNAL;
CREATE INDEX ix_ticket_attachments_ticket ON ticket_attachments (tenant_id, ticket_id);

CREATE TABLE ticket_events (
    id            uuid        NOT NULL DEFAULT gen_random_uuid(),
    seq           bigint      GENERATED ALWAYS AS IDENTITY,
    tenant_id     uuid        NOT NULL REFERENCES tenants (id) ON DELETE CASCADE,
    ticket_id     uuid        NOT NULL,
    kind          text        NOT NULL,
    data          jsonb       NOT NULL DEFAULT '{}',
    actor_type    text        NOT NULL,
    actor_user_id uuid,
    created_at    timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (id),
    CONSTRAINT fk_ticket_events_ticket FOREIGN KEY (tenant_id, ticket_id) REFERENCES tickets (tenant_id, id) ON DELETE CASCADE,
    CONSTRAINT ck_ticket_events_kind CHECK (kind ~ '^[a-z_]+$'),
    CONSTRAINT ck_ticket_events_actor CHECK (actor_type IN ('staff', 'customer', 'system'))
);
CREATE INDEX ix_ticket_events_ticket ON ticket_events (tenant_id, ticket_id, seq);

DO $$
DECLARE t text;
BEGIN
    FOREACH t IN ARRAY ARRAY['tickets', 'ticket_messages', 'ticket_attachments', 'ticket_events'] LOOP
        EXECUTE format('ALTER TABLE %I ENABLE ROW LEVEL SECURITY', t);
        EXECUTE format('ALTER TABLE %I FORCE ROW LEVEL SECURITY', t);
        EXECUTE format('CREATE POLICY tenant_isolation ON %I USING (tenant_id = app.current_tenant_id()) WITH CHECK (tenant_id = app.current_tenant_id())', t);
    END LOOP;
END $$;
-- AC-TKT-005: riwayat & pesan append-only; ticket tidak dihapus aplikasi.
REVOKE UPDATE, DELETE ON ticket_events, ticket_messages FROM umkm_app_role;
REVOKE UPDATE ON ticket_attachments FROM umkm_app_role;
REVOKE DELETE ON tickets FROM umkm_app_role;
