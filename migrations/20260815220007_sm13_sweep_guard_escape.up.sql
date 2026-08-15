-- Migration: SM-B13 sweep escape for the sms monotonic guard
--
-- The stuck-process sweep (SM-B13) re-queues a stuck 'process' row back to
-- 'outgoing' — a state-rank regression the SM-B6 guard (983abfb) forbids by
-- design. The two rules are reconciled here: the regression is permitted
-- ONLY on the requeue write itself, identified by the write that mints the
-- `swept_at` metadata marker (NULL -> set). The marker is the SM-B13 bound —
-- it is set exactly once per row lifetime, so the escape cannot be replayed;
-- a stuck-again row is detected for alerting but never re-queued twice.
--
-- This is the ONLY sanctioned path from a higher rank back to 'outgoing'.

CREATE OR REPLACE FUNCTION messaging.sms_state_monotonic_guard()
RETURNS trigger
LANGUAGE plpgsql
AS $$
BEGIN
    IF NEW.state IS DISTINCT FROM OLD.state
       AND messaging.messaging_status_rank(NEW.state::text) IS NOT NULL
       AND messaging.messaging_status_rank(OLD.state::text) IS NOT NULL
       AND messaging.messaging_status_rank(NEW.state::text)
           < messaging.messaging_status_rank(OLD.state::text)
       -- SM-B13 escape: the requeue write mints swept_at on the same UPDATE.
       AND NOT (
               (OLD.metadata->>'swept_at') IS NULL
               AND (NEW.metadata->>'swept_at') IS NOT NULL
           ) THEN
        RAISE EXCEPTION
            'messaging.sms.state monotonic violation (SM-B6/ADR-0015): cannot regress % -> %',
            OLD.state, NEW.state
            USING ERRCODE = 'check_violation';
    END IF;
    RETURN NEW;
END;
$$;
