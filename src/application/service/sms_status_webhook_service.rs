//! The SMS delivery-status webhook service (hand-written; user-owned).
//!
//! Port of the provider→Odoo status callback (SM-B2). ADR-0021 scheme
//! `hmac_raw_body`: the provider POSTs the raw JSON body it signed, plus a
//! hex `X-Signature` header of `HMAC-SHA256(SMS_WEBHOOK_SECRET, raw_body)`.
//!
//! **Fail-closed ordering is the whole design:** signature and timestamp are
//! verified BEFORE any state write — a bad signature, missing header, or stale
//! timestamp returns an error having touched NOTHING (the no-write-on-bad-sig
//! test snapshots all rows to prove it). There is deliberately no repository
//! layer here: verification is pure crypto, and the verified path delegates to
//! [`SmsWriteService::advance_state`] — the ONE public seam that runs the
//! sms state advance + tracker mirror + notification pump in lockstep. Its
//! `apply_outcome` state-guard on `'process'` makes provider retries
//! (replays) idempotent no-ops; the SM-B6 audit trigger is the backstop.

use hmac::{Hmac, Mac};
use sha2::Sha256;

use crate::application::service::sms_write_service::SmsWriteService;

type HmacSha256 = Hmac<Sha256>;

/// The ADR-0021 scheme identifier — greppable from runbooks/incident notes.
pub const WEBHOOK_SCHEME: &str = "hmac_raw_body";

/// Accepted clock skew between the provider's timestamp and ours.
const MAX_SKEW_SECS: i64 = 300;

#[derive(Debug, thiserror::Error)]
pub enum WebhookError {
    #[error("verification failed: {0}")]
    Verify(&'static str),
    #[error("invalid payload: {0}")]
    Invalid(String),
    #[error("db: {0}")]
    Db(#[from] sqlx::Error),
    #[error("sms: {0}")]
    Sms(#[from] crate::application::service::sms_write_service::SmsError),
}

/// What one verified callback did.
#[derive(Debug, Clone, PartialEq)]
pub enum WebhookOutcome {
    /// The sms row advanced (tracker + notification pump ran).
    Advanced,
    /// The row was already past `'process'` — a provider retry, idempotent no-op.
    Replay,
}

pub struct SmsStatusWebhookService {
    sms: SmsWriteService,
    /// Loaded from `sms.webhook_secret` (an ENV-VAR REFERENCE — ADR-0024).
    secret: Vec<u8>,
}

impl SmsStatusWebhookService {
    pub fn new(pool: sqlx::PgPool, secret: &str) -> Self {
        Self { sms: SmsWriteService::new(pool), secret: secret.as_bytes().to_vec() }
    }

    /// Handle one provider callback. `raw_body` is the EXACT bytes received
    /// (re-serialization would change the MAC input); `signature_header` is
    /// the hex digest from `X-Signature` (absent → immediate Verify error).
    pub async fn handle(
        &self,
        raw_body: &[u8],
        signature_header: Option<&str>,
    ) -> Result<WebhookOutcome, WebhookError> {
        // ---- Verification first. NOTHING below this block writes state. ----
        let header = signature_header.ok_or(WebhookError::Verify("missing X-Signature"))?;
        let given = parse_hex32(header)
            .ok_or(WebhookError::Verify("X-Signature is not 64 hex chars"))?;
        let expected = mac_raw_body(&self.secret, raw_body);
        if !const_eq(&expected, &given) {
            return Err(WebhookError::Verify("signature mismatch"));
        }

        let payload: WebhookPayload = serde_json::from_slice(raw_body)
            .map_err(|e| WebhookError::Invalid(format!("body: {e}")))?;
        let skew = (chrono::Utc::now() - payload.timestamp).num_seconds().abs();
        if skew > MAX_SKEW_SECS {
            return Err(WebhookError::Verify("timestamp outside ±5min window"));
        }
        // ---- Verified. Only now may state change. ----
        let state = payload.sms_state()?;
        let advanced = self
            .sms
            .advance_state(
                &payload.sms_uuid,
                state,
                payload.failure_type.as_deref(),
                payload.error_message.as_deref(),
            )
            .await?;
        Ok(if advanced { WebhookOutcome::Advanced } else { WebhookOutcome::Replay })
    }
}

/// The callback contract. `status` uses the sms state vocabulary the provider
/// echoes back (the same four states the drainer can apply).
#[derive(Debug, serde::Deserialize)]
struct WebhookPayload {
    timestamp: chrono::DateTime<chrono::Utc>,
    sms_uuid: String,
    status: String,
    failure_type: Option<String>,
    error_message: Option<String>,
}

impl WebhookPayload {
    fn sms_state(&self) -> Result<&str, WebhookError> {
        match self.status.as_str() {
            "process" | "pending" | "sent" | "error" => Ok(self.status.as_str()),
            other => Err(WebhookError::Invalid(format!(
                "status must be process|pending|sent|error, got {other:?}"
            ))),
        }
    }
}

/// HMAC-SHA256 over the raw body with the configured secret.
fn mac_raw_body(secret: &[u8], raw_body: &[u8]) -> [u8; 32] {
    let mut mac = HmacSha256::new_from_slice(secret).expect("HMAC accepts any key length");
    mac.update(raw_body);
    mac.finalize().into_bytes().into()
}

/// Parse a 64-char lowercase-or-uppercase hex string into 32 bytes.
fn parse_hex32(s: &str) -> Option<[u8; 32]> {
    if s.len() != 64 || !s.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    let mut out = [0u8; 32];
    for (i, pair) in s.as_bytes().chunks(2).enumerate() {
        let hi = (pair[0] as char).to_digit(16)?;
        let lo = (pair[1] as char).to_digit(16)?;
        out[i] = ((hi << 4) | lo) as u8;
    }
    Some(out)
}

/// Constant-time equality — no early exit on the first differing byte (no
/// timing oracle on how much of the MAC matched). Lengths are both 32 by
/// construction.
fn const_eq(a: &[u8; 32], b: &[u8; 32]) -> bool {
    let mut diff = 0u8;
    for (x, y) in a.iter().zip(b.iter()) {
        diff |= x ^ y;
    }
    diff == 0
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A payload serialized at NOW (the window is relative — never bake a
    /// fixed timestamp into a test).
    fn fresh_body(status: &str) -> Vec<u8> {
        format!(
            r#"{{"timestamp":"{}","sms_uuid":"abc","status":"{status}"}}"#,
            chrono::Utc::now().to_rfc3339()
        )
        .into_bytes()
    }

    fn sig(secret: &[u8], body: &[u8]) -> String {
        hex(&mac_raw_body(secret, body))
    }

    fn hex(b: &[u8]) -> String {
        b.iter().map(|x| format!("{x:02x}")).collect()
    }

    fn dead_pool() -> sqlx::PgPool {
        sqlx::PgPool::connect_lazy(
            &std::env::var("MAIL_TEST_DATABASE_URL").unwrap_or_else(|_| {
                "postgres://root:password@localhost:5432/backbone_mail_test".into()
            }),
        )
        .unwrap_or_else(|_| panic!("lazy pool handle"))
    }

    #[tokio::test]
    async fn stale_timestamp_rejected_after_signature_passes() {
        let svc = SmsStatusWebhookService::new(dead_pool(), "s3cret");
        // Signature is CORRECT; only the timestamp is ancient — proving the
        // signature check ran (and passed) before the window check.
        let raw = br#"{"timestamp":"2020-01-01T00:00:00Z","sms_uuid":"x","status":"sent"}"#;
        let err = svc.handle(&raw[..], Some(&sig(b"s3cret", &raw[..]))).await.unwrap_err();
        assert!(matches!(err, WebhookError::Verify(ref m) if m.contains("window")));
    }

    #[tokio::test]
    async fn wrong_secret_rejected() {
        let svc = SmsStatusWebhookService::new(dead_pool(), "s3cret");
        let raw = fresh_body("sent");
        let err = svc.handle(&raw, Some(&sig(b"other", &raw))).await.unwrap_err();
        assert!(matches!(err, WebhookError::Verify(ref m) if m.contains("signature")));
    }

    #[tokio::test]
    async fn missing_or_malformed_header_fail_closed() {
        let svc = SmsStatusWebhookService::new(dead_pool(), "s3cret");
        let raw = fresh_body("sent");
        assert!(matches!(
            svc.handle(&raw, None).await.unwrap_err(),
            WebhookError::Verify(_)
        ));
        assert!(matches!(
            svc.handle(&raw, Some("zz")).await.unwrap_err(),
            WebhookError::Verify(_)
        ));
    }

    #[tokio::test]
    async fn tampered_body_rejected() {
        // Sign one body, send another — the MAC input is the raw bytes.
        let svc = SmsStatusWebhookService::new(dead_pool(), "s3cret");
        let signed = fresh_body("sent");
        let sent = fresh_body("error");
        let err = svc.handle(&sent, Some(&sig(b"s3cret", &signed))).await.unwrap_err();
        assert!(matches!(err, WebhookError::Verify(ref m) if m.contains("signature")));
    }

    #[tokio::test]
    async fn bad_status_vocabulary_rejected_after_verification() {
        let svc = SmsStatusWebhookService::new(dead_pool(), "s3cret");
        let raw = fresh_body("banana");
        assert!(matches!(
            svc.handle(&raw, Some(&sig(b"s3cret", &raw))).await.unwrap_err(),
            WebhookError::Invalid(_)
        ));
    }

    #[test]
    fn hex_parsing_rejects_odd_and_non_hex() {
        assert!(parse_hex32("ab").is_none());
        assert!(parse_hex32(&"z".repeat(64)).is_none());
        assert!(parse_hex32(&"0".repeat(63)).is_none());
        assert!(parse_hex32(&"0f".repeat(32)).is_some());
    }

    #[test]
    fn const_eq_has_no_length_or_position_leak() {
        let a = [7u8; 32];
        let mut b = [7u8; 32];
        b[31] ^= 1;
        assert!(const_eq(&a, &[7u8; 32]));
        assert!(!const_eq(&a, &b));
    }
}
