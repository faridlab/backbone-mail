-- Down: drop messaging.mail_activities table
DROP TABLE IF EXISTS messaging.mail_activities CASCADE;
DROP FUNCTION IF EXISTS messaging.mail_activities_audit_timestamp() CASCADE;
