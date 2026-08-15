-- Down: drop messaging.mail_servers table
DROP TABLE IF EXISTS messaging.mail_servers CASCADE;
DROP FUNCTION IF EXISTS messaging.mail_servers_audit_timestamp() CASCADE;
