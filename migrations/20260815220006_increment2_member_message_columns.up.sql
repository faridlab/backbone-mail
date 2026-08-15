-- Migration: increment-2 member read-state + message pin columns
-- Hand-written ALTER (the codegen plugin's `migration alter` path targets the
-- legacy libs/ layout; hand-written ALTERs are established here — see
-- 20260815220000_sms_monotonic_guard).
--
-- MAIL-M38.1: DiscussChannelMember read-state fields (source-verified against
-- playground/odoo discuss_channel_member.py):
--   fetched_message_id, new_message_separator (int→uuid adaptation),
--   custom_notifications (new enum), mute_until_dt ('+infinity' = until-unmuted),
--   unpin_dt, last_seen_dt + the (channel,partner,seen) read-path index.
-- MAIL-M37.1: MailMessage.pinned_at (Odoo pins via a column on mail.message).

-- Create member_custom_notifications enum type
DO $$
BEGIN
    IF NOT EXISTS (SELECT 1 FROM pg_type WHERE typname = 'member_custom_notifications') THEN
        CREATE TYPE member_custom_notifications AS ENUM ('all', 'mentions', 'no_notif');
    END IF;
END
$$;

ALTER TABLE messaging.discuss_channel_members
    ADD COLUMN IF NOT EXISTS fetched_message_id UUID,
    ADD COLUMN IF NOT EXISTS new_message_separator UUID,
    ADD COLUMN IF NOT EXISTS custom_notifications member_custom_notifications,
    ADD COLUMN IF NOT EXISTS mute_until_dt TIMESTAMPTZ,
    ADD COLUMN IF NOT EXISTS unpin_dt TIMESTAMPTZ,
    ADD COLUMN IF NOT EXISTS last_seen_dt TIMESTAMPTZ;

CREATE INDEX IF NOT EXISTS idx_discuss_channel_members_fetched_message_id ON messaging.discuss_channel_members (fetched_message_id);
CREATE INDEX IF NOT EXISTS idx_discuss_channel_members_unpin_dt ON messaging.discuss_channel_members (unpin_dt);
CREATE INDEX IF NOT EXISTS discuss_channel_member_seen_idx ON messaging.discuss_channel_members (channel_id, partner_id, seen_message_id);

ALTER TABLE messaging.mail_messages
    ADD COLUMN IF NOT EXISTS pinned_at TIMESTAMPTZ;

CREATE INDEX IF NOT EXISTS idx_mail_messages_pinned_at ON messaging.mail_messages (pinned_at);
