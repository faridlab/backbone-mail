-- Down: drop messaging.phone_blacklists table
DROP TABLE IF EXISTS messaging.phone_blacklists CASCADE;
DROP FUNCTION IF EXISTS messaging.phone_blacklists_audit_timestamp() CASCADE;
