-- Down: drop messaging.mail_aliases table
DROP TABLE IF EXISTS messaging.mail_aliases CASCADE;
DROP FUNCTION IF EXISTS messaging.mail_aliases_audit_timestamp() CASCADE;
