-- P6-1: conversation / session / message (PRD.md §4.12, FR-CNV-001, ADR-0006 web chat).
-- UUID terpisah per conversation, session, dan message; 4 sender type. Lifecycle di Rust
-- (crates/domain/src/conversation.rs), dicerminkan trigger (paritas diuji). Pesan & riwayat append-only.
-- Retensi ADR-0012: 30 hari setelah pesan terakhir (app.purge_conversations).

CREATE TABLE conversations (
    id                uuid        NOT NULL DEFAULT gen_random_uuid(),
    tenant_id         uuid        NOT NULL REFERENCES tenants (id) ON DELETE CASCADE,
    -- ADR-0006: channel baru = nilai baru + adapter, tanpa mengubah skema pesan.
    channel           text        NOT NULL DEFAULT 'web',
    status            text        NOT NULL DEFAULT 'AI',
    customer_id       uuid,
    assignee_user_id  uuid,
    -- D-12: pemberitahuan AI ditampilkan & disetujui sebelum pesan pertama.
    consent_version   text        NOT NULL,
    consented_at      timestamptz NOT NULL,
    -- Analitik (P6-6): pernah ditangani manusia → dihitung human-resolved saat ditutup.
    human_involved    boolean     NOT NULL DEFAULT false,
    last_message_at   timestamptz NOT NULL DEFAULT now(),
    closed_at         timestamptz,
    created_at        timestamptz NOT NULL DEFAULT now(),
    updated_at        timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (id),
    CONSTRAINT uq_conversations_tenant_id UNIQUE (tenant_id, id),
    CONSTRAINT fk_conversations_customer FOREIGN KEY (tenant_id, customer_id)
        REFERENCES customers (tenant_id, id) ON DELETE SET NULL (customer_id),
    CONSTRAINT ck_conversations_channel CHECK (channel IN ('web')),
    CONSTRAINT ck_conversations_status CHECK (status IN ('AI', 'WAITING_HUMAN', 'HUMAN', 'CLOSED')),
    CONSTRAINT ck_conversations_closed CHECK ((status = 'CLOSED') = (closed_at IS NOT NULL)),
    CONSTRAINT ck_conversations_human CHECK (status <> 'HUMAN' OR (assignee_user_id IS NOT NULL AND human_involved)),
    CONSTRAINT ck_conversations_consent CHECK (consent_version ~ '^[a-z0-9.-]{1,40}$')
);
CREATE INDEX ix_conversations_inbox ON conversations (tenant_id, status, last_message_at DESC);
CREATE INDEX ix_conversations_assignee ON conversations (tenant_id, assignee_user_id);
CREATE INDEX ix_conversations_customer ON conversations (tenant_id, customer_id);
CREATE INDEX ix_conversations_retention ON conversations (last_message_at);

-- Assignee wajib anggota aktif tenant yang sama.
CREATE FUNCTION app.check_conversation_assignee() RETURNS trigger
LANGUAGE plpgsql AS $$
BEGIN
    IF NEW.assignee_user_id IS NOT NULL AND (TG_OP = 'INSERT' OR NEW.assignee_user_id IS DISTINCT FROM OLD.assignee_user_id)
       AND NOT EXISTS (SELECT 1 FROM tenant_memberships m
                       WHERE m.tenant_id = NEW.tenant_id AND m.user_id = NEW.assignee_user_id AND m.status = 'active') THEN
        RAISE EXCEPTION 'assignee bukan anggota aktif tenant'
            USING ERRCODE = 'check_violation', CONSTRAINT = 'ck_conversations_assignee_member';
    END IF;
    NEW.updated_at := now();
    RETURN NEW;
END $$;
CREATE TRIGGER trg_conversations_assignee BEFORE INSERT OR UPDATE ON conversations
    FOR EACH ROW EXECUTE FUNCTION app.check_conversation_assignee();

-- Cermin ConversationStatus::can_transition (paritas diuji).
CREATE FUNCTION app.conversation_transition_allowed(p_from text, p_to text) RETURNS boolean
    LANGUAGE sql IMMUTABLE AS $$
    SELECT CASE p_from
        WHEN 'AI' THEN p_to IN ('WAITING_HUMAN', 'HUMAN', 'CLOSED')
        WHEN 'WAITING_HUMAN' THEN p_to IN ('HUMAN', 'AI', 'CLOSED')
        WHEN 'HUMAN' THEN p_to IN ('AI', 'WAITING_HUMAN', 'CLOSED')
        ELSE false
    END
$$;
CREATE FUNCTION app.check_conversation_status() RETURNS trigger
LANGUAGE plpgsql AS $$
BEGIN
    IF TG_OP = 'INSERT' THEN
        IF NEW.status NOT IN ('AI', 'WAITING_HUMAN') THEN
            RAISE EXCEPTION 'conversation baru harus AI/WAITING_HUMAN'
                USING ERRCODE = 'check_violation', CONSTRAINT = 'ck_conversations_initial';
        END IF;
    ELSIF NEW.status IS DISTINCT FROM OLD.status AND NOT app.conversation_transition_allowed(OLD.status, NEW.status) THEN
        RAISE EXCEPTION 'transisi conversation % -> % tidak diizinkan', OLD.status, NEW.status
            USING ERRCODE = 'check_violation', CONSTRAINT = 'ck_conversations_transition';
    END IF;
    RETURN NEW;
END $$;
CREATE TRIGGER trg_conversations_status BEFORE INSERT OR UPDATE ON conversations
    FOR EACH ROW EXECUTE FUNCTION app.check_conversation_status();

-- Sesi chat anonim (ADR-0006): token acak per percakapan, hanya hash disimpan.
CREATE TABLE conversation_sessions (
    id               uuid        NOT NULL DEFAULT gen_random_uuid(),
    tenant_id        uuid        NOT NULL REFERENCES tenants (id) ON DELETE CASCADE,
    conversation_id  uuid        NOT NULL,
    token_hash       bytea       NOT NULL,
    created_at       timestamptz NOT NULL DEFAULT now(),
    last_seen_at     timestamptz NOT NULL DEFAULT now(),
    expires_at       timestamptz NOT NULL,
    revoked_at       timestamptz,
    PRIMARY KEY (id),
    CONSTRAINT uq_conversation_sessions_tenant_id UNIQUE (tenant_id, id),
    CONSTRAINT fk_conversation_sessions_conv FOREIGN KEY (tenant_id, conversation_id)
        REFERENCES conversations (tenant_id, id) ON DELETE CASCADE,
    CONSTRAINT ck_conversation_sessions_hash CHECK (octet_length(token_hash) = 32),
    CONSTRAINT ck_conversation_sessions_expiry CHECK (expires_at > created_at)
);
CREATE UNIQUE INDEX uq_conversation_sessions_token ON conversation_sessions (tenant_id, token_hash);
CREATE INDEX ix_conversation_sessions_conv ON conversation_sessions (tenant_id, conversation_id);

CREATE TABLE conversation_messages (
    id               uuid        NOT NULL DEFAULT gen_random_uuid(),
    seq              bigint      GENERATED ALWAYS AS IDENTITY,
    tenant_id        uuid        NOT NULL REFERENCES tenants (id) ON DELETE CASCADE,
    conversation_id  uuid        NOT NULL,
    session_id       uuid,
    -- AC-CNV-001: tepat 4 sender type.
    sender_type      text        NOT NULL,
    sender_user_id   uuid,
    body             text        NOT NULL,
    -- Jejak AI (viewer P6-3: provider/model, token, latency, tool summary lewat ai_runs).
    ai_run_id        uuid,
    created_at       timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (id),
    CONSTRAINT uq_conversation_messages_tenant_id UNIQUE (tenant_id, id),
    CONSTRAINT fk_conversation_messages_conv FOREIGN KEY (tenant_id, conversation_id)
        REFERENCES conversations (tenant_id, id) ON DELETE CASCADE,
    CONSTRAINT fk_conversation_messages_session FOREIGN KEY (tenant_id, session_id)
        REFERENCES conversation_sessions (tenant_id, id) ON DELETE SET NULL (session_id),
    CONSTRAINT fk_conversation_messages_run FOREIGN KEY (tenant_id, ai_run_id)
        REFERENCES ai_runs (tenant_id, id) ON DELETE SET NULL (ai_run_id),
    CONSTRAINT ck_conversation_messages_sender CHECK (sender_type IN ('CUSTOMER', 'AI', 'ADMIN', 'SYSTEM')),
    CONSTRAINT ck_conversation_messages_sender_ref CHECK (
        (sender_type = 'ADMIN') = (sender_user_id IS NOT NULL)
        AND (sender_type = 'AI' OR ai_run_id IS NULL)),
    CONSTRAINT ck_conversation_messages_body CHECK (length(btrim(body)) BETWEEN 1 AND 4000)
);
CREATE INDEX ix_conversation_messages_conv ON conversation_messages (tenant_id, conversation_id, seq);

-- Pesan hanya masuk ke percakapan terbuka; last_message_at mengikuti pesan (dasar retensi).
CREATE FUNCTION app.on_conversation_message() RETURNS trigger
LANGUAGE plpgsql AS $$
BEGIN
    UPDATE conversations SET last_message_at = NEW.created_at, updated_at = now()
        WHERE tenant_id = NEW.tenant_id AND id = NEW.conversation_id AND status <> 'CLOSED';
    IF NOT FOUND THEN
        RAISE EXCEPTION 'conversation tertutup atau tidak ada'
            USING ERRCODE = 'check_violation', CONSTRAINT = 'ck_conversation_messages_open';
    END IF;
    RETURN NEW;
END $$;
CREATE TRIGGER trg_conversation_messages_touch BEFORE INSERT ON conversation_messages
    FOR EACH ROW EXECUTE FUNCTION app.on_conversation_message();

-- Riwayat assignment & handover (viewer P6-3).
CREATE TABLE conversation_events (
    id               uuid        NOT NULL DEFAULT gen_random_uuid(),
    seq              bigint      GENERATED ALWAYS AS IDENTITY,
    tenant_id        uuid        NOT NULL REFERENCES tenants (id) ON DELETE CASCADE,
    conversation_id  uuid        NOT NULL,
    kind             text        NOT NULL,
    from_status      text,
    to_status        text,
    data             jsonb       NOT NULL DEFAULT '{}',
    actor_type       text        NOT NULL,
    actor_user_id    uuid,
    created_at       timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (id),
    CONSTRAINT fk_conversation_events_conv FOREIGN KEY (tenant_id, conversation_id)
        REFERENCES conversations (tenant_id, id) ON DELETE CASCADE,
    CONSTRAINT ck_conversation_events_kind CHECK (kind IN (
        'created', 'assigned', 'unassigned', 'handover_requested', 'taken_over', 'released', 'closed')),
    CONSTRAINT ck_conversation_events_actor CHECK (actor_type IN ('customer', 'ai', 'staff', 'system')
        AND (actor_type <> 'staff' OR actor_user_id IS NOT NULL))
);
CREATE INDEX ix_conversation_events_conv ON conversation_events (tenant_id, conversation_id, seq);

-- Rujukan conversation yang selama ini menunggu P6.
ALTER TABLE ai_runs ADD CONSTRAINT fk_ai_runs_conversation FOREIGN KEY (tenant_id, conversation_id)
    REFERENCES conversations (tenant_id, id) ON DELETE SET NULL (conversation_id);
CREATE INDEX ix_ai_runs_conversation ON ai_runs (tenant_id, conversation_id);
-- Ticket tetap ada setelah percakapan dipurge (ticket != conversation, FR-TKT-006): rujukan chat
-- wajib saat dibuat (trigger), bukan CHECK permanen.
ALTER TABLE tickets DROP CONSTRAINT ck_tickets_source_ref;
ALTER TABLE tickets ADD CONSTRAINT ck_tickets_order_ref CHECK (source <> 'order' OR order_id IS NOT NULL);
ALTER TABLE tickets ADD CONSTRAINT fk_tickets_conversation FOREIGN KEY (tenant_id, conversation_id)
    REFERENCES conversations (tenant_id, id) ON DELETE SET NULL (conversation_id);
CREATE INDEX ix_tickets_conversation ON tickets (tenant_id, conversation_id);
CREATE FUNCTION app.check_ticket_chat_ref() RETURNS trigger
LANGUAGE plpgsql AS $$
BEGIN
    IF NEW.source = 'chat' AND NEW.conversation_id IS NULL THEN
        RAISE EXCEPTION 'ticket dari chat wajib merujuk conversation'
            USING ERRCODE = 'check_violation', CONSTRAINT = 'ck_tickets_chat_ref';
    END IF;
    RETURN NEW;
END $$;
CREATE TRIGGER trg_tickets_chat_ref BEFORE INSERT ON tickets
    FOR EACH ROW EXECUTE FUNCTION app.check_ticket_chat_ref();

DO $$
DECLARE t text;
BEGIN
    FOREACH t IN ARRAY ARRAY['conversations', 'conversation_sessions', 'conversation_messages', 'conversation_events'] LOOP
        EXECUTE format('ALTER TABLE %I ENABLE ROW LEVEL SECURITY', t);
        EXECUTE format('ALTER TABLE %I FORCE ROW LEVEL SECURITY', t);
        EXECUTE format('CREATE POLICY tenant_isolation ON %I USING (tenant_id = app.current_tenant_id()) WITH CHECK (tenant_id = app.current_tenant_id())', t);
    END LOOP;
END $$;
-- Append-only; percakapan hanya dihapus lewat purge retensi.
REVOKE UPDATE, DELETE ON conversation_messages, conversation_events FROM umkm_app_role;
REVOKE DELETE ON conversations, conversation_sessions FROM umkm_app_role;

-- ADR-0012: 30 hari setelah pesan terakhir. Batas bawah 7 hari mencegah salah konfigurasi.
CREATE FUNCTION app.purge_conversations(p_before timestamptz) RETURNS bigint
    LANGUAGE plpgsql SECURITY DEFINER
    SET search_path = pg_catalog, public
AS $$
DECLARE n bigint;
BEGIN
    IF p_before > now() - interval '7 days' THEN
        RAISE EXCEPTION 'purge_conversations: cutoff % terlalu baru (minimal 7 hari)', p_before
            USING ERRCODE = 'invalid_parameter_value';
    END IF;
    DELETE FROM public.conversations WHERE last_message_at < p_before;
    GET DIAGNOSTICS n = ROW_COUNT;
    RETURN n;
END $$;
REVOKE ALL ON FUNCTION app.purge_conversations(timestamptz) FROM PUBLIC;
GRANT EXECUTE ON FUNCTION app.purge_conversations(timestamptz) TO umkm_app_role;
