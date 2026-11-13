-- Hand-authored (user-owned). Not regenerated.
--
-- Create the transactional outbox and inbox tables this module stages into.
--
-- Every write verb that raises a bus event (message post, sms mint, mail queue, notification status change,
-- and the rest) stages it with backbone_outbox::outbox::stage on the write transaction, and the realtime
-- tailer reads the same table back. Until now the table existed only after the composing service ran
-- backbone_outbox::outbox::migrate for the messaging schema, so on a database built from this migration chain
-- alone every one of those verbs failed on the missing relation.
--
-- The shape is the one outbox::migrate creates: staging writes company_id (the module's platform sentinel, so
-- the column is NOT NULL), the relay reads company_id and org_unit_id and filters on failed_at, and the
-- dead-letter listing reads failed_at and failure_reason. The column adds below are nullable and IF NOT EXISTS,
-- the exact shape outbox::migrate heals to, so a table the framework already created (older shape included)
-- is brought up to date and untouched where it is current: the two paths converge whichever runs first.
--
-- The org-unit fill trigger and the row-level-security fence are NOT installed here. This module declares no
-- company fence, so the messaging outbox stays unfenced unless a composing service opts in by running
-- outbox::migrate itself.

CREATE SCHEMA IF NOT EXISTS messaging;

CREATE TABLE IF NOT EXISTS messaging.outbox_events (
    id             UUID PRIMARY KEY,
    event_type     TEXT NOT NULL,
    aggregate_type TEXT NOT NULL,
    aggregate_id   TEXT NOT NULL,
    company_id     UUID NOT NULL,
    payload        JSONB NOT NULL,
    occurred_at    TIMESTAMPTZ NOT NULL,
    correlation_id TEXT,
    causation_id   TEXT,
    version        INT NOT NULL DEFAULT 1,
    created_at     TIMESTAMPTZ NOT NULL DEFAULT now(),
    published_at   TIMESTAMPTZ
);

ALTER TABLE messaging.outbox_events
    ADD COLUMN IF NOT EXISTS company_id     UUID,
    ADD COLUMN IF NOT EXISTS org_unit_id    UUID,
    ADD COLUMN IF NOT EXISTS failed_at      TIMESTAMPTZ,
    ADD COLUMN IF NOT EXISTS failure_reason TEXT;

-- The un-drained tail only: keeps the relay's poll cheap as the table grows. Same name and predicate as the
-- index outbox::migrate maintains, so the framework's boot finds it in place.
CREATE INDEX IF NOT EXISTS idx_messaging_outbox_pending
    ON messaging.outbox_events (occurred_at) WHERE published_at IS NULL AND failed_at IS NULL;

CREATE TABLE IF NOT EXISTS messaging.inbox_consumed (
    consumer    TEXT NOT NULL,
    event_id    UUID NOT NULL,
    consumed_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (consumer, event_id)
);
