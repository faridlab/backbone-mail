-- Down: drop messaging.mail_message_attachments table
DROP TABLE IF EXISTS messaging.mail_message_attachments CASCADE;
DROP FUNCTION IF EXISTS messaging.mail_message_attachments_audit_timestamp() CASCADE;
