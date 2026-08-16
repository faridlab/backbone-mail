//! `HttpSmsApi` — the generic HTTP JSON SMS provider (increment 3).
//!
//! Mirrors Odoo's IAP batch request shape (`{content, numbers: [{uuid, number}]}`
//! with a Bearer token) against any configurable endpoint, and maps the IAP
//! response vocabulary onto the module's `SmsSendOutcome` / `sms_failure_type`
//! set. Provider swap is config, not code: the composing service selects
//! `jobs.sms_provider: http` + the two env-ref secrets in its gateway config.
//!
//! State map (Odoo `IAP_TO_SMS_STATE_*`):
//!   processing            → `Processing` (sms.state process)
//!   success | sent        → `Accepted`  (sms.state pending, LABEL 'Sent')
//!   delivered             → `Delivered` (sms.state sent,  LABEL 'Delivered')
//!
//! Error map (SM §2.3 PROVIDER_TO_SMS_FAILURE_TYPE):
//!   insufficient_credit     → sms_credit
//!   wrong_number_format     → sms_number_format
//!   country_not_supported   → sms_country_not_supported
//!   server_error            → sms_server
//!   unregistered            → sms_acc
//!   anything else           → sms_server (the honest generic bucket)
//!
//! One request per queued row (the uuid IS the correlation key — the webhook
//! and tracker key on it), so the batch envelope carries exactly one number.

use crate::application::service::sms_ports::{
    SmsApiPort, SmsSendFailure, SmsSendOutcome, SmsSendRequest,
};

#[derive(serde::Deserialize)]
struct ProviderResponse {
    /// IAP state: processing | success | sent | delivered.
    #[serde(default)]
    state: Option<String>,
    /// IAP error code on rejection (see module doc for the map).
    #[serde(default, alias = "error_code")]
    code: Option<String>,
    #[serde(default)]
    message: Option<String>,
}

pub struct HttpSmsApi {
    endpoint: String,
    token: String,
    client: reqwest::Client,
}

impl HttpSmsApi {
    pub fn new(endpoint: String, token: String) -> Self {
        Self {
            endpoint,
            token,
            client: reqwest::Client::new(),
        }
    }
}

/// The IAP state string → the port's outcome vocabulary.
fn map_state(state: &str) -> Option<SmsSendOutcome> {
    match state {
        "processing" => Some(SmsSendOutcome::Processing),
        "success" | "sent" => Some(SmsSendOutcome::Accepted),
        "delivered" => Some(SmsSendOutcome::Delivered),
        _ => None,
    }
}

/// The IAP error code → the module's `sms_failure_type` vocabulary.
pub fn map_error_code(code: &str) -> &'static str {
    match code {
        "insufficient_credit" => "sms_credit",
        "wrong_number_format" => "sms_number_format",
        "country_not_supported" => "sms_country_not_supported",
        "unregistered" => "sms_acc",
        _ => "sms_server",
    }
}

#[async_trait::async_trait]
impl SmsApiPort for HttpSmsApi {
    async fn send(&self, req: &SmsSendRequest) -> Result<SmsSendOutcome, SmsSendFailure> {
        let body = serde_json::json!({
            "content": req.body,
            "numbers": [{ "uuid": req.uuid, "number": req.number }],
        });

        let resp = self
            .client
            .post(&self.endpoint)
            .bearer_auth(&self.token)
            .json(&body)
            .send()
            .await
            .map_err(|e| SmsSendFailure {
                failure_type: "sms_server".into(),
                message: format!("provider request failed: {e}"),
                iap_status_code: None,
            })?;

        let status = resp.status();
        let payload: ProviderResponse = resp.json().await.map_err(|e| SmsSendFailure {
            failure_type: "sms_server".into(),
            message: format!("provider returned a non-JSON body (HTTP {status}): {e}"),
            iap_status_code: None,
        })?;

        if let Some(state) = payload.state.as_deref().and_then(map_state) {
            return Ok(state);
        }
        // No mappable state — an error body (or an unmapped state, which we
        // refuse to guess at: sms_server with the raw detail preserved).
        Err(SmsSendFailure {
            failure_type: map_error_code(payload.code.as_deref().unwrap_or("")).into(),
            message: payload.message.unwrap_or_else(|| {
                format!("provider rejected the send (HTTP {status}, no detail)")
            }),
            iap_status_code: None,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn iap_states_map_onto_port_outcomes() {
        assert_eq!(map_state("processing"), Some(SmsSendOutcome::Processing));
        assert_eq!(map_state("success"), Some(SmsSendOutcome::Accepted));
        assert_eq!(map_state("sent"), Some(SmsSendOutcome::Accepted));
        assert_eq!(map_state("delivered"), Some(SmsSendOutcome::Delivered));
        assert_eq!(map_state("mystery"), None);
    }

    #[test]
    fn iap_error_codes_map_onto_failure_vocabulary() {
        assert_eq!(map_error_code("insufficient_credit"), "sms_credit");
        assert_eq!(map_error_code("wrong_number_format"), "sms_number_format");
        assert_eq!(map_error_code("country_not_supported"), "sms_country_not_supported");
        assert_eq!(map_error_code("unregistered"), "sms_acc");
        assert_eq!(map_error_code("server_error"), "sms_server");
        assert_eq!(map_error_code("never-seen-before"), "sms_server");
    }
}
