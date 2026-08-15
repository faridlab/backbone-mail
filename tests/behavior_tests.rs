//! Behavior test runner: the messaging increment-1 golden cases.
//!
//! DB-backed: every test skips with a `SKIPPED-DB` marker when no live Postgres
//! is reachable (the brief forbids faking results). Point `DATABASE_URL` at a
//! migrated messaging schema; the docker default is used otherwise.

mod behavior;
