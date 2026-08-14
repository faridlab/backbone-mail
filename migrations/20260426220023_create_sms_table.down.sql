-- Down: drop messaging.sms table
DROP TABLE IF EXISTS messaging.sms CASCADE;
DROP FUNCTION IF EXISTS messaging.sms_audit_timestamp() CASCADE;
