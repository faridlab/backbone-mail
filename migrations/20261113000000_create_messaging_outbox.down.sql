-- Hand-authored (user-owned). Not regenerated.
-- Down: drop the messaging outbox and inbox tables.
-- Destructive: events staged but not yet relayed, and the consumer dedup log, are lost. The framework's
-- outbox::migrate recreates the empty tables on the next boot of any service that runs it for this schema.
DROP TABLE IF EXISTS messaging.inbox_consumed;
DROP TABLE IF EXISTS messaging.outbox_events;
