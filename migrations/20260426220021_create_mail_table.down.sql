-- Down: drop messaging.mails table
DROP TABLE IF EXISTS messaging.mails CASCADE;
DROP FUNCTION IF EXISTS messaging.mails_audit_timestamp() CASCADE;
