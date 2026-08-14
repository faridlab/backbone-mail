-- Down: drop messaging.mail_activity_types table
DROP TABLE IF EXISTS messaging.mail_activity_types CASCADE;
DROP FUNCTION IF EXISTS messaging.mail_activity_types_audit_timestamp() CASCADE;
