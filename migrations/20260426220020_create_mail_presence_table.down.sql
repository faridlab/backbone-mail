-- Down: drop messaging.mail_presences table
DROP TABLE IF EXISTS messaging.mail_presences CASCADE;
DROP FUNCTION IF EXISTS messaging.mail_presences_audit_timestamp() CASCADE;
