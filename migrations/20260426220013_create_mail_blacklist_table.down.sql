-- Down: drop messaging.mail_blacklists table
DROP TABLE IF EXISTS messaging.mail_blacklists CASCADE;
DROP FUNCTION IF EXISTS messaging.mail_blacklists_audit_timestamp() CASCADE;
