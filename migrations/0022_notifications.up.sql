-- P4-4: notifikasi in-app + catatan pengiriman (PRD.md §4.13, FR-NTF-001..003).
-- Worker mengubah event outbox menjadi notifikasi per penerima di tenant context event.

CREATE TABLE notifications (
    id              uuid        NOT NULL DEFAULT gen_random_uuid(),
    tenant_id       uuid        NOT NULL REFERENCES tenants (id) ON DELETE CASCADE,
    user_id         uuid        NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    event_type      text        NOT NULL,
    title           text        NOT NULL,
    body            text        NOT NULL,
    link            text,
    source_event_id uuid        NOT NULL,
    read_at         timestamptz,
    created_at      timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (id),
    -- AC-NTF-002/003: tepat satu notifikasi per penerima per event, walau worker mengulang.
    CONSTRAINT uq_notifications_recipient UNIQUE (tenant_id, user_id, source_event_id),
    CONSTRAINT ck_notifications_title CHECK (length(title) BETWEEN 1 AND 120),
    CONSTRAINT ck_notifications_body CHECK (length(body) <= 500),
    CONSTRAINT ck_notifications_link CHECK (link IS NULL OR (link ~ '^/[A-Za-z0-9/_?=&.-]*$' AND length(link) <= 200))
);
CREATE INDEX ix_notifications_user ON notifications (tenant_id, user_id, created_at DESC);
CREATE INDEX ix_notifications_unread ON notifications (tenant_id, user_id) WHERE read_at IS NULL;

-- Pengiriman kanal eksternal (email). Baris ditulis SEBELUM kirim; kunci = idempotency key provider.
CREATE TABLE notification_deliveries (
    id              uuid        NOT NULL DEFAULT gen_random_uuid(),
    tenant_id       uuid        NOT NULL REFERENCES tenants (id) ON DELETE CASCADE,
    source_event_id uuid        NOT NULL,
    channel         text        NOT NULL,
    recipient_hash  text        NOT NULL,
    status          text        NOT NULL DEFAULT 'pending',
    created_at      timestamptz NOT NULL DEFAULT now(),
    sent_at         timestamptz,
    PRIMARY KEY (id),
    CONSTRAINT uq_notification_deliveries UNIQUE (tenant_id, source_event_id, channel, recipient_hash),
    CONSTRAINT ck_notification_deliveries_channel CHECK (channel IN ('email')),
    CONSTRAINT ck_notification_deliveries_status CHECK (status IN ('pending', 'sent'))
);

DO $$
DECLARE t text;
BEGIN
    FOREACH t IN ARRAY ARRAY['notifications', 'notification_deliveries'] LOOP
        EXECUTE format('ALTER TABLE %I ENABLE ROW LEVEL SECURITY', t);
        EXECUTE format('ALTER TABLE %I FORCE ROW LEVEL SECURITY', t);
        EXECUTE format('CREATE POLICY tenant_isolation ON %I USING (tenant_id = app.current_tenant_id()) WITH CHECK (tenant_id = app.current_tenant_id())', t);
    END LOOP;
END $$;
REVOKE DELETE ON notification_deliveries FROM umkm_app_role;
