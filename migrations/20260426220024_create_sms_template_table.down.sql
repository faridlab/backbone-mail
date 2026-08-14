-- Down: drop messaging.sms_templates table
DROP TABLE IF EXISTS messaging.sms_templates CASCADE;
DROP FUNCTION IF EXISTS messaging.sms_templates_audit_timestamp() CASCADE;
