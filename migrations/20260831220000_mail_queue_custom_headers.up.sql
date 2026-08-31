-- Migration: per-mail custom headers on the outgoing queue row
-- Hand-written ALTER (established for column adds in this module — the
-- codegen plugin's alter path targets the legacy layout; the schema YAML in
-- schema/models/core.model.yaml remains the source of truth for the entity).

-- Custom RFC 5322 headers per outgoing mail: a JSON object of header name to
-- string value, empty by default. Names/values must be single-line — the
-- enqueue verb refuses CR/LF (header-injection guard). The transport's
-- structured threading headers and envelope headers are never overridable
-- from this column.
ALTER TABLE messaging.mails
    ADD COLUMN IF NOT EXISTS headers JSONB NOT NULL DEFAULT '{}'::jsonb;
