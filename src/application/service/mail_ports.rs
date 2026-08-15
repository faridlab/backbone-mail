//! The MailApiPort — messaging's ONLY seam to a real SMTP transport
//! (hand-written; user-owned).
//!
//! Odoo's extension point is `ir.mail_server.send_email()` (the whole SMTP hop
//! lives behind the server registry); the port keeps that shape as a trait.
//! The module owns the queue row and the state machine (MAIL-M2) — the drainer
//! claims, hands each row to the port, and applies the verdict via
//! `mark_sent` / `mark_failed`. The transport itself (lettre, backbone-email's
//! `SmtpProvider`) is wired by the COMPOSING APP, never by the module — the
//! module never reads env and never opens sockets.
//!
//! Mirrors [`crate::application::service::sms_ports::SmsApiPort`] exactly in
//! shape and contract (increment 3, MAIL-M26).

use std::sync::{Arc, Mutex};

use uuid::Uuid;

/// What the drainer asks the transport to do with one queued mail.
#[derive(Debug, Clone)]
pub struct MailSendRequest {
    /// The mails-row id — the correlation key (mark_sent/mark_failed address it;
    /// the MailDispatchRequested event carries it too).
    pub mail_id: Uuid,
    /// The underlying mail.message row (content already fetched by the claim).
    pub mail_message_id: Uuid,
    /// Envelope sender. Resolved by the app from the server-selection ladder
    /// (exact from_filter → domain → wildcard → first-by-sequence); the module
    /// supplies the row, the app picks the envelope.
    pub from: String,
    /// Envelope recipients (email_to + email_cc, already split by the claim).
    pub to: Vec<String>,
    pub subject: Option<String>,
    /// The rendered body (mail.message.body — html by convention).
    pub body_html: String,
    /// The References header seed, when the parent message had a message_id.
    pub in_reply_to: Option<String>,
}

/// The transport's synchronous verdict. SMTP is a 250-or-fail protocol — unlike
/// the SMS port there is no `Processing` middle state (a queued-provider shape
/// can extend this enum later without touching the contract).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MailSendOutcome {
    /// SMTP 250 — the MTA accepted the message for delivery.
    Accepted,
}

/// A transport-side failure. `failure_type` MUST be one of the
/// `MailFailureType` DB vocabulary (`mail_smtp`, `mail_email_invalid`,
/// `mail_bounce`, `mail_blacklist`, `mail_recipient`, `mail_server`,
/// `unknown`) — the drainer normalizes anything else to `unknown` before it
/// reaches the row (see `MailQueueWriteService::normalize_failure_type`).
///
/// Odoo's wider mail.mail vocabulary folds onto that set:
/// `mail_email_missing` → `mail_recipient` (no valid recipient),
/// `mail_from_invalid` / `mail_from_missing` → `mail_server` (envelope sender
/// rejected — a server-side verdict), `mail_spam` → `unknown` (provider policy,
/// no honest DB bucket).
#[derive(Debug, Clone)]
pub struct MailSendFailure {
    pub failure_type: String,
    pub message: String,
}

impl MailSendFailure {
    /// Convenience constructor for the common transport-error case.
    pub fn smtp(message: impl Into<String>) -> Self {
        Self { failure_type: "mail_smtp".into(), message: message.into() }
    }
}

/// The transport seam. Implementations MUST be idempotent per `mail_id` when
/// called at-least-once: the claim pre-writes `state='exception'`, so a crash
/// between send and verdict can replay the same row (the MAIL-M2 crash-safety
/// cycle; `mark_sent`'s state guard makes the replay a no-op row-side).
#[async_trait::async_trait]
pub trait MailApiPort: Send + Sync {
    async fn send(&self, req: &MailSendRequest) -> Result<MailSendOutcome, MailSendFailure>;
}

/// The NoOp/test double: records every request, replays the configured outcome
/// (default: `Accepted`), optionally fails with the configured failure. No
/// network, no secrets — increment-3 module tests use this; the app swaps in
/// the real SMTP adapter over backbone-email.
pub struct NoopMailApi {
    pub outcome: MailSendOutcome,
    pub failure: Option<MailSendFailure>,
    requests: Mutex<Vec<MailSendRequest>>,
}

impl NoopMailApi {
    pub fn accepting() -> Self {
        Self { outcome: MailSendOutcome::Accepted, failure: None, requests: Mutex::new(Vec::new()) }
    }

    pub fn failing(failure_type: &str, message: &str) -> Self {
        Self {
            outcome: MailSendOutcome::Accepted,
            failure: Some(MailSendFailure { failure_type: failure_type.into(), message: message.into() }),
            requests: Mutex::new(Vec::new()),
        }
    }

    /// Every request the double has seen (test assertions).
    pub fn requests(&self) -> Vec<MailSendRequest> {
        self.requests.lock().unwrap_or_else(|e| e.into_inner()).clone()
    }
}

#[async_trait::async_trait]
impl MailApiPort for NoopMailApi {
    async fn send(&self, req: &MailSendRequest) -> Result<MailSendOutcome, MailSendFailure> {
        self.requests.lock().unwrap_or_else(|e| e.into_inner()).push(req.clone());
        if let Some(f) = &self.failure {
            return Err(f.clone());
        }
        Ok(self.outcome.clone())
    }
}

/// Shared-handle helper for tests.
pub fn shared_noop() -> Arc<NoopMailApi> {
    Arc::new(NoopMailApi::accepting())
}
