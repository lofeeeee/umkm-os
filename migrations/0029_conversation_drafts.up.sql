-- P6-4: draft balasan AI mode Assisted (PRD.md §4.10) + persetujuan chat (PRD.md §4.9 entity ai_response).
-- Draft tidak pernah terlihat pelanggan; terkirim hanya setelah staf menyetujui (dan approval engine bila
-- tenant punya kebijakan ai_response).
CREATE TABLE conversation_drafts (
    id                 uuid        NOT NULL DEFAULT gen_random_uuid(),
    tenant_id          uuid        NOT NULL REFERENCES tenants (id) ON DELETE CASCADE,
    conversation_id    uuid        NOT NULL,
    ai_run_id          uuid,
    body               text        NOT NULL,
    status             text        NOT NULL DEFAULT 'pending',
    decided_by_user_id uuid,
    decided_at         timestamptz,
    sent_message_id    uuid,
    created_at         timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (id),
    CONSTRAINT uq_conversation_drafts_tenant_id UNIQUE (tenant_id, id),
    CONSTRAINT fk_conversation_drafts_conv FOREIGN KEY (tenant_id, conversation_id)
        REFERENCES conversations (tenant_id, id) ON DELETE CASCADE,
    CONSTRAINT fk_conversation_drafts_run FOREIGN KEY (tenant_id, ai_run_id)
        REFERENCES ai_runs (tenant_id, id) ON DELETE SET NULL (ai_run_id),
    CONSTRAINT fk_conversation_drafts_message FOREIGN KEY (tenant_id, sent_message_id)
        REFERENCES conversation_messages (tenant_id, id) ON DELETE SET NULL (sent_message_id),
    CONSTRAINT ck_conversation_drafts_status CHECK (status IN ('pending', 'approval_pending', 'sent', 'discarded')),
    CONSTRAINT ck_conversation_drafts_body CHECK (length(btrim(body)) BETWEEN 1 AND 4000),
    CONSTRAINT ck_conversation_drafts_decided CHECK (
        (status IN ('sent', 'discarded')) = (decided_at IS NOT NULL)
        AND (status <> 'sent' OR sent_message_id IS NOT NULL))
);
CREATE INDEX ix_conversation_drafts_conv ON conversation_drafts (tenant_id, conversation_id, created_at);

ALTER TABLE conversation_drafts ENABLE ROW LEVEL SECURITY;
ALTER TABLE conversation_drafts FORCE ROW LEVEL SECURITY;
CREATE POLICY tenant_isolation ON conversation_drafts
    USING (tenant_id = app.current_tenant_id()) WITH CHECK (tenant_id = app.current_tenant_id());
REVOKE DELETE ON conversation_drafts FROM umkm_app_role;

-- Riwayat: draft dibuat/dikirim/dibuang.
ALTER TABLE conversation_events DROP CONSTRAINT ck_conversation_events_kind;
ALTER TABLE conversation_events ADD CONSTRAINT ck_conversation_events_kind CHECK (kind IN (
    'created', 'assigned', 'unassigned', 'handover_requested', 'taken_over', 'released', 'closed',
    'draft_created', 'draft_sent', 'draft_discarded', 'draft_approval_requested'));
