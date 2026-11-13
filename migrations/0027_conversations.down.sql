DROP FUNCTION IF EXISTS app.purge_conversations(timestamptz);
DROP TRIGGER IF EXISTS trg_tickets_chat_ref ON tickets;
DROP FUNCTION IF EXISTS app.check_ticket_chat_ref();
DROP INDEX IF EXISTS ix_tickets_conversation;
ALTER TABLE tickets DROP CONSTRAINT IF EXISTS fk_tickets_conversation;
ALTER TABLE tickets DROP CONSTRAINT IF EXISTS ck_tickets_order_ref;
ALTER TABLE tickets ADD CONSTRAINT ck_tickets_source_ref
    CHECK ((source <> 'order' OR order_id IS NOT NULL) AND (source <> 'chat' OR conversation_id IS NOT NULL));
DROP INDEX IF EXISTS ix_ai_runs_conversation;
ALTER TABLE ai_runs DROP CONSTRAINT IF EXISTS fk_ai_runs_conversation;
DROP TABLE IF EXISTS conversation_events;
DROP TABLE IF EXISTS conversation_messages;
DROP TABLE IF EXISTS conversation_sessions;
DROP TABLE IF EXISTS conversations;
DROP FUNCTION IF EXISTS app.on_conversation_message();
DROP FUNCTION IF EXISTS app.check_conversation_status();
DROP FUNCTION IF EXISTS app.conversation_transition_allowed(text, text);
DROP FUNCTION IF EXISTS app.check_conversation_assignee();
