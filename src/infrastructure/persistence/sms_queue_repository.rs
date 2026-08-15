//! Repository for the sms send queue (hand-written; user-owned).
//!
//! Holds the SQL for [`crate::application::service::SmsWriteService`]: enqueue with
//! uuid correlation (SM-M1/SM-M21 — NO FK, the tracker outlives the sms row's GC),
//! the MMB-4 `FOR UPDATE SKIP LOCKED` drain claim (one claim per row per pass;
//! concurrent workers drain disjoint sets; a crashed worker's claim expires with the
//! transaction), and the state-guarded outcome advances.

use sqlx::{PgConnection, Row};
use uuid::Uuid;

/// A claimed sms row, as the drainer holds it between claim and outcome.
pub struct SmsQueueRow {
    pub id: Uuid,
    pub uuid: String,
    pub number: String,
    pub body: String,
    pub mail_message_id: Option<Uuid>,
}

/// Hand-written sms queue SQL.
pub struct SmsQueueRepository;

impl SmsQueueRepository {
    pub fn new() -> Self {
        Self
    }
}

impl Default for SmsQueueRepository {
    fn default() -> Self {
        Self::new()
    }
}

impl SmsQueueRepository {
    /// Enqueue an sms for send: mint the `sms` row (`state='outgoing'`) and its
    /// uuid-correlated `sms_tracker`. Runs on the caller's open transaction so the
    /// row, the tracker, and the `SmsCreated` outbox event (TR-SM-1 queue re-arm)
    /// commit atomically.
    pub async fn enqueue(
        conn: &mut PgConnection,
        id: Uuid,
        sms_uuid: &str,
        number: &str,
        body: &str,
        mail_message_id: Option<Uuid>,
        notification_id: Option<Uuid>,
    ) -> Result<(), sqlx::Error> {
        sqlx::query(
            r#"INSERT INTO messaging.sms (id, uuid, number, body, state, mail_message_id)
               VALUES ($1,$2,$3,$4,'outgoing'::sms_state,$5)"#,
        )
        .bind(id)
        .bind(sms_uuid)
        .bind(number)
        .bind(body)
        .bind(mail_message_id)
        .execute(&mut *conn)
        .await?;

        sqlx::query(
            r#"INSERT INTO messaging.sms_trackers (sms_uuid, message_id, notification_id, state)
               VALUES ($1,$2,$3,'process'::mail_notification_status)
               ON CONFLICT (sms_uuid) DO NOTHING"#,
        )
        .bind(sms_uuid)
        .bind(mail_message_id)
        .bind(notification_id)
        .execute(&mut *conn)
        .await?;
        Ok(())
    }

    /// MMB-4 drain claim: `outgoing → process` over the batch, one statement, with
    /// `FOR UPDATE SKIP LOCKED` inside the claim's subselect. Concurrent drainers
    /// claim disjoint sets; a drainer that crashes mid-claim leaves no phantom
    /// claims (the lock and the row-state revert with the transaction).
    pub async fn claim_batch_for_drain(
        conn: &mut PgConnection,
        batch: i64,
    ) -> Result<Vec<SmsQueueRow>, sqlx::Error> {
        let rows = sqlx::query(
            r#"UPDATE messaging.sms AS s
               SET state = 'process'::sms_state
               WHERE s.id IN (
                   SELECT id FROM messaging.sms
                   WHERE state = 'outgoing'::sms_state
                   ORDER BY id
                   LIMIT $1
                   FOR UPDATE SKIP LOCKED
               )
               RETURNING s.id, s.uuid, s.number, s.body, s.mail_message_id"#,
        )
        .bind(batch)
        .fetch_all(&mut *conn)
        .await?;
        Ok(rows
            .iter()
            .map(|r| SmsQueueRow {
                id: r.get("id"),
                uuid: r.get("uuid"),
                number: r.get("number"),
                body: r.get("body"),
                mail_message_id: r.get("mail_message_id"),
            })
            .collect())
    }

    /// State-guarded outcome advance for a drained row: `process → pending` (LABEL
    /// 'Sent' — accepted, awaiting delivery report), `process → sent`, or
    /// `process → error` (failure_type set). The state guard (`AND state='process'`)
    /// plus the SM-B6 trigger make replays and regressions impossible; `Ok(false)`
    /// = the row was already advanced (a replayed result).
    #[allow(clippy::too_many_arguments)]
    pub async fn apply_outcome(
        conn: &mut PgConnection,
        id: Uuid,
        target_state: &str,
        failure_type: Option<&str>,
        error_message: Option<&str>,
        iap_status_code: Option<i32>,
    ) -> Result<bool, sqlx::Error> {
        let updated = sqlx::query_scalar::<_, Uuid>(
            r#"UPDATE messaging.sms
               SET state = $2::sms_state,
                   failure_type = $3::sms_failure_type,
                   error_message = $4,
                   iap_status_code = $5
               WHERE id = $1 AND state = 'process'::sms_state
               RETURNING id"#,
        )
        .bind(id)
        .bind(target_state)
        .bind(failure_type)
        .bind(error_message)
        .bind(iap_status_code)
        .fetch_optional(&mut *conn)
        .await?;
        Ok(updated.is_some())
    }

    /// Set the tracker's state to mirror the sms row's (the tracker is the
    /// notification pump's bridge; its `state` column starts 'process').
    pub async fn mirror_tracker_state(
        conn: &mut PgConnection,
        sms_uuid: &str,
        tracker_state: &str,
    ) -> Result<(), sqlx::Error> {
        sqlx::query(
            r#"UPDATE messaging.sms_trackers
               SET state = $2::mail_notification_status
               WHERE sms_uuid = $1"#,
        )
        .bind(sms_uuid)
        .bind(tracker_state)
        .execute(&mut *conn)
        .await?;
        Ok(())
    }

    /// Count rows still queued (drain-progress reporting).
    pub async fn count_outgoing(conn: &mut PgConnection) -> Result<i64, sqlx::Error> {
        sqlx::query_scalar::<_, i64>(r#"SELECT COUNT(*) FROM messaging.sms WHERE state = 'outgoing'::sms_state"#)
            .fetch_one(&mut *conn)
            .await
    }
}
