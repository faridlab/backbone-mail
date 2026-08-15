-- Down: drop messaging.mail_gateway_allowed table
DROP TABLE IF EXISTS messaging.mail_gateway_allowed CASCADE;
DROP FUNCTION IF EXISTS messaging.mail_gateway_allowed_audit_timestamp() CASCADE;
