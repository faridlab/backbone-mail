//! The outgoing-email queue service (hand-written; user-owned).
//!
//! Messaging owns the persisted `mails` queue row + its state machine (MAIL-M2)
//! but holds NO transport: increment 3 put the [`MailApiPort`] seam inside
//! [`Self::process_queue`] — claim (MMB-4 + the MAIL-M2 pre-write, one tx),
//! send through the port the COMPOSING APP supplies (its adapter rides
//! backbone-email/lettre; the module never reads env or opens sockets), then
//! apply each verdict in its own short tx. The drainer still stages a
//! `MailDispatchRequested` event per row (the relay carrier-of-record +
//! backbone-notification observability), but the row's fate is decided by the
//! port, not the event.
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

use crate::application::service::mail_ports::{
    mail_headers_from_json, MailApiPort, MailHeaderError, MailSendRequest,
};
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
    /// Per-mail headers refused by the single-line guard — the row is never
    /// written (a typed refusal; there is no sanitize-to-empty path).
    #[error("refused mail headers: {0}")]
    Header(#[from] MailHeaderError),
}

/// What one drain pass did.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct MailDrainOutcome {
    pub claimed: usize,
    /// Increment 3: transport verdicts applied (sum of the two = claimed).
    pub sent: usize,
    pub failed: usize,
}

pub struct MailQueueWriteService {
    pool: sqlx::PgPool,
}

impl MailQueueWriteService {
    pub fn new(pool: sqlx::PgPool) -> Self {
        Self { pool }
    }

    /// Enqueue an outgoing mail (state='outgoing'). Stages `MailQueued` in-tx.
    ///
    /// `headers` — optional per-mail custom RFC 5322 headers (a JSON object of
    /// name → string). Validated BEFORE any write: a CR/LF in a name or value
    /// (the header-injection guard) or a non-object shape refuses the enqueue
    /// with [`MailQueueError::Header`] — nothing is persisted. `None` stores
    /// the column's empty-object default.
    #[allow(clippy::too_many_arguments)]
    pub async fn enqueue(
        &self,
        mail_message_id: Uuid,
        email_to: &str,
        email_cc: Option<&str>,
        reply_to: Option<&str>,
        headers: Option<&serde_json::Value>,
        scheduled_date: Option<DateTime<Utc>>,
        model: Option<&str>,
        res_id: Option<Uuid>,
    ) -> Result<Uuid, MailQueueError> {
        if email_to.trim().is_empty() {
            return Err(MailQueueError::Invalid("email_to is required".into()));
        }
        // The guard runs before the transaction opens: a refused header never
        // writes anything, and the error names the exact defect.
        let headers_json = match headers {
            Some(value) => {
                mail_headers_from_json(value)?;
                value.clone()
            }
            None => serde_json::json!({}),
        };
        let id = Uuid::new_v4();
        let mut tx = self.pool.begin().await?;
        MailQueueRepository::enqueue(&mut tx, id, mail_message_id, email_to, email_cc, reply_to, &headers_json, scheduled_date).await?;
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
    /// pickup). Increment 3 put the transport INSIDE the loop: one claim tx per
    /// batch (`outgoing → exception` pre-write + a `MailDispatchRequested` event
    /// staged per row, unchanged), then each claimed row is handed to the
    /// [`MailApiPort`] and the verdict applied in its own short tx via
    /// [`Self::mark_sent`] / [`Self::mark_failed`] — commit-per-row, so one
    /// poisoned recipient never rolls back the batch's good sends (the SMS
    /// drainer's shape).
    ///
    /// A hard port error still leaves the row claimed `exception` (the
    /// MAIL-M2 pre-write) — diagnosable, manually requeueable, never
    /// silently retryable.
    pub async fn process_queue(
        &self,
        port: &dyn MailApiPort,
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
                    // The dispatch request: backbone-notification / observability
                    // consume it (the relay carrier-of-record). Correlation = the
                    // mail row id; the transport verdict lands on the row itself.
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

            // Send + apply each verdict in its own unit (commit per row).
            for row in &claimed {
                // Per-mail headers: the row's JSONB object through the same
                // guard enqueue applies. A row written OUTSIDE the sanctioned
                // enqueue (raw SQL, generic CRUD) can hold anything — a
                // malformed object fails the row loudly here (state is already
                // 'exception' from the claim; the refusal becomes its failure
                // reason), never silently mailing partial headers.
                let row_headers = match mail_headers_from_json(&row.headers) {
                    Ok(map) => map,
                    Err(refusal) => {
                        self.mark_failed(
                            row.id,
                            "unknown",
                            Some(&format!("malformed per-mail headers on the queue row: {refusal}")),
                        )
                        .await?;
                        total.failed += 1;
                        continue;
                    }
                };
                let result = port
                    .send(&MailSendRequest {
                        mail_id: row.id,
                        mail_message_id: row.mail_message_id,
                        from: row.email_from.clone().unwrap_or_default(),
                        to: split_recipients(row.email_to.as_deref(), row.email_cc.as_deref()),
                        subject: row.subject.clone(),
                        body_html: row.body.clone().unwrap_or_default(),
                        // The queue carries no parent-reference column (Odoo's
                        // mail.message doesn't either — reply collation lives in
                        // inbound routing, not the outbound envelope). The port
                        // shape keeps the field so a threading producer can set
                        // it; the drainer has nothing to seed it with.
                        in_reply_to: None,
                        headers: row_headers,
                    })
                    .await;
                match result {
                    Ok(_) => {
                        self.mark_sent(row.id).await?;
                        total.sent += 1;
                    }
                    Err(f) => {
                        let failure_type = normalize_failure_type(&f.failure_type);
                        self.mark_failed(row.id, failure_type, Some(&f.message)).await?;
                        total.failed += 1;
                    }
                }
            }

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

/// email_to + email_cc split into envelope recipients (comma-separated lists
/// on the row — the notification pump joins them, the port wants a Vec).
fn split_recipients(to: Option<&str>, cc: Option<&str>) -> Vec<String> {
    let mut out = Vec::new();
    for list in [to, cc].into_iter().flatten() {
        for addr in list.split(',') {
            let addr = addr.trim();
            if !addr.is_empty() {
                out.push(addr.to_string());
            }
        }
    }
    out
}

/// Fold a transport failure code onto the `mail_failure_type` DB vocabulary
/// (`mail_smtp`, `mail_email_invalid`, `mail_bounce`, `mail_blacklist`,
/// `mail_recipient`, `mail_server`, `unknown`). Odoo's wider mail.mail
/// vocabulary maps: `mail_email_missing` → `mail_recipient`,
/// `mail_from_invalid`/`mail_from_missing` → `mail_server` (an envelope-sender
/// verdict is a server-side verdict), `mail_spam` → `unknown` (provider
/// content policy — no honest bucket). Anything unrecognized → `unknown`
/// (never a DB cast error from an adapter's creative error string).
pub fn normalize_failure_type(raw: &str) -> &'static str {
    match raw {
        "mail_smtp" => "mail_smtp",
        "mail_email_invalid" => "mail_email_invalid",
        "mail_bounce" => "mail_bounce",
        "mail_blacklist" => "mail_blacklist",
        "mail_recipient" => "mail_recipient",
        "mail_server" => "mail_server",
        "mail_email_missing" => "mail_recipient",
        "mail_from_invalid" | "mail_from_missing" => "mail_server",
        _ => "unknown",
    }
}

#[cfg(test)]
mod tests {
    use super::{normalize_failure_type, split_recipients};

    #[test]
    fn odoo_vocabulary_folds_onto_the_db_enum() {
        assert_eq!(normalize_failure_type("mail_smtp"), "mail_smtp");
        assert_eq!(normalize_failure_type("mail_email_missing"), "mail_recipient");
        assert_eq!(normalize_failure_type("mail_from_invalid"), "mail_server");
        assert_eq!(normalize_failure_type("mail_spam"), "unknown");
        assert_eq!(normalize_failure_type("provider-500"), "unknown");
    }

    #[test]
    fn recipients_split_and_trim() {
        assert_eq!(
            split_recipients(Some("a@b.c, d@e.f"), Some(" cc@g.h ")),
            vec!["a@b.c".to_string(), "d@e.f".to_string(), "cc@g.h".to_string()]
        );
        assert!(split_recipients(None, None).is_empty());
    }
}
