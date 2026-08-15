//! Repository for the outgoing-email send queue `mails` (hand-written; user-owned).
//!
//! Holds the SQL for [`crate::application::service::MailQueueWriteService`]:
//! enqueue, the MMB-4 `FOR UPDATE SKIP LOCKED` drain claim with the MAIL-M2
//! crash-safety pre-write (`state='exception'` BEFORE the dispatch attempt), and
//! the consumer-side sent/failed callbacks. Messaging owns the queue row and its
//! state machine; SMTP itself lives downstream (backbone-notification /
//! backbone-email consume the staged `MailDispatchRequested` event and send).

use chrono::{DateTime, Utc};
use sqlx::{PgConnection, Row};
use uuid::Uuid;

/// A claimed mail row, as the drainer dispatches it.
pub struct MailQueueRow {
    pub id: Uuid,
    pub mail_message_id: Uuid,
    pub email_to: Option<String>,
    pub email_cc: Option<String>,
    pub reply_to: Option<String>,
}

/// Hand-written mail queue SQL.
pub struct MailQueueRepository;

impl MailQueueRepository {
    pub fn new() -> Self {
        Self
    }
}

impl Default for MailQueueRepository {
    fn default() -> Self {
        Self::new()
    }
}

impl MailQueueRepository {
    /// Enqueue an outgoing mail (state='outgoing'). Runs on the caller's open
    /// transaction (the notify pump enqueues in-tx with the notification rows).
    #[allow(clippy::too_many_arguments)]
    pub async fn enqueue(
        conn: &mut PgConnection,
        id: Uuid,
        mail_message_id: Uuid,
        email_to: &str,
        email_cc: Option<&str>,
        reply_to: Option<&str>,
        scheduled_date: Option<DateTime<Utc>>,
    ) -> Result<(), sqlx::Error> {
        sqlx::query(
            r#"INSERT INTO messaging.mails
                 (id, mail_message_id, state, email_to, email_cc, reply_to, scheduled_date)
               VALUES ($1,$2,'outgoing'::mail_state,$3,$4,$5,$6)"#,
        )
        .bind(id)
        .bind(mail_message_id)
        .bind(email_to)
        .bind(email_cc)
        .bind(reply_to)
        .bind(scheduled_date)
        .execute(&mut *conn)
        .await?;
        Ok(())
    }

    /// MMB-4 drain claim + MAIL-M2 crash-safety pre-write, ONE statement:
    /// `outgoing → exception` over the batch via `FOR UPDATE SKIP LOCKED`. The row
    /// leaves the claim ALREADY marked `exception` — if anything crashes between
    /// the commit and the SMTP consumer's verdict, the queue is left a diagnosable
    /// failure, never a falsely-reclaimable 'outgoing' (Odoo's `_send` writes
    /// `state='exception'` before the SMTP attempt for exactly this reason).
    /// `scheduled_date <= now` mirrors the cron domain (a deferred row is not yet
    /// claimable).
    pub async fn claim_batch_for_drain(
        conn: &mut PgConnection,
        batch: i64,
        now: DateTime<Utc>,
    ) -> Result<Vec<MailQueueRow>, sqlx::Error> {
        let rows = sqlx::query(
            r#"UPDATE messaging.mails AS m
               SET state = 'exception'::mail_state, failure_type = 'unknown'::mail_failure_type
               WHERE m.id IN (
                   SELECT id FROM messaging.mails
                   WHERE state = 'outgoing'::mail_state
                     AND (scheduled_date IS NULL OR scheduled_date <= $2)
                   ORDER BY id
                   LIMIT $1
                   FOR UPDATE SKIP LOCKED
               )
               RETURNING m.id, m.mail_message_id, m.email_to, m.email_cc, m.reply_to"#,
        )
        .bind(batch)
        .bind(now)
        .fetch_all(&mut *conn)
        .await?;
        Ok(rows
            .iter()
            .map(|r| MailQueueRow {
                id: r.get("id"),
                mail_message_id: r.get("mail_message_id"),
                email_to: r.get("email_to"),
                email_cc: r.get("email_cc"),
                reply_to: r.get("reply_to"),
            })
            .collect())
    }

    /// The SMTP consumer's 250 callback: `outgoing|exception → sent`, state-guarded
    /// (a redelivered dispatch event is a no-op). MAIL-M2: `sent` means sent —
    /// mails.state is NOT label-inverted.
    pub async fn mark_sent(conn: &mut PgConnection, id: Uuid) -> Result<bool, sqlx::Error> {
        let updated = sqlx::query_scalar::<_, Uuid>(
            r#"UPDATE messaging.mails
               SET state = 'sent'::mail_state, failure_type = NULL, failure_reason = NULL
               WHERE id = $1 AND state IN ('outgoing'::mail_state, 'exception'::mail_state)
               RETURNING id"#,
        )
        .bind(id)
        .fetch_optional(&mut *conn)
        .await?;
        Ok(updated.is_some())
    }

    /// The SMTP consumer's failure callback: `→ exception` with the failure_type
    /// vocabulary (MAIL-M2 §2.3: mail_smtp / mail_server / mail_email_invalid /
    /// mail_bounce / mail_recipient / mail_blacklist / unknown). No automatic
    /// retry — an exception row stays exception until a human requeues.
    pub async fn mark_failed(
        conn: &mut PgConnection,
        id: Uuid,
        failure_type: &str,
        reason: Option<&str>,
    ) -> Result<bool, sqlx::Error> {
        let updated = sqlx::query_scalar::<_, Uuid>(
            r#"UPDATE messaging.mails
               SET state = 'exception'::mail_state,
                   failure_type = $2::mail_failure_type,
                   failure_reason = $3
               WHERE id = $1 AND state IN ('outgoing'::mail_state, 'exception'::mail_state)
               RETURNING id"#,
        )
        .bind(id)
        .bind(failure_type)
        .bind(reason)
        .fetch_optional(&mut *conn)
        .await?;
        Ok(updated.is_some())
    }

    /// Manual requeue (Odoo `resend_failed`): `exception → outgoing`. The
    /// monotonic guard does NOT cover mails.state (deliberate — MAIL-M2's
    /// crash-safety cycle needs exception→sent and exception→outgoing).
    pub async fn requeue(conn: &mut PgConnection, id: Uuid) -> Result<bool, sqlx::Error> {
        let updated = sqlx::query_scalar::<_, Uuid>(
            r#"UPDATE messaging.mails
               SET state = 'outgoing'::mail_state, failure_type = NULL, failure_reason = NULL
               WHERE id = $1 AND state = 'exception'::mail_state
               RETURNING id"#,
        )
        .bind(id)
        .fetch_optional(&mut *conn)
        .await?;
        Ok(updated.is_some())
    }
}
