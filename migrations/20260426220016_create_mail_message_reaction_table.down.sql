-- Down: drop messaging.mail_message_reactions table
DROP TABLE IF EXISTS messaging.mail_message_reactions CASCADE;
DROP FUNCTION IF EXISTS messaging.mail_message_reactions_audit_timestamp() CASCADE;
