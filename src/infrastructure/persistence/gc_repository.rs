//! Repository for the GC/sweep job bodies (hand-written; user-owned).
//!
//! Holds the SQL the [`crate::application::service::GcService`] orchestrates:
//! MAIL-B8 notification reap-all, presence GC (MAIL-M11), guest GC (M43), and
//! the SM-B13 stuck-process sweep. Timestamps live in `metadata` jsonb (audit
//! trigger writes `to_jsonb(NOW())` — ISO strings), so age predicates cast
//! `(metadata->>'updated_at')::timestamptz` and coalesce to created_at for
//! never-updated rows. Runtime queries — no `.sqlx` cache.

use sqlx::{PgConnection, Row};
use uuid::Uuid;

/// Hand-written GC/sweep SQL. Services orchestrate; this holds SQL.
pub struct GcRepository;

impl GcRepository {
    pub fn new() -> Self {
        Self
    }

    /// MAIL-B8: reap ALL mail_notification rows past the age bound. Odoo's
    /// partner_share carve-out is dropped (no such flag in the port — the
    /// delta is recorded in port-notes). Returns the reaped count.
    pub async fn notification_gc(
        conn: &mut PgConnection,
        retention_days: i64,
    ) -> Result<u64, sqlx::Error> {
        let res = sqlx::query(r#"
            DELETE FROM messaging.mail_notifications
            WHERE COALESCE((metadata->>'updated_at'), (metadata->>'created_at'))::timestamptz
                  < NOW() - make_interval(days => $1::int)
        "#)
            .bind(retention_days)
            .execute(&mut *conn)
            .await?;
        Ok(res.rows_affected())
    }

    /// MAIL-M11: reap presence rows whose last_poll went stale (Odoo's
    /// vacuum-in-code, 12h default — the route/config feeds the bound).
    pub async fn presence_gc(
        conn: &mut PgConnection,
        stale_hours: i64,
    ) -> Result<u64, sqlx::Error> {
        let res = sqlx::query(r#"
            DELETE FROM messaging.mail_presences
            WHERE last_poll < NOW() - make_interval(hours => $1::int)
        "#)
            .bind(stale_hours)
            .execute(&mut *conn)
            .await?;
        Ok(res.rows_affected())
    }

    /// M43: reap guests whose last_connection_dt went stale (60min default —
    /// a guest persona is only worth keeping while its browser might return).
    /// Soft-delete shape (active = FALSE) — hard delete stays a maintenance op.
    pub async fn guest_gc(
        conn: &mut PgConnection,
        stale_minutes: i64,
    ) -> Result<u64, sqlx::Error> {
        let res = sqlx::query(r#"
            UPDATE messaging.mail_guests
            SET active = FALSE,
                metadata = jsonb_set(metadata, '{deleted_at}', to_jsonb(NOW()))
            WHERE active
              AND last_connection_dt < NOW() - make_interval(mins => $1::int)
        "#)
            .bind(stale_minutes)
            .execute(&mut *conn)
            .await?;
        Ok(res.rows_affected())
    }

    /// SM-B13 step 1: find Sms rows stuck in `'process'` past the threshold.
    /// `state = 'process'` means a drainer claimed the row and never wrote an
    /// outcome — the row's updated_at IS the claim time. Returns (ids, the
    /// oldest row's age in minutes).
    pub async fn find_stuck_process(
        conn: &mut PgConnection,
        stuck_threshold_minutes: i64,
    ) -> Result<(Vec<Uuid>, i64), sqlx::Error> {
        let rows = sqlx::query(r#"
            SELECT id,
                   EXTRACT(EPOCH FROM (
                       NOW() - COALESCE((metadata->>'updated_at'),
                                         (metadata->>'created_at'))::timestamptz
                   )) / 60.0 AS age_minutes
            FROM messaging.sms
            WHERE state = 'process'
              AND COALESCE((metadata->>'updated_at'), (metadata->>'created_at'))::timestamptz
                  < NOW() - make_interval(mins => $1::int)
            ORDER BY COALESCE((metadata->>'updated_at'), (metadata->>'created_at'))::timestamptz
        "#)
            .bind(stuck_threshold_minutes)
            .fetch_all(&mut *conn)
            .await?;
        let oldest = rows
            .iter()
            .map(|r| r.try_get::<f64, _>("age_minutes").unwrap_or(0.0).floor() as i64)
            .max()
            .unwrap_or(0);
        let ids = rows.iter().filter_map(|r| r.try_get::<Uuid, _>("id").ok()).collect();
        Ok((ids, oldest))
    }

    /// SM-B13 step 2: bounded re-queue — stuck rows go back to `'outgoing'`
    /// (the queue's claimable state) UNLESS they already carry a `swept_at`
    /// metadata marker (a previous sweep's re-queue that went stuck AGAIN is
    /// left in place — the bound that prevents an infinite retry loop). The
    /// marker doubles as the new stuck-measurement start: updated_at is
    /// stamped now, swept_at records WHY.
    pub async fn requeue_stuck_process(
        conn: &mut PgConnection,
        ids: &[Uuid],
    ) -> Result<u64, sqlx::Error> {
        if ids.is_empty() {
            return Ok(0);
        }
        let res = sqlx::query(r#"
            UPDATE messaging.sms
            SET state = 'outgoing',
                metadata = jsonb_set(
                    jsonb_set(metadata, '{updated_at}', to_jsonb(NOW())),
                    '{swept_at}', to_jsonb(NOW()))
            WHERE id = ANY($1)
              AND state = 'process'
              AND (metadata->>'swept_at') IS NULL
        "#)
            .bind(ids)
            .execute(&mut *conn)
            .await?;
        Ok(res.rows_affected())
    }
}

impl Default for GcRepository {
    fn default() -> Self {
        Self::new()
    }
}
