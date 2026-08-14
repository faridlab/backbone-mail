-- Down: drop messaging.mail_guests table
DROP TABLE IF EXISTS messaging.mail_guests CASCADE;
DROP FUNCTION IF EXISTS messaging.mail_guests_audit_timestamp() CASCADE;
