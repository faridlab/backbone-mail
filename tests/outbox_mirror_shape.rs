//! The messaging outbox must be creatable from this module's own migration chain.
//!
//! `backbone_outbox::outbox::stage` names `company_id` in its INSERT, the relay reads `org_unit_id` and
//! filters on `failed_at`, and the realtime tailer reads the table back. The behavior harness provisions the
//! table through `outbox::migrate` before any test runs, which would hide a chain that never creates it, so
//! this guard reads the migration file itself and needs no database.

const CREATE: &str = include_str!("../migrations/20261113000000_create_messaging_outbox.up.sql");

/// Columns the framework's outbox code touches on `outbox_events`.
const COLUMNS: [&str; 12] = [
    "id", "event_type", "aggregate_type", "aggregate_id", "company_id", "payload", "occurred_at",
    "correlation_id", "causation_id", "version", "created_at", "published_at",
];
/// Columns the relay and dead-letter paths add on top, healed in place on an older table.
const HEALED: [&str; 3] = ["org_unit_id", "failed_at", "failure_reason"];

#[test]
fn the_chain_creates_the_outbox_and_inbox_tables() {
    assert!(CREATE.contains("CREATE TABLE IF NOT EXISTS messaging.outbox_events"));
    assert!(CREATE.contains("CREATE TABLE IF NOT EXISTS messaging.inbox_consumed"));
}

#[test]
fn every_column_the_outbox_code_uses_is_created() {
    for column in COLUMNS {
        assert!(CREATE.contains(&format!("    {column} ")) || CREATE.contains(&format!("    {column}\t")),
            "outbox_events must declare `{column}`");
    }
    for column in HEALED {
        assert!(CREATE.contains(&format!("ADD COLUMN IF NOT EXISTS {column} ")),
            "outbox_events must add `{column}` idempotently");
    }
}

#[test]
fn the_pending_tail_index_matches_the_one_the_framework_keeps() {
    assert!(CREATE.contains("idx_messaging_outbox_pending"));
    assert!(CREATE.contains("WHERE published_at IS NULL AND failed_at IS NULL"));
}
