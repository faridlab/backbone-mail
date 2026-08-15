-- Down: drop the SM-B6 monotonic-guard triggers and the shared rank function.
-- Trigger and function names are load-bearing — keep them stable so up/down is
-- idempotent across regens.

DROP TRIGGER IF EXISTS sms_state_monotonic_guard ON messaging.sms;
DROP FUNCTION IF EXISTS messaging.sms_state_monotonic_guard();

DROP TRIGGER IF EXISTS mail_notification_monotonic_guard ON messaging.mail_notifications;
DROP FUNCTION IF EXISTS messaging.mail_notification_monotonic_guard();

DROP FUNCTION IF EXISTS messaging.messaging_status_rank();
