-- Migration: sms_monotonic_guard (hand-written; NOT generated — survives regen by its
-- timestamp sitting after the generated series).
--
-- SM-B6 must-fix (ADR-0015): Odoo's monotonic status guard
-- (`sms.tracker.notifications_statuses_to_ignore`, sms_tracker.py:64-72) is a
-- pure-Python local-var dict filtered BEFORE the ORM write — no row lock on
-- mail.notification, so a TOCTOU window on concurrent webhooks lets a `bounce`
-- regress to `sent`. This trigger moves the lattice to the DB, surviving raw SQL,
-- concurrent webhooks, and the sync-IAP path alike. The service-level
-- SMS_STATE_TO_NOTIFICATION_STATUS map (sms_write_service) remains the single
-- source for which advance is LEGAL; this trigger is the floor that no write can
-- go below.
--
-- Lattice (rank never decreases):
--   ready/outgoing = 0 < process = 1 < pending = 2 < sent = 3
--   bounce/exception/error/canceled = 100 (terminal-once-set: every rank below is a
--   regression; equal-rank writes — including terminal→terminal relabels and
--   idempotent same-value rewrites — are allowed, which is exactly what makes a
--   replayed webhook a no-op instead of an error).
--
-- Covers BOTH guarded fields:
--   messaging.mail_notifications.notification_status  (MAIL-M3 origin of the inversion)
--   messaging.sms.state                               (SM-M1/SM-F19 mirror)
--
-- Note `messaging.mails.state` is deliberately NOT guarded (MAIL-M2): mail.mail's
-- state is NOT label-inverted and its crash-safety pre-writes state='exception'
-- BEFORE the SMTP attempt, then 'sent' on 250 — a monotonic guard would break that.

-- The shared rank function (text-typed: both enums reduce to the same lattice).
CREATE OR REPLACE FUNCTION messaging.messaging_status_rank(v text)
RETURNS integer
LANGUAGE sql
IMMUTABLE
AS $$
    SELECT CASE v
        WHEN 'ready'    THEN 0
        WHEN 'outgoing' THEN 0
        WHEN 'process'  THEN 1
        WHEN 'pending'  THEN 2
        WHEN 'sent'     THEN 3
        WHEN 'bounce'   THEN 100
        WHEN 'exception'THEN 100
        WHEN 'error'    THEN 100
        WHEN 'canceled' THEN 100
        ELSE NULL  -- not a lattice member: guard is vacuous (enum types constrain values anyway)
    END
$$;

-- Guard on messaging.mail_notifications.notification_status.
CREATE OR REPLACE FUNCTION messaging.mail_notification_monotonic_guard()
RETURNS trigger
LANGUAGE plpgsql
AS $$
BEGIN
    IF NEW.notification_status IS DISTINCT FROM OLD.notification_status
       AND messaging.messaging_status_rank(NEW.notification_status::text) IS NOT NULL
       AND messaging.messaging_status_rank(OLD.notification_status::text) IS NOT NULL
       AND messaging.messaging_status_rank(NEW.notification_status::text)
           < messaging.messaging_status_rank(OLD.notification_status::text) THEN
        RAISE EXCEPTION
            'messaging.mail_notifications.notification_status monotonic violation (SM-B6/ADR-0015): cannot regress % -> %',
            OLD.notification_status, NEW.notification_status
            USING ERRCODE = 'check_violation';
    END IF;
    RETURN NEW;
END;
$$;

DROP TRIGGER IF EXISTS mail_notification_monotonic_guard ON messaging.mail_notifications;
CREATE TRIGGER mail_notification_monotonic_guard
    BEFORE UPDATE OF notification_status ON messaging.mail_notifications
    FOR EACH ROW
    EXECUTE FUNCTION messaging.mail_notification_monotonic_guard();

-- Guard on messaging.sms.state (the SM-M1/SM-F19 mirror; 'outgoing' sits where
-- 'ready' sits on the notification).
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

DROP TRIGGER IF EXISTS sms_state_monotonic_guard ON messaging.sms;
CREATE TRIGGER sms_state_monotonic_guard
    BEFORE UPDATE OF state ON messaging.sms
    FOR EACH ROW
    EXECUTE FUNCTION messaging.sms_state_monotonic_guard();
