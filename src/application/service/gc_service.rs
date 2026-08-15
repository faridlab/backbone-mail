//! The GC + stuck-sweep service (hand-written; user-owned).
//!
//! Four job bodies in one service (they share the "age-bound delete/sweep"
//! shape and all run as autovacuum-ride or pull jobs with no request path):
//!
//! - `notification_gc` — MAIL-B8: reap ALL mail_notification rows past the
//!   retention bound. Odoo carves out partner_share rows; the port has no
//!   such flag, so the carve-out is dropped — a documented delta.
//! - `presence_gc` — MAIL-M11: presence rows not polled within the stale
//!   window (Odoo's 12h vacuum-in-code).
//! - `guest_gc` — M43: guests whose last_connection_dt went stale are
//!   soft-deleted (possession of the dgid cookie was their only identity; a
//!   stale one is abandoned by design).
//! - `sweep_stuck_process` — SM-B13: sms rows stuck in `'process'` past the
//!   threshold mean a drainer died mid-flight; emit `SmsStuckProcessDetected`
//!   and (bounded) re-queue the rows for pickup.
//! - `sms_gc` — increment 3, the `sms::gc` job hook (sms-gc-device): reap
//!   TERMINAL sms rows (`sent`/`error`/`canceled`) past the retention bound;
//!   `pending` still awaits a DSN and is never reaped here.

use uuid::Uuid;

use crate::domain::event::constants::stage_bus_event;
use crate::infrastructure::persistence::gc_repository::GcRepository;

/// The ops bus channel the stuck-sweep alert rides (plain-str, BUS-B2-safe —
/// built here, never accepted from a wire).
const SMS_OPS_CHANNEL: &str = "sms.queue_ops";

#[derive(Debug, thiserror::Error)]
pub enum GcError {
    #[error("db: {0}")]
    Db(#[from] sqlx::Error),
}

pub struct GcService {
    pool: sqlx::PgPool,
    /// MAIL-B8 retention bound (days). Odoo's cleanup default is 30.
    pub notification_retention_days: i64,
    /// MAIL-M11 stale bound (hours). Odoo reaps at 12h.
    pub presence_stale_hours: i64,
    /// M43 stale bound (minutes). Config default 60.
    pub guest_stale_minutes: i64,
    /// SM-B13: how long in `'process'` before a row counts as stuck.
    pub stuck_threshold_minutes: i64,
}

impl GcService {
    pub fn new(
        pool: sqlx::PgPool,
        notification_retention_days: i64,
        presence_stale_hours: i64,
        guest_stale_minutes: i64,
        stuck_threshold_minutes: i64,
    ) -> Self {
        Self {
            pool,
            notification_retention_days,
            presence_stale_hours,
            guest_stale_minutes,
            stuck_threshold_minutes,
        }
    }

    /// MAIL-B8 (`mail_notification::gc`). One tx, returns rows reaped.
    pub async fn notification_gc(&self) -> Result<u64, GcError> {
        let mut tx = self.pool.begin().await?;
        let n = GcRepository::notification_gc(&mut tx, self.notification_retention_days).await?;
        tx.commit().await?;
        Ok(n)
    }

    /// MAIL-M11 (`mail_presence::gc`).
    pub async fn presence_gc(&self) -> Result<u64, GcError> {
        let mut tx = self.pool.begin().await?;
        let n = GcRepository::presence_gc(&mut tx, self.presence_stale_hours).await?;
        tx.commit().await?;
        Ok(n)
    }

    /// M43 (`mail_guest::gc`).
    pub async fn guest_gc(&self) -> Result<u64, GcError> {
        let mut tx = self.pool.begin().await?;
        let n = GcRepository::guest_gc(&mut tx, self.guest_stale_minutes).await?;
        tx.commit().await?;
        Ok(n)
    }

    /// Increment 3 (`sms::gc` — the sms-gc-device job hook): reap sms rows in
    /// terminal states past `retention_days`. The bound comes from the JOB
    /// config (not the constructor) — notification/presence/guest bounds are
    /// module-wide constants, but sms retention is an ops dial (aggressive
    /// reaping is sometimes wanted under provider-quota pressure).
    pub async fn sms_gc(&self, retention_days: i64) -> Result<u64, GcError> {
        let mut tx = self.pool.begin().await?;
        let n = GcRepository::sms_gc(&mut tx, retention_days).await?;
        tx.commit().await?;
        Ok(n)
    }

    /// SM-B13 (`sms::sweep_stuck_process`): find → alert → bounded re-queue,
    /// one tx. Rows already carrying a `swept_at` marker stay put (the bound
    /// — no infinite retry). Returns (stuck ids, requeued count).
    pub async fn sweep_stuck_process(&self) -> Result<(Vec<Uuid>, u64), GcError> {
        let mut tx = self.pool.begin().await?;
        let (ids, oldest_minutes) =
            GcRepository::find_stuck_process(&mut tx, self.stuck_threshold_minutes).await?;
        let mut requeued = 0u64;
        if !ids.is_empty() {
            requeued = GcRepository::requeue_stuck_process(&mut tx, &ids).await?;
            stage_bus_event(
                &mut tx,
                "SmsStuckProcessDetected",
                "Sms",
                // Aggregate alert — the event has no single row id; the first
                // stuck id anchors it for traceability.
                ids[0],
                SMS_OPS_CHANNEL.to_string(),
                "sms.sweep_stuck_process",
                serde_json::json!({
                    "sms_ids": ids,
                    "stuck_for_minutes": oldest_minutes,
                    "requeued": requeued > 0,
                }),
            )
            .await?;
        }
        tx.commit().await?;
        Ok((ids, requeued))
    }
}
