-- Down: drop messaging.mail_message_schedules table
DROP TABLE IF EXISTS messaging.mail_message_schedules CASCADE;
DROP FUNCTION IF EXISTS messaging.mail_message_schedules_audit_timestamp() CASCADE;
