-- Down: drop messaging.mail_notifications table
DROP TABLE IF EXISTS messaging.mail_notifications CASCADE;
DROP FUNCTION IF EXISTS messaging.mail_notifications_audit_timestamp() CASCADE;
