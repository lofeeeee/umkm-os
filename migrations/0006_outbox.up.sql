-- P1-6: transactional outbox (PRD.md §4.13, §5.4). Event ditulis di transaksi yang
-- sama dengan perubahan bisnis; worker mengirim dengan retry/backoff, idempotent.
-- Payload TIDAK boleh berisi secret/token: token dibuat worker saat pengiriman.
CREATE TABLE notification_outbox (
    id              uuid        PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id       uuid        REFERENCES tenants (id) ON DELETE CASCADE,
    event_type      text        NOT NULL,
    payload         jsonb       NOT NULL DEFAULT '{}'::jsonb,
    idempotency_key text        NOT NULL,
    status          text        NOT NULL DEFAULT 'pending',
    attempts        integer     NOT NULL DEFAULT 0,
    available_at    timestamptz NOT NULL DEFAULT now(),
    locked_until    timestamptz,
    locked_by       text,
    last_error      text,
    created_at      timestamptz NOT NULL DEFAULT now(),
    processed_at    timestamptz,
    CONSTRAINT ck_outbox_event_type CHECK (event_type ~ '^[a-z_]+(\.[a-z_]+)+$'),
    CONSTRAINT ck_outbox_status CHECK (status IN ('pending', 'processing', 'sent', 'dead')),
    CONSTRAINT ck_outbox_attempts CHECK (attempts >= 0),
    CONSTRAINT ck_outbox_payload CHECK (jsonb_typeof(payload) = 'object'),
    CONSTRAINT ck_outbox_last_error CHECK (last_error IS NULL OR length(last_error) <= 500),
    CONSTRAINT ck_outbox_processing_lock CHECK ((status = 'processing') = (locked_until IS NOT NULL))
);
-- Idempotensi enqueue (BR-017)
CREATE UNIQUE INDEX uq_outbox_idempotency ON notification_outbox (idempotency_key);
-- Antrean siap kerja + reclaim lease kedaluwarsa
CREATE INDEX ix_outbox_ready ON notification_outbox (available_at) WHERE status = 'pending';
CREATE INDEX ix_outbox_processing ON notification_outbox (locked_until) WHERE status = 'processing';
CREATE INDEX ix_outbox_tenant ON notification_outbox (tenant_id);
