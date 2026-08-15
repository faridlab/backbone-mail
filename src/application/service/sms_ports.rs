//! The SmsApiPort — messaging's ONLY seam to a real SMS provider (hand-written;
//! user-owned).
//!
//! Odoo's per-provider extension point is `res.company._get_sms_api_class()`; the
//! port keeps that shape as a trait. **Increment 1 ships no real provider and no
//! HTTP** — only [`NoopSmsApi`], the test double. A Twilio/IAP adapter lands in a
//! later increment as another impl of this trait, behind env-var credentials only
//! (ADR-0024 interim posture — `docs/adr-notes/outbox-fence-and-credentials.md` §b).

use std::sync::{Arc, Mutex};

/// What the drainer asks the provider to do with one queued sms.
#[derive(Debug, Clone)]
pub struct SmsSendRequest {
    /// The uuid correlation key (G-SM1 — the provider echoes it back, the tracker
    /// keys on it, the webhook addresses by it).
    pub uuid: String,
    pub number: String,
    pub body: String,
}

/// The provider's synchronous verdict — the port of Odoo's IAP response states
/// (`IAP_TO_SMS_STATE_SUCCESS`). NOTE: `Delivered` normally only ever arrives via
/// the async webhook; the sync response can at best accept.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SmsSendOutcome {
    /// Provider accepted and is still working → sms.state `process` (rank 1).
    Processing,
    /// Accepted, awaiting the delivery report → sms.state `pending` (LABEL 'Sent').
    Accepted,
    /// Delivery confirmed (webhook-only in practice) → sms.state `sent`
    /// (LABEL 'Delivered').
    Delivered,
}

/// A provider-side failure, already mapped onto the `sms_failure_type` vocabulary
/// (SM §2.3: PROVIDER_TO_SMS_FAILURE_TYPE — server_error→sms_server,
/// wrong_number_format→sms_number_format, insufficient_credit→sms_credit,
/// unregistered→sms_acc, country_not_supported→sms_country_not_supported, …).
#[derive(Debug, Clone)]
pub struct SmsSendFailure {
    pub failure_type: String,
    pub message: String,
    pub iap_status_code: Option<i32>,
}

/// The provider seam. Implementations MUST be idempotent per uuid when called
/// at-least-once (the queue hands each row to exactly one drainer per pass, but a
/// consumer crash after send can replay).
#[async_trait::async_trait]
pub trait SmsApiPort: Send + Sync {
    async fn send(&self, req: &SmsSendRequest) -> Result<SmsSendOutcome, SmsSendFailure>;
}

/// The increment-1 NoOp/test double: records every request, replays the configured
/// outcome (default: `Accepted`), optionally fails with the configured failure.
/// No network, no provider, no secrets.
pub struct NoopSmsApi {
    pub outcome: SmsSendOutcome,
    pub failure: Option<SmsSendFailure>,
    pub delay: std::time::Duration,
    requests: Mutex<Vec<SmsSendRequest>>,
}

impl NoopSmsApi {
    pub fn accepting() -> Self {
        Self { outcome: SmsSendOutcome::Accepted, failure: None, delay: std::time::Duration::ZERO, requests: Mutex::new(Vec::new()) }
    }

    pub fn with_outcome(outcome: SmsSendOutcome) -> Self {
        Self { outcome, failure: None, delay: std::time::Duration::ZERO, requests: Mutex::new(Vec::new()) }
    }

    pub fn failing(failure_type: &str, message: &str) -> Self {
        Self {
            outcome: SmsSendOutcome::Accepted,
            failure: Some(SmsSendFailure {
                failure_type: failure_type.into(),
                message: message.into(),
                iap_status_code: None,
            }),
            delay: std::time::Duration::ZERO,
            requests: Mutex::new(Vec::new()),
        }
    }

    /// Every request the double has seen (test assertions / the duplicate-send
    /// proof).
    pub fn requests(&self) -> Vec<SmsSendRequest> {
        self.requests.lock().unwrap().clone()
    }
}

#[async_trait::async_trait]
impl SmsApiPort for NoopSmsApi {
    async fn send(&self, req: &SmsSendRequest) -> Result<SmsSendOutcome, SmsSendFailure> {
        if !self.delay.is_zero() {
            tokio::time::sleep(self.delay).await;
        }
        self.requests.lock().unwrap().push(req.clone());
        if let Some(f) = &self.failure {
            return Err(f.clone());
        }
        Ok(self.outcome.clone())
    }
}

/// Shared-handle helper for the concurrent-drainer tests.
pub fn shared_noop() -> Arc<NoopSmsApi> {
    Arc::new(NoopSmsApi::accepting())
}
