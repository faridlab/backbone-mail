-- Down: drop messaging.mail_tracking_values table
DROP TABLE IF EXISTS messaging.mail_tracking_values CASCADE;
DROP FUNCTION IF EXISTS messaging.mail_tracking_values_audit_timestamp() CASCADE;
