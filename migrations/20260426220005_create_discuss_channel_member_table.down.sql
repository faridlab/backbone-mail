-- Down: drop messaging.discuss_channel_members table
DROP TABLE IF EXISTS messaging.discuss_channel_members CASCADE;
DROP FUNCTION IF EXISTS messaging.discuss_channel_members_audit_timestamp() CASCADE;
