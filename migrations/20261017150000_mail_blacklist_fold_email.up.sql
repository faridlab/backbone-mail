-- A blacklisted address is stored lower-cased, whoever writes it: the write service folds case,
-- and this trigger folds it for every other writer (mass mailing's bounce sweep, generic CRUD, raw
-- SQL), so the unique index on email holds one row per address regardless of case.
--
-- Rows written before the trigger are merged first: of the rows sharing one address, the active
-- one (else the oldest) stays, active when any of them was, and the others go.
WITH ranked AS (
    SELECT id,
           lower(email) AS folded,
           row_number() OVER (PARTITION BY lower(email) ORDER BY active DESC, metadata->>'created_at' NULLS LAST, id) AS rn,
           bool_or(active) OVER (PARTITION BY lower(email)) AS any_active
      FROM messaging.mail_blacklists
), dropped AS (
    DELETE FROM messaging.mail_blacklists b USING ranked r WHERE b.id = r.id AND r.rn > 1
)
UPDATE messaging.mail_blacklists b SET active = r.any_active
  FROM ranked r WHERE b.id = r.id AND r.rn = 1 AND b.active IS DISTINCT FROM r.any_active;

UPDATE messaging.mail_blacklists SET email = lower(email) WHERE email <> lower(email);

CREATE OR REPLACE FUNCTION messaging.mail_blacklists_fold_email() RETURNS trigger AS $$
BEGIN
    NEW.email := lower(NEW.email);
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

DROP TRIGGER IF EXISTS mail_blacklists_fold_email ON messaging.mail_blacklists;
CREATE TRIGGER mail_blacklists_fold_email
    BEFORE INSERT OR UPDATE OF email ON messaging.mail_blacklists
    FOR EACH ROW EXECUTE FUNCTION messaging.mail_blacklists_fold_email();
