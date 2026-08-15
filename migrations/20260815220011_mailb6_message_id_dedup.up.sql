-- Migration: MAIL-B6 — inbound dedup via UNIQUE partial index on message_id
--
-- Odoo serializes inbound duplicates with pg_try_advisory_xact_lock over
-- hashtext(message_id) — a 32-bit hash that can collide (distinct message-ids
-- alias to the same lock, serializing unrelated messages) while ALSO failing
-- the other way: the advisory lock only guards the INSERT window, so a crash
-- between commit and the next poll can still double-process.
--
-- The port replaces the lock entirely: a UNIQUE partial index makes the
-- database the dedup authority. NULLs (local chatter posts, discuss messages)
-- are outside the index — Postgres UNIQUE treats NULLs as distinct, so the
-- partial WHERE keeps the index honest AND small.
--
-- Generated migrations are immutable history (the schema YAML change is the
-- source of truth; this hand ALTER carries it to existing deployments).

DROP INDEX IF EXISTS messaging.mail_message_message_id_idx;

-- Dedup before constraint: collapse any pre-existing duplicates (keep the
-- oldest row per message_id, matching Odoo's first-wins inbound semantics).
-- Timestamps live in the metadata jsonb (audit-metadata convention).
DELETE FROM messaging.mail_messages a
    USING messaging.mail_messages b
    WHERE a.message_id IS NOT NULL
      AND a.message_id = b.message_id
      AND COALESCE(a.metadata ->> 'created_at', '') > COALESCE(b.metadata ->> 'created_at', '');

CREATE UNIQUE INDEX mail_message_message_id_uniq
    ON messaging.mail_messages (message_id)
    WHERE message_id IS NOT NULL;
