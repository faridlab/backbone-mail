-- Down: drop messaging.mail_message_stars table
DROP TABLE IF EXISTS messaging.mail_message_stars CASCADE;
DROP FUNCTION IF EXISTS messaging.mail_message_stars_audit_timestamp() CASCADE;
