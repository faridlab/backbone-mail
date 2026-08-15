//! The outgoing-email queue service (hand-written; user-owned).
//!
//! Messaging owns the persisted `mails` queue row + its state machine (MAIL-M2),
//! but sends NO mail itself: the drainer STAGES a `MailDispatchRequested` outbox
//! event and backbone-notification / backbone-email consume it and do SMTP. The
//! state machine stays here — `mark_sent` / `mark_failed` are the consumer's
//! callbacks.
//!
//! Crash-safety (MAIL-M2, preserved verbatim): the claim tx pre-writes
//! `state='exception'` BEFORE the dispatch is attempted — a crash between claim
//! and the consumer's verdict leaves a diagnosable failure, never a
//! falsely-reclaimable 'outgoing' (Odoo's `_send` writes exception before SMTP for
//! exactly this reason; `mails.state` is NOT label-inverted and gets NO monotonic
//! guard — the exception→sent completion of the cycle must stay legal).
//!
//! The pickup is MMB-4's `FOR UPDATE SKIP LOCKED` (ADR-0020 pickup-lock standard):
//! concurrent drainers claim disjoint sets.

use chrono::{DateTime, Utc};
use uuid::Uuid;

use crate::domain::event::{record_channel, stage_bus_event};
use crate::infrastructure::persistence::mail_queue_repository::MailQueueRepository;

/// The default drain batch (Odoo `process_email_queue(batch_size=1000)`, SJ-MAIL-1).
pub const DEFAULT_MAIL_BATCH: i64 = 1000;

#[derive(Debug, thiserror::Error)]
pub enum MailQueueError {
    #[error("db: {0}")]
    Db(#[from] sqlx::Error),
    #[error("invalid input: {0}")]
    Invalid(String),
}

/// What one drain pass did.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct MailDrainOutcome {
    pub claimed: usize,
}

pub struct MailQueueWriteService {
    pool: sqlx::PgPool,
}

impl MailQueueWriteService {
    pub fn new(pool: sqlx::PgPool) -> Self {
        Self { pool }
    }

    /// Enqueue an outgoing mail (state='outgoing'). Stages `MailQueued` in-tx.
    #[allow(clippy::too_many_arguments)]
    pub async fn enqueue(
        &self,
        mail_message_id: Uuid,
        email_to: &str,
        email_cc: Option<&str>,
        reply_to: Option<&str>,
        scheduled_date: Option<DateTime<Utc>>,
        model: Option<&str>,
        res_id: Option<Uuid>,
    ) -> Result<Uuid, MailQueueError> {
        if email_to.trim().is_empty() {
            return Err(MailQueueError::Invalid("email_to is required".into()));
        }
        let id = Uuid::new_v4();
        let mut tx = self.pool.begin().await?;
        MailQueueRepository::enqueue(&mut tx, id, mail_message_id, email_to, email_cc, reply_to, scheduled_date).await?;
        let channel_key = match (model, res_id) {
            (Some(m), Some(r)) => record_channel(m, r),
            _ => format!("mail.message_{mail_message_id}"),
        };
        stage_bus_event(
            &mut tx,
            "MailQueued",
            "Mail",
            id,
            channel_key,
            "MailQueued",
            serde_json::json!({ "mail_id": id, "mail_message_id": mail_message_id, "email_to": email_to }),
        )
        .await?;
        tx.commit().await?;
        Ok(id)
    }

    /// The drainer (`process_email_queue`, SJ-MAIL-1 — with the MMB-4 SKIP LOCKED
    /// pickup). One claim tx per batch: `outgoing → exception` (the MAIL-M2
    /// pre-write) over `FOR UPDATE SKIP LOCKED` rows, with a
    /// `MailDispatchRequested` event staged per row IN the same tx — so the
    /// "attempting dispatch" signal is durable exactly when the row leaves the
    /// claimable set. The SMTP consumer advances the row afterwards via
    /// [`Self::mark_sent`] / [`Self::mark_failed`].
    pub async fn process_queue(
        &self,
        batch: i64,
        max_batches: usize,
    ) -> Result<MailDrainOutcome, MailQueueError> {
        let mut total = MailDrainOutcome::default();
        let mut passes = 0usize;
        loop {
            let claimed = {
                let mut tx = self.pool.begin().await?;
                let rows = MailQueueRepository::claim_batch_for_drain(
                    &mut tx, batch, Utc::now())
                    .await?;
                for row in &rows {
                    // The dispatch request: consumed by backbone-notification /
                    // backbone-email, which own SMTP. Correlation = the mail row id.
                    stage_bus_event(
                        &mut tx,
                        "MailDispatchRequested",
                        "Mail",
                        row.id,
                        // The dispatch consumer's stream — the record channel when
                        // we can derive it cheaply, else the message pseudo-channel.
                        format!("mail.message_{}", row.mail_message_id),
                        "MailDispatchRequested",
                        serde_json::json!({
                            "mail_id": row.id,
                            "mail_message_id": row.mail_message_id,
                            "email_to": row.email_to,
                            "email_cc": row.email_cc,
                            "reply_to": row.reply_to,
                        }),
                    )
                    .await?;
                }
                tx.commit().await?;
                rows
            };
            if claimed.is_empty() {
                break;
            }
            total.claimed += claimed.len();
            passes += 1;
            if max_batches != 0 && passes >= max_batches {
                break;
            }
        }
        Ok(total)
    }

    /// The SMTP consumer's 250 callback (state-guarded; a redelivered dispatch
    /// event is a no-op). Also advances the notification rows linked through
    /// `mail_mail_id_int`: `ready → process → pending` (LABEL 'Sent' — handed to
    /// MTA, awaiting DSN), per `_postprocess_sent_message` (§2.2).
    pub async fn mark_sent(&self, mail_id: Uuid) -> Result<bool, MailQueueError> {
        let mut tx = self.pool.begin().await?;
        let advanced = MailQueueRepository::mark_sent(&mut tx, mail_id).await?;
        if advanced {
            Self::advance_linked_notifications(&mut tx, mail_id, "pending", None, None).await?;
        }
        tx.commit().await?;
        Ok(advanced)
    }

    /// The SMTP consumer's failure callback (MAIL-M2 §2.3 failure vocabulary).
    /// The linked notifications go to `'exception'`.
    pub async fn mark_failed(
        &self,
        mail_id: Uuid,
        failure_type: &str,
        reason: Option<&str>,
    ) -> Result<bool, MailQueueError> {
        let mut tx = self.pool.begin().await?;
        let advanced = MailQueueRepository::mark_failed(&mut tx, mail_id, failure_type, reason).await?;
        if advanced {
            Self::advance_linked_notifications(&mut tx, mail_id, "exception", Some(failure_type), reason).await?;
        }
        tx.commit().await?;
        Ok(advanced)
    }

    /// Manual requeue (`resend_failed`: `exception → outgoing`, no automatic retry).
    ///
    /// NOTE — deliberate deviation from Odoo, forced by the SM-B6 lattice: Odoo's
    /// ignore-table lets a resent notification regress `exception → ready`, but the
    /// port's DB guard makes the terminal statuses terminal-once-set. The mail ROW
    /// re-enters the queue (mails.state carries no guard — MAIL-M2's crash-safety
    /// cycle needs exception→outgoing and exception→sent); the linked
    /// NOTIFICATIONS keep their terminal `exception` history instead of regressing
    /// — the resend's outcome will land on them as a fresh advance (e.g. a later
    /// `pending` write is refused by the guard, which is the intended strictness:
    /// a failed delivery attempt stays visible in the recipient's history).
    pub async fn requeue(&self, mail_id: Uuid) -> Result<bool, MailQueueError> {
        let mut tx = self.pool.begin().await?;
        let requeued = MailQueueRepository::requeue(&mut tx, mail_id).await?;
        tx.commit().await?;
        Ok(requeued)
    }

    /// Advance every notification linked to a mail row through the pump map,
    /// staging `NotificationStatusChanged` per row (the recipient's channel).
    async fn advance_linked_notifications(
        tx: &mut sqlx::PgConnection,
        mail_id: Uuid,
        target: &str,
        failure_type: Option<&str>,
        reason: Option<&str>,
    ) -> Result<(), MailQueueError> {
        let rows = sqlx::query(
            r#"SELECT id, res_partner_id FROM messaging.mail_notifications
               WHERE mail_mail_id_int = $1"#,
        )
        .bind(mail_id)
        .fetch_all(&mut *tx)
        .await?;
        for row in &rows {
            use sqlx::Row;
            let id: Uuid = row.get("id");
            let partner: Option<Uuid> = row.get("res_partner_id");
            let advanced = crate::infrastructure::persistence::message_pipeline_repository::MessagePipelineRepository::advance_notification_status(
                tx, id, target, failure_type, reason,
            )
            .await?;
            if let Some(new_status) = advanced {
                let channel_key = partner
                    .map(crate::domain::event::partner_channel)
                    .unwrap_or_else(|| format!("mail_{mail_id}"));
                stage_bus_event(
                    tx,
                    "NotificationStatusChanged",
                    "MailNotification",
                    id,
                    channel_key,
                    "NotificationStatusChanged",
                    serde_json::json!({
                        "notification_id": id, "mail_id": mail_id,
                        "notification_status": new_status,
                    }),
                )
                .await?;
            }
        }
        Ok(())
    }
}
