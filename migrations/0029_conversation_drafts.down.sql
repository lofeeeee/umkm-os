DELETE FROM conversation_events WHERE kind LIKE 'draft_%';
ALTER TABLE conversation_events DROP CONSTRAINT ck_conversation_events_kind;
ALTER TABLE conversation_events ADD CONSTRAINT ck_conversation_events_kind CHECK (kind IN (
    'created', 'assigned', 'unassigned', 'handover_requested', 'taken_over', 'released', 'closed'));
DROP TABLE IF EXISTS conversation_drafts;
