-- Down: drop messaging.mail_alias_domains table
DROP TABLE IF EXISTS messaging.mail_alias_domains CASCADE;
DROP FUNCTION IF EXISTS messaging.mail_alias_domains_audit_timestamp() CASCADE;
