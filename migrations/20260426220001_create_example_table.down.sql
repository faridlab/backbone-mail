-- Down: drop messaging.examples table
DROP TABLE IF EXISTS messaging.examples CASCADE;
DROP FUNCTION IF EXISTS messaging.examples_audit_timestamp() CASCADE;
