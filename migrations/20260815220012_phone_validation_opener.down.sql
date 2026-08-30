-- Down: drop messaging.phone_blacklists table
DROP TABLE IF EXISTS messaging.phone_blacklists CASCADE;
DROP FUNCTION IF EXISTS messaging.phone_blacklists_audit_timestamp() CASCADE;
-- Reverse the mail_blacklists opt-out-reason logical ref.
ALTER TABLE messaging.mail_blacklists
    DROP COLUMN IF EXISTS opt_out_reason_id;
