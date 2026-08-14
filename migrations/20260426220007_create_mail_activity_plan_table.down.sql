-- Down: drop messaging.mail_activity_plans table
DROP TABLE IF EXISTS messaging.mail_activity_plans CASCADE;
DROP FUNCTION IF EXISTS messaging.mail_activity_plans_audit_timestamp() CASCADE;
