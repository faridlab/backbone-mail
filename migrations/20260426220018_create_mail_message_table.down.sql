-- Down: drop messaging.mail_messages table
DROP TABLE IF EXISTS messaging.mail_messages CASCADE;
DROP FUNCTION IF EXISTS messaging.mail_messages_audit_timestamp() CASCADE;
