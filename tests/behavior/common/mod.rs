//! Shared harness: pool bootstrap + graceful SKIPPED-DB plumbing.
//!
//! The schema migrations do NOT create `messaging.outbox_events` (the outbox
//! crate owns that table), so the harness runs the idempotent
//! `backbone_outbox::outbox::migrate` before handing the pool to a test.

use sqlx::postgres::PgPoolOptions;
use sqlx::PgPool;
use std::time::Duration;

/// The docker postgres used for module tests in this workspace.
pub const DEFAULT_DB_URL: &str = "postgres://root:password@localhost:5432/backbone_mail_test";

/// Connect to the test DB (or `None` when unreachable → the caller skips).
pub async fn test_pool() -> Option<PgPool> {
    let url = std::env::var("DATABASE_URL").unwrap_or_else(|_| DEFAULT_DB_URL.into());
    let pool = PgPoolOptions::new()
        .max_connections(8)
        .acquire_timeout(Duration::from_secs(5))
        .connect(&url)
        .await
        .ok()?;
    backbone_outbox::outbox::migrate(&pool, "messaging").await.ok()?;
    Some(pool)
}

/// The skip marker body — print + return from the test.
pub fn skipped(marker: &str) {
    eprintln!("SKIPPED-DB: {marker} — no live Postgres reachable, not faking results");
}

/// Queue-drainer tests cannot isolate by id (a drain claims ANY claimable row),
/// so they serialize on this lock — the shared DB stays deterministic for them.
pub static DRAIN_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

/// Sweep claimable rows left behind by a crashed run (outgoing sms + mails, their
/// trackers, and their arming events). Run under DRAIN_LOCK before enqueuing.
pub async fn sweep_queues(pool: &sqlx::PgPool) {
    // Trackers whose sms is going away (no FK — match by uuid).
    sqlx::query(
        r#"DELETE FROM messaging.sms_trackers
           WHERE sms_uuid IN (SELECT uuid FROM messaging.sms WHERE state = 'outgoing'::sms_state)"#,
    )
    .execute(pool)
    .await
    .ok();
    sqlx::query("DELETE FROM messaging.sms WHERE state = 'outgoing'::sms_state")
        .execute(pool)
        .await
        .ok();
    sqlx::query("DELETE FROM messaging.mails WHERE state = 'outgoing'::mail_state")
        .execute(pool)
        .await
        .ok();
    // The arming/dispatch events those rows staged.
    sqlx::query(
        r#"DELETE FROM messaging.outbox_events
           WHERE event_type IN ('SmsCreated', 'MailQueued', 'MailDispatchRequested')
             AND aggregate_id NOT IN (
                 SELECT id::text FROM messaging.sms
                 UNION SELECT id::text FROM messaging.mails)"#,
    )
    .execute(pool)
    .await
    .ok();
}

/// Delete every row a test minted, by id, best-effort (tests run in parallel
/// against the shared DB; each test only touches rows it created).
pub async fn cleanup(pool: &PgPool, tables: &[(&str, &[uuid::Uuid])]) {
    for (table, ids) in tables {
        if ids.is_empty() {
            continue;
        }
        let _ = sqlx::query(&format!("DELETE FROM messaging.{table} WHERE id = ANY($1)"))
            .bind(ids)
            .execute(pool)
            .await;
    }
}

/// Seed a `mail_message_subtypes` row and return its id.
pub async fn seed_subtype(pool: &PgPool, name: &str) -> uuid::Uuid {
    let id = uuid::Uuid::new_v4();
    sqlx::query(
        r#"INSERT INTO messaging.mail_message_subtypes (id, name)
           VALUES ($1, $2)"#,
    )
    .bind(id)
    .bind(name)
    .execute(pool)
    .await
    .expect("seed subtype");
    id
}
