//! Behavior test runner: the messaging increment-1 golden cases.
//!
//! DB-backed, two harness generations:
//! - the shared-harness cases resolve `DATABASE_URL` and fall back to the
//!   docker default, skipping with a `SKIPPED-DB` marker when neither is
//!   reachable (the brief forbids faking results);
//! - the phone-validation cases use `#[sqlx::test]`, which requires
//!   `DATABASE_URL` to be set outright and provisions a migrated scratch
//!   database per test from that server (the per-mail-headers and
//!   email-blacklist cases follow the same scratch-database harness);
//!
//! A bare run with no reachable default DB therefore skips the older cases but
//! FAILS the phone cases — always run with `DATABASE_URL` pointed at a
//! Postgres the harness may create scratch databases on.

mod behavior;
