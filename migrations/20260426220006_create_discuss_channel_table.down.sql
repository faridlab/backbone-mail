-- Down: drop messaging.discuss_channels table
DROP TABLE IF EXISTS messaging.discuss_channels CASCADE;
DROP FUNCTION IF EXISTS messaging.discuss_channels_audit_timestamp() CASCADE;
