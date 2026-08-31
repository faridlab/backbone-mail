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
//!
//! # Per-mail custom headers — the precedence contract
//!
//! A queue row may carry a `headers` JSONB object (name → string). It flows:
//! enqueue (validated, [`mail_headers_from_json`]) → the row → the claim →
//! [`MailSendRequest::headers`] → the transport's header map. Precedence, in
//! order:
//!
//! 1. **Envelope/structural headers** (From, To, Cc, Subject, Message-ID,
//!    MIME-*, Date) belong to the transport message, never to the custom map.
//!    A per-mail entry with such a name lands in the custom-header set the
//!    transport appends AFTER its own — it cannot replace an envelope value
//!    the transport itself controls.
//! 2. **Structured threading wins, and exclusively.** When
//!    [`MailSendRequest::in_reply_to`] is set, the gateway derives
//!    `In-Reply-To`/`References` from it; a per-mail entry carrying either
//!    name (case-insensitive) is REFUSED — a typed failure, never a silent
//!    override or duplicate (see [`TRANSPORT_THREADING_HEADERS`]).
//! 3. **Everything else passes through verbatim**, after the CR/LF guard.
//!
//! The CR/LF guard (header-injection safety): a header NAME or VALUE
//! containing `\r` or `\n` is refused wherever headers enter the module —
//! at enqueue (typed [`MailHeaderError`], the row is never written) and again
//! at the gateway merge (typed `MailSendFailure`, the row lands `exception`
//! with the refusal as its failure reason). Refusal is always loud; there is
//! deliberately NO sanitize-to-empty path (a smuggled header must surface,
//! never vanish).

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use uuid::Uuid;

/// The threading header names the gateway derives from
/// [`MailSendRequest::in_reply_to`] when it is set. A per-mail header with
/// one of these names (compared case-insensitively, as RFC 5322 field names
/// are) is refused while the structured field also carries a value — both
/// sources claiming the thread linkage is an ambiguous instruction, and the
/// send fails loudly instead of silently picking a winner. When
/// `in_reply_to` is unset, per-mail entries with these names flow through
/// (they are then the ONLY source of threading).
pub const TRANSPORT_THREADING_HEADERS: [&str; 2] = ["In-Reply-To", "References"];

/// A typed refusal from the per-mail header guard. Never a silent sanitize:
/// each variant names the exact defect so the caller can persist it as a
/// failure reason.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum MailHeaderError {
    /// The headers column is not a JSON object of name → string.
    #[error("headers must be a JSON object of string → string, found {found}")]
    NotAnObject { found: &'static str },
    /// A header name is empty.
    #[error("header name is empty (value {value:?})")]
    EmptyName { value: String },
    /// A header NAME carries a line break — the header-injection guard.
    #[error("header name {name:?} contains a CR/LF line break — refused")]
    LineBreakInName { name: String },
    /// A header VALUE carries a line break — the header-injection guard.
    #[error("header {name:?} value contains a CR/LF line break — refused")]
    LineBreakInValue { name: String },
    /// A header value is not a JSON string (e.g. a number or nested object).
    #[error("header {name:?} value is not a string")]
    NonStringValue { name: String },
}

/// The single-line guard shared by every entry point: a header name/value
/// containing `\r` or `\n` is refused (an attempt to smuggle additional
/// header lines — e.g. a `Bcc:`/`To:` injection — through one value).
/// Names must also be non-empty.
pub fn validate_mail_header(name: &str, value: &str) -> Result<(), MailHeaderError> {
    if name.is_empty() {
        return Err(MailHeaderError::EmptyName { value: value.to_string() });
    }
    if name.contains('\r') || name.contains('\n') {
        return Err(MailHeaderError::LineBreakInName { name: name.to_string() });
    }
    if value.contains('\r') || value.contains('\n') {
        return Err(MailHeaderError::LineBreakInValue { name: name.to_string() });
    }
    Ok(())
}

/// Convert the row's `headers` JSONB (or any caller-supplied JSON) into the
/// transport's `HashMap<String, String>`, enforcing the same single-line
/// guard on the way through. Anything that is not an object of strings is a
/// typed refusal — the alternative (silently skipping entries) would let a
/// malformed row mail half its intended headers with no visible defect.
pub fn mail_headers_from_json(
    value: &serde_json::Value,
) -> Result<HashMap<String, String>, MailHeaderError> {
    let serde_json::Value::Object(map) = value else {
        return Err(MailHeaderError::NotAnObject {
            found: match value {
                serde_json::Value::Null => "null",
                serde_json::Value::Bool(_) => "a boolean",
                serde_json::Value::Number(_) => "a number",
                serde_json::Value::String(_) => "a string",
                serde_json::Value::Array(_) => "an array",
                serde_json::Value::Object(_) => unreachable!(),
            },
        });
    };
    let mut out = HashMap::with_capacity(map.len());
    for (name, val) in map {
        let serde_json::Value::String(text) = val else {
            return Err(MailHeaderError::NonStringValue { name: name.clone() });
        };
        validate_mail_header(name, text)?;
        out.insert(name.clone(), text.clone());
    }
    Ok(out)
}

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
    /// Per-mail custom headers from the queue row (already through the
    /// single-line guard). Supplements — never overrides — the transport's
    /// own headers; see the module-doc precedence contract above.
    pub headers: HashMap<String, String>,
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn object_of_strings_converts_verbatim() {
        let json = serde_json::json!({
            "X-Campaign-Id": "summer-2026",
            "List-Unsubscribe": "<https://example.com/unsub>",
        });
        let map = mail_headers_from_json(&json).expect("clean headers convert");
        assert_eq!(map.get("X-Campaign-Id").map(String::as_str), Some("summer-2026"));
        assert_eq!(
            map.get("List-Unsubscribe").map(String::as_str),
            Some("<https://example.com/unsub>")
        );
        // The empty default round-trips to an empty map.
        assert!(mail_headers_from_json(&serde_json::json!({})).unwrap().is_empty());
    }

    #[test]
    fn crlf_smuggle_in_value_is_a_typed_refusal() {
        let json = serde_json::json!({
            "X-Campaign-Id": "summer\r\nBcc: victim@example.com",
        });
        match mail_headers_from_json(&json) {
            Err(MailHeaderError::LineBreakInValue { name }) => {
                assert_eq!(name, "X-Campaign-Id");
            }
            other => panic!("CRLF in value must refuse with LineBreakInValue, got {other:?}"),
        }
    }

    #[test]
    fn crlf_smuggle_in_name_is_a_typed_refusal() {
        let json = serde_json::json!({ "X-Fine: 1\r\nBcc: a@b.c": "value" });
        match mail_headers_from_json(&json) {
            Err(MailHeaderError::LineBreakInName { name }) => {
                assert!(name.contains("\r\n"));
            }
            other => panic!("CRLF in name must refuse with LineBreakInName, got {other:?}"),
        }
        // Bare LF and bare CR are refused too — any line break smuggles.
        assert!(mail_headers_from_json(&serde_json::json!({ "X-N": "a\nb" })).is_err());
        assert!(mail_headers_from_json(&serde_json::json!({ "X-R": "a\rb" })).is_err());
    }

    #[test]
    fn non_object_shapes_and_non_string_values_are_refused() {
        assert!(matches!(
            mail_headers_from_json(&serde_json::json!(["not", "an", "object"])),
            Err(MailHeaderError::NotAnObject { found: "an array" })
        ));
        assert!(matches!(
            mail_headers_from_json(&serde_json::json!("a string")),
            Err(MailHeaderError::NotAnObject { found: "a string" })
        ));
        assert!(matches!(
            mail_headers_from_json(&serde_json::json!({ "X-Num": 42 })),
            Err(MailHeaderError::NonStringValue { name }) if name == "X-Num"
        ));
        assert!(matches!(
            mail_headers_from_json(&serde_json::json!({ "": "empty name" })),
            Err(MailHeaderError::EmptyName { .. })
        ));
    }
}
