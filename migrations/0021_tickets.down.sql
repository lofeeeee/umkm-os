DROP TABLE IF EXISTS ticket_events;
DROP TABLE IF EXISTS ticket_attachments;
DROP TABLE IF EXISTS ticket_messages;
DROP TABLE IF EXISTS tickets;
DROP FUNCTION IF EXISTS app.check_ticket_initial();
DROP FUNCTION IF EXISTS app.check_ticket_transition();
DROP FUNCTION IF EXISTS app.ticket_transition_allowed(text, text);
DROP FUNCTION IF EXISTS app.check_ticket_assignee();
