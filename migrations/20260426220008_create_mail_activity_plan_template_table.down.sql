-- Down: drop messaging.mail_activity_plan_templates table
DROP TABLE IF EXISTS messaging.mail_activity_plan_templates CASCADE;
DROP FUNCTION IF EXISTS messaging.mail_activity_plan_templates_audit_timestamp() CASCADE;
