-- Down: drop messaging.mail_attachments table
DROP TABLE IF EXISTS messaging.mail_attachments CASCADE;
DROP FUNCTION IF EXISTS messaging.mail_attachments_audit_timestamp() CASCADE;
