-- Down: drop messaging.mail_followers table
DROP TABLE IF EXISTS messaging.mail_followers CASCADE;
DROP FUNCTION IF EXISTS messaging.mail_followers_audit_timestamp() CASCADE;
