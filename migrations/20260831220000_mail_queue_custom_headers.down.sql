-- Migration: per-mail custom headers on the outgoing queue row (down)
ALTER TABLE messaging.mails
    DROP COLUMN IF EXISTS headers;
