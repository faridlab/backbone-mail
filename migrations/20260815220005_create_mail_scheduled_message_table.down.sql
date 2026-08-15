-- Down: drop messaging.mail_scheduled_messages table
DROP TABLE IF EXISTS messaging.mail_scheduled_messages CASCADE;
DROP FUNCTION IF EXISTS messaging.mail_scheduled_messages_audit_timestamp() CASCADE;
