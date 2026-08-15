-- Down: drop messaging.fetchmail_servers table
DROP TABLE IF EXISTS messaging.fetchmail_servers CASCADE;
DROP FUNCTION IF EXISTS messaging.fetchmail_servers_audit_timestamp() CASCADE;
