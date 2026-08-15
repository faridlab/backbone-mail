-- Revert the SM-B13 sweep escape: restore the strict monotonic guard from
-- 20260815220000 (no regression path at all).

CREATE OR REPLACE FUNCTION messaging.sms_state_monotonic_guard()
RETURNS trigger
LANGUAGE plpgsql
AS $$
BEGIN
    IF NEW.state IS DISTINCT FROM OLD.state
       AND messaging.messaging_status_rank(NEW.state::text) IS NOT NULL
       AND messaging.messaging_status_rank(OLD.state::text) IS NOT NULL
       AND messaging.messaging_status_rank(NEW.state::text)
           < messaging.messaging_status_rank(OLD.state::text) THEN
        RAISE EXCEPTION
            'messaging.sms.state monotonic violation (SM-B6/ADR-0015): cannot regress % -> %',
            OLD.state, NEW.state
            USING ERRCODE = 'check_violation';
    END IF;
    RETURN NEW;
END;
$$;
