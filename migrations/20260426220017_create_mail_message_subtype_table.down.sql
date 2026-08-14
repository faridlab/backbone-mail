-- Down: drop messaging.mail_message_subtypes table
DROP TABLE IF EXISTS messaging.mail_message_subtypes CASCADE;
DROP FUNCTION IF EXISTS messaging.mail_message_subtypes_audit_timestamp() CASCADE;
