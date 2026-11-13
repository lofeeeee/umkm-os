-- P6-6: analitik percakapan (PRD.md §4.12, FR-CNV-004). ADR-0012: isi percakapan dipurge 30 hari setelah pesan
-- terakhir → analitik memakai agregat harian TERPISAH (tanpa FK ke conversations) yang diisi trigger di
-- transaksi yang sama dengan perubahannya. Hari = tanggal Asia/Jakarta (sama dengan meter pemakaian AI).

ALTER TABLE conversations
    ADD COLUMN first_customer_at timestamptz,
    ADD COLUMN first_response_at timestamptz,
    ADD COLUMN handed_over       boolean NOT NULL DEFAULT false;

CREATE TABLE conversation_daily_stats (
    tenant_id              uuid    NOT NULL REFERENCES tenants (id) ON DELETE CASCADE,
    day                    date    NOT NULL,
    started                integer NOT NULL DEFAULT 0,
    -- percakapan pertama kali dialihkan ke staf (handover dari AI / permintaan pelanggan)
    handed_over            integer NOT NULL DEFAULT 0,
    closed_ai              integer NOT NULL DEFAULT 0,
    closed_human           integer NOT NULL DEFAULT 0,
    first_response_count   integer NOT NULL DEFAULT 0,
    first_response_ms_sum  bigint  NOT NULL DEFAULT 0,
    resolution_count       integer NOT NULL DEFAULT 0,
    resolution_ms_sum      bigint  NOT NULL DEFAULT 0,
    PRIMARY KEY (tenant_id, day),
    CONSTRAINT ck_cds_nonneg CHECK (started >= 0 AND handed_over >= 0 AND closed_ai >= 0 AND closed_human >= 0
        AND first_response_count >= 0 AND first_response_ms_sum >= 0 AND resolution_count >= 0 AND resolution_ms_sum >= 0)
);
ALTER TABLE conversation_daily_stats ENABLE ROW LEVEL SECURITY;
ALTER TABLE conversation_daily_stats FORCE ROW LEVEL SECURITY;
CREATE POLICY tenant_isolation ON conversation_daily_stats
    USING (tenant_id = app.current_tenant_id()) WITH CHECK (tenant_id = app.current_tenant_id());
-- Hanya trigger yang menulis; aplikasi membaca.
REVOKE DELETE ON conversation_daily_stats FROM umkm_app_role;

CREATE FUNCTION app.jkt_day(ts timestamptz) RETURNS date
    LANGUAGE sql IMMUTABLE AS $$ SELECT (ts AT TIME ZONE 'Asia/Jakarta')::date $$;

CREATE FUNCTION app.bump_conversation_stats(
    p_tenant uuid, p_day date, p_started int, p_handed int, p_closed_ai int, p_closed_human int,
    p_fr_count int, p_fr_ms bigint, p_res_count int, p_res_ms bigint) RETURNS void
LANGUAGE sql AS $$
    INSERT INTO conversation_daily_stats AS s (tenant_id, day, started, handed_over, closed_ai, closed_human,
        first_response_count, first_response_ms_sum, resolution_count, resolution_ms_sum)
    VALUES (p_tenant, p_day, p_started, p_handed, p_closed_ai, p_closed_human, p_fr_count, p_fr_ms, p_res_count, p_res_ms)
    ON CONFLICT (tenant_id, day) DO UPDATE SET
        started = s.started + EXCLUDED.started,
        handed_over = s.handed_over + EXCLUDED.handed_over,
        closed_ai = s.closed_ai + EXCLUDED.closed_ai,
        closed_human = s.closed_human + EXCLUDED.closed_human,
        first_response_count = s.first_response_count + EXCLUDED.first_response_count,
        first_response_ms_sum = s.first_response_ms_sum + EXCLUDED.first_response_ms_sum,
        resolution_count = s.resolution_count + EXCLUDED.resolution_count,
        resolution_ms_sum = s.resolution_ms_sum + EXCLUDED.resolution_ms_sum
$$;

CREATE FUNCTION app.ms_between(a timestamptz, b timestamptz) RETURNS bigint
    LANGUAGE sql IMMUTABLE AS $$ SELECT greatest(0, (extract(epoch FROM (b - a)) * 1000)::bigint) $$;

-- Percakapan baru.
CREATE FUNCTION app.stats_on_conversation_insert() RETURNS trigger
LANGUAGE plpgsql AS $$
BEGIN
    PERFORM app.bump_conversation_stats(NEW.tenant_id, app.jkt_day(NEW.created_at), 1, 0, 0, 0, 0, 0, 0, 0);
    RETURN NEW;
END $$;
CREATE TRIGGER trg_conversations_stats_insert AFTER INSERT ON conversations
    FOR EACH ROW EXECUTE FUNCTION app.stats_on_conversation_insert();

-- Handover pertama & penutupan (AI-resolved = ditutup tanpa pernah ditangani staf).
CREATE FUNCTION app.stats_on_conversation_update() RETURNS trigger
LANGUAGE plpgsql AS $$
BEGIN
    IF NEW.status = 'WAITING_HUMAN' AND OLD.status = 'AI' AND NOT OLD.handed_over THEN
        NEW.handed_over := true;
        PERFORM app.bump_conversation_stats(NEW.tenant_id, app.jkt_day(now()), 0, 1, 0, 0, 0, 0, 0, 0);
    END IF;
    IF NEW.status = 'CLOSED' AND OLD.status <> 'CLOSED' THEN
        PERFORM app.bump_conversation_stats(NEW.tenant_id, app.jkt_day(NEW.closed_at), 0, 0,
            CASE WHEN NEW.human_involved THEN 0 ELSE 1 END, CASE WHEN NEW.human_involved THEN 1 ELSE 0 END,
            0, 0, 1, app.ms_between(NEW.created_at, NEW.closed_at));
    END IF;
    RETURN NEW;
END $$;
CREATE TRIGGER trg_conversations_stats_update BEFORE UPDATE ON conversations
    FOR EACH ROW EXECUTE FUNCTION app.stats_on_conversation_update();

-- Waktu respons pertama: pesan pelanggan pertama → balasan AI/staf pertama setelahnya.
CREATE FUNCTION app.stats_on_message() RETURNS trigger
LANGUAGE plpgsql AS $$
DECLARE c record;
BEGIN
    SELECT first_customer_at, first_response_at INTO c FROM conversations
        WHERE tenant_id = NEW.tenant_id AND id = NEW.conversation_id;
    IF NEW.sender_type = 'CUSTOMER' AND c.first_customer_at IS NULL THEN
        UPDATE conversations SET first_customer_at = NEW.created_at
            WHERE tenant_id = NEW.tenant_id AND id = NEW.conversation_id;
    ELSIF NEW.sender_type IN ('AI', 'ADMIN') AND c.first_customer_at IS NOT NULL AND c.first_response_at IS NULL THEN
        UPDATE conversations SET first_response_at = NEW.created_at
            WHERE tenant_id = NEW.tenant_id AND id = NEW.conversation_id;
        PERFORM app.bump_conversation_stats(NEW.tenant_id, app.jkt_day(NEW.created_at), 0, 0, 0, 0,
            1, app.ms_between(c.first_customer_at, NEW.created_at), 0, 0);
    END IF;
    RETURN NEW;
END $$;
CREATE TRIGGER trg_conversation_messages_stats AFTER INSERT ON conversation_messages
    FOR EACH ROW EXECUTE FUNCTION app.stats_on_message();

-- Pemakaian & kegagalan AI per hari dibaca dari ai_runs (tidak ikut purge percakapan).
CREATE INDEX ix_ai_runs_chat_day ON ai_runs (tenant_id, purpose, created_at);
