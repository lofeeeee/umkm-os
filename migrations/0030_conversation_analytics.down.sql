DROP INDEX IF EXISTS ix_ai_runs_chat_day;
DROP TRIGGER IF EXISTS trg_conversation_messages_stats ON conversation_messages;
DROP TRIGGER IF EXISTS trg_conversations_stats_update ON conversations;
DROP TRIGGER IF EXISTS trg_conversations_stats_insert ON conversations;
DROP FUNCTION IF EXISTS app.stats_on_message();
DROP FUNCTION IF EXISTS app.stats_on_conversation_update();
DROP FUNCTION IF EXISTS app.stats_on_conversation_insert();
DROP FUNCTION IF EXISTS app.ms_between(timestamptz, timestamptz);
DROP FUNCTION IF EXISTS app.bump_conversation_stats(uuid, date, int, int, int, int, int, bigint, int, bigint);
DROP FUNCTION IF EXISTS app.jkt_day(timestamptz);
DROP TABLE IF EXISTS conversation_daily_stats;
ALTER TABLE conversations
    DROP COLUMN IF EXISTS handed_over,
    DROP COLUMN IF EXISTS first_response_at,
    DROP COLUMN IF EXISTS first_customer_at;
