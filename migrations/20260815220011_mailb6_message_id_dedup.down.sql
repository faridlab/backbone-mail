-- Revert MAIL-B6: back to the plain (non-unique) lookup index. Rows deduped
-- by the up migration are NOT resurrected (they were duplicates).

DROP INDEX IF EXISTS messaging.mail_message_message_id_uniq;

CREATE INDEX mail_message_message_id_idx
    ON messaging.mail_messages (message_id);
