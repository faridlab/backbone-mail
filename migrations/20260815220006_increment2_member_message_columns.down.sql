-- Down: drop increment-2 member read-state + message pin columns

DROP INDEX IF EXISTS messaging.idx_mail_messages_pinned_at;
ALTER TABLE messaging.mail_messages DROP COLUMN IF EXISTS pinned_at;

DROP INDEX IF EXISTS messaging.discuss_channel_member_seen_idx;
DROP INDEX IF EXISTS messaging.idx_discuss_channel_members_unpin_dt;
DROP INDEX IF EXISTS messaging.idx_discuss_channel_members_fetched_message_id;
ALTER TABLE messaging.discuss_channel_members
    DROP COLUMN IF EXISTS last_seen_dt,
    DROP COLUMN IF EXISTS unpin_dt,
    DROP COLUMN IF EXISTS mute_until_dt,
    DROP COLUMN IF EXISTS custom_notifications,
    DROP COLUMN IF EXISTS new_message_separator,
    DROP COLUMN IF EXISTS fetched_message_id;

-- The enum type is left in place: other columns may reference it and
-- CREATE TYPE in the up-migration is IF NOT EXISTS-guarded.
