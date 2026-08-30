-- Migration: phone_validation opener — phone_blacklists + the mail_blacklists
-- opt-out-reason logical ref (generated table DDL, extended by hand with the
-- mail_blacklists column; the schema YAML is the source of truth for both).

CREATE SCHEMA IF NOT EXISTS messaging;

CREATE TABLE IF NOT EXISTS messaging.phone_blacklists (
    id UUID NOT NULL DEFAULT gen_random_uuid(),
    number TEXT NOT NULL,
    active BOOLEAN NOT NULL DEFAULT TRUE,
    metadata JSONB NOT NULL DEFAULT '{"created_at":null,"updated_at":null,"deleted_at":null,"created_by":null,"updated_by":null,"deleted_by":null}'::jsonb,
    PRIMARY KEY (id)
);

CREATE INDEX IF NOT EXISTS phone_blacklists_active_idx ON messaging.phone_blacklists (active);

CREATE UNIQUE INDEX IF NOT EXISTS idx_phone_blacklists_number ON messaging.phone_blacklists (number);

-- GIN index for audit metadata JSONB queries
CREATE INDEX IF NOT EXISTS idx_phone_blacklists_metadata_gin ON messaging.phone_blacklists USING GIN (metadata);
CREATE INDEX IF NOT EXISTS idx_phone_blacklists_metadata_deleted_at ON messaging.phone_blacklists ((metadata->>'deleted_at'));
CREATE INDEX IF NOT EXISTS idx_phone_blacklists_metadata_created_at ON messaging.phone_blacklists ((metadata->>'created_at'));
CREATE INDEX IF NOT EXISTS idx_phone_blacklists_metadata_updated_at ON messaging.phone_blacklists ((metadata->>'updated_at'));

-- Triggers for automatic metadata timestamp management
-- Automatically sets created_at on INSERT and updated_at on UPDATE

-- Function to set metadata->'created_at' on INSERT
CREATE OR REPLACE FUNCTION messaging.phone_blacklists_audit_timestamp() RETURNS trigger AS $$
BEGIN
    IF TG_OP = 'INSERT' THEN
        NEW.metadata = jsonb_set(NEW.metadata::jsonb, '{created_at}', to_jsonb(NOW()));
        NEW.metadata = jsonb_set(NEW.metadata::jsonb, '{updated_at}', to_jsonb(NOW()));
    ELSIF TG_OP = 'UPDATE' THEN
        NEW.metadata = jsonb_set(NEW.metadata::jsonb, '{updated_at}', to_jsonb(NOW()));
    END IF;
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

-- Trigger to set timestamps on INSERT
DROP TRIGGER IF EXISTS phone_blacklists_insert_audit ON messaging.phone_blacklists;
CREATE TRIGGER phone_blacklists_insert_audit BEFORE INSERT ON messaging.phone_blacklists
    FOR EACH ROW EXECUTE FUNCTION messaging.phone_blacklists_audit_timestamp();

-- Trigger to set updated_at on UPDATE
DROP TRIGGER IF EXISTS phone_blacklists_update_audit ON messaging.phone_blacklists;
CREATE TRIGGER phone_blacklists_update_audit BEFORE UPDATE ON messaging.phone_blacklists
    FOR EACH ROW EXECUTE FUNCTION messaging.phone_blacklists_audit_timestamp();

-- mail.blacklist opt-out reason: a nullable LOGICAL uuid ref to the mailing
-- module's OptOutReason catalog (cross-module, no FK — the inventory
-- service_project_id logical-ref class). Mail treats the value as opaque.
ALTER TABLE messaging.mail_blacklists
    ADD COLUMN IF NOT EXISTS opt_out_reason_id UUID NULL;
