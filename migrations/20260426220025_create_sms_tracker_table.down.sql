-- Down: drop messaging.sms_trackers table
DROP TABLE IF EXISTS messaging.sms_trackers CASCADE;
DROP FUNCTION IF EXISTS messaging.sms_trackers_audit_timestamp() CASCADE;
