//! `SmtpMailApi` — the `MailApiPort` transport over backbone-email (increment 3).
//!
//! Per send: resolve ONE `MailServer` row through the module's selection ladder
//! (`MailServerQueryService` — exact from_filter → domain → wildcard → sequence),
//! resolve the row's `smtp_pass_ref` ENV VAR NAME to the secret (the module never
//! reads env; this is the ADR-0024 seam), and hand the message to a cached
//! per-server `SmtpEmailService` (lettre under the hood). One transport per
//! `MailServer` row — built lazily on first use, reused for the row's lifetime.
//!
//! Failure vocabulary (the module's `mail_failure_type`, Odoo folds applied):
//! - no/blank envelope sender → `mail_server` (Odoo `mail_from_missing`)
//! - no ladder match → `mail_server` (there is no system-wide SMTP fallback;
//!   the port treats "no server" as a server-side verdict)
//! - `certificate` auth, unresolvable pass ref → `mail_server`
//! - transport errors → `mail_smtp`
//!
//! Idempotency: the port contract allows at-least-once per `mail_id`; the MTA
//! sees a duplicate send at worst, and `mark_sent`'s state guard makes the row
//! side a no-op — mirrors Odoo's own retry posture.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use backbone_email::smtp::{SmtpConfig, SmtpEmailService};
use backbone_email::{EmailAddress, EmailMessage, EmailPriority, EmailRecipients, EmailService, EmailStatus};
use chrono::Utc;
use uuid::Uuid;

use crate::application::service::mail_ports::{
    validate_mail_header, MailApiPort, MailSendFailure, MailSendOutcome, MailSendRequest,
    TRANSPORT_THREADING_HEADERS,
};
use crate::application::service::MailServerQueryService;
use crate::infrastructure::persistence::smtp_selection_repository::SmtpEndpoint;

/// Merge the request's per-mail headers with the structured threading into
/// the header map handed to the transport. PRECEDENCE (the contract
/// documented on [`crate::application::service::mail_ports`]):
///
/// 1. Per-mail entries are re-validated here (the single-line guard,
///    defense-in-depth for rows written outside the sanctioned enqueue) —
///    a CR/LF in a name or value is a typed refusal; the row lands
///    `exception` with the refusal as its failure reason.
/// 2. When the structured `in_reply_to` is set, a per-mail entry carrying
///    `In-Reply-To` or `References` (case-insensitive) is REFUSED — both
///    sources claiming thread linkage is ambiguous, and the send fails
///    loudly rather than silently picking a winner or emitting duplicates.
/// 3. With no collision, per-mail entries pass through verbatim and the
///    structured threading is added on top.
///
/// The transport's own envelope headers (From/To/Subject/Message-ID/MIME-*)
/// are outside this map entirely — backbone-email builds them itself, so a
/// per-mail entry with such a name can never replace them.
fn merge_transport_headers(
    in_reply_to: Option<&str>,
    per_mail: &HashMap<String, String>,
) -> Result<HashMap<String, String>, MailSendFailure> {
    let refused = |detail: String| MailSendFailure {
        // No honest bucket in the mail_failure_type vocabulary for "refused
        // before the wire" — unknown is the documented catch-all and the
        // message carries the precise reason.
        failure_type: "unknown".into(),
        message: format!("refused per-mail headers: {detail}"),
    };
    let mut merged = HashMap::with_capacity(per_mail.len() + 2);
    for (name, value) in per_mail {
        if let Err(e) = validate_mail_header(name, value) {
            return Err(refused(e.to_string()));
        }
        if in_reply_to.is_some()
            && TRANSPORT_THREADING_HEADERS
                .iter()
                .any(|reserved| name.eq_ignore_ascii_case(reserved))
        {
            return Err(refused(format!(
                "both the structured in_reply_to and a per-mail entry carry {name:?} \
                 ({:?}) — ambiguous threading, refusing",
                TRANSPORT_THREADING_HEADERS
            )));
        }
        merged.insert(name.clone(), value.clone());
    }
    if let Some(parent) = in_reply_to {
        merged.insert("In-Reply-To".into(), parent.to_string());
        merged.insert("References".into(), parent.to_string());
    }
    Ok(merged)
}

pub struct SmtpMailApi {
    servers: MailServerQueryService,
    /// One built transport per MailServer row (the lettre transport is expensive
    /// to build — TLS params + relay resolution — and safe to share).
    transports: Mutex<HashMap<Uuid, Arc<SmtpEmailService>>>,
}

impl SmtpMailApi {
    pub fn new(pool: sqlx::PgPool) -> Self {
        Self {
            servers: MailServerQueryService::new(pool),
            transports: Mutex::new(HashMap::new()),
        }
    }

    /// Build (or fetch the cached) transport for a selected endpoint.
    fn transport_for(&self, ep: &SmtpEndpoint) -> Result<Arc<SmtpEmailService>, MailSendFailure> {
        if let Some(cached) = self
            .transports
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .get(&ep.server_id)
        {
            return Ok(Arc::clone(cached));
        }

        // Password: the row carries the ENV VAR NAME; only the composing
        // process resolves it.
        let password = match (&ep.smtp_user, &ep.smtp_pass_ref) {
            (Some(_), Some(ref_name)) => Some(
                std::env::var(ref_name).map_err(|_| MailSendFailure {
                    failure_type: "mail_server".into(),
                    message: format!(
                        "smtp_pass_ref env var {ref_name:?} is not set (server {:?})",
                        ep.name
                    ),
                })?,
            ),
            // Anonymous relay — no credentials configured.
            (None, _) => None,
            (Some(_), None) => {
                return Err(MailSendFailure {
                    failure_type: "mail_server".into(),
                    message: format!(
                        "server {:?} has smtp_user but no smtp_pass_ref — cannot authenticate",
                        ep.name
                    ),
                })
            }
        };

        let config = SmtpConfig {
            host: ep.smtp_host.clone(),
            port: ep.smtp_port as u16,
            username: ep.smtp_user.clone(),
            password,
            use_tls: ep.smtp_encryption == "starttls",
            use_ssl: ep.smtp_encryption == "ssl",
            timeout: 30,
            hello_name: None,
        };
        let service = Arc::new(SmtpEmailService::new(config).map_err(|e| MailSendFailure {
            failure_type: "mail_server".into(),
            message: format!("transport build failed for server {:?}: {e}", ep.name),
        })?);
        self.transports
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .insert(ep.server_id, Arc::clone(&service));
        Ok(service)
    }
}

#[async_trait::async_trait]
impl MailApiPort for SmtpMailApi {
    async fn send(&self, req: &MailSendRequest) -> Result<MailSendOutcome, MailSendFailure> {
        // The ladder cannot even be walked without a domain — Odoo's
        // mail_from_missing folds onto mail_server.
        if req.from.trim().is_empty() {
            return Err(MailSendFailure {
                failure_type: "mail_server".into(),
                message: "mail_from_missing: the queue row carries no envelope sender".into(),
            });
        }

        let ep = self
            .servers
            .resolve_endpoint(&req.from)
            .await
            .map_err(|e| MailSendFailure {
                failure_type: "mail_server".into(),
                message: format!("server selection failed: {e}"),
            })?
            .ok_or_else(|| MailSendFailure {
                failure_type: "mail_server".into(),
                message: format!(
                    "no active mail server matches {:?} (the from_filter ladder is empty at every rung)",
                    req.from
                ),
            })?;

        if ep.smtp_authentication == "certificate" {
            return Err(MailSendFailure {
                failure_type: "mail_server".into(),
                message: format!(
                    "server {:?} uses smtp_authentication 'certificate' — declared but unsupported \
                     (needs a file-based secret store; port-notes §8)",
                    ep.name
                ),
            });
        }

        let transport = self.transport_for(&ep)?;

        let headers = merge_transport_headers(req.in_reply_to.as_deref(), &req.headers)?;
        let message = EmailMessage {
            id: req.mail_id.to_string(),
            from: EmailAddress::new(&req.from),
            reply_to: None,
            recipients: EmailRecipients::new(
                req.to.iter().map(EmailAddress::new).collect(),
            ),
            subject: req.subject.clone().unwrap_or_default(),
            text: None,
            html: Some(req.body_html.clone()),
            attachments: Vec::new(),
            headers,
            template_data: None,
            created_at: Utc::now(),
            scheduled_at: None,
            priority: EmailPriority::default(),
            tracking: false,
        };

        let report = transport.send(message).await.map_err(|e| MailSendFailure {
            failure_type: "mail_smtp".into(),
            message: format!("smtp send failed via {:?}: {e}", ep.name),
        })?;
        if report.status == EmailStatus::Sent {
            Ok(MailSendOutcome::Accepted)
        } else {
            Err(MailSendFailure {
                failure_type: "mail_smtp".into(),
                message: format!(
                    "smtp verdict {:?} via {:?}: {}",
                    report.status,
                    ep.name,
                    report.error.unwrap_or_else(|| "no provider detail".into())
                ),
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn per_mail(entries: &[(&str, &str)]) -> HashMap<String, String> {
        entries.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect()
    }

    #[test]
    fn per_mail_headers_flow_into_the_transport_map() {
        let merged =
            merge_transport_headers(None, &per_mail(&[("X-Campaign-Id", "summer-2026")]))
                .expect("clean per-mail headers merge");
        assert_eq!(merged.get("X-Campaign-Id").map(String::as_str), Some("summer-2026"));
        assert_eq!(merged.len(), 1);
    }

    #[test]
    fn structured_threading_is_added_on_top_of_per_mail_headers() {
        let merged = merge_transport_headers(
            Some("<parent@example.com>"),
            &per_mail(&[("X-Campaign-Id", "promo")]),
        )
        .expect("no collision merges");
        assert_eq!(merged.get("In-Reply-To").map(String::as_str), Some("<parent@example.com>"));
        assert_eq!(merged.get("References").map(String::as_str), Some("<parent@example.com>"));
        assert_eq!(merged.get("X-Campaign-Id").map(String::as_str), Some("promo"));
    }

    #[test]
    fn per_mail_threading_headers_pass_when_no_structured_threading() {
        let merged = merge_transport_headers(
            None,
            &per_mail(&[("In-Reply-To", "<self@example.com>"), ("references", "<t@example.com>")]),
        )
        .expect("sole source of threading flows through");
        assert_eq!(merged.get("In-Reply-To").map(String::as_str), Some("<self@example.com>"));
        // RFC 5322 names are case-insensitive — a lowercase entry is kept
        // verbatim (no silent case rewrite).
        assert_eq!(merged.get("references").map(String::as_str), Some("<t@example.com>"));
    }

    #[test]
    fn threading_collision_is_refused_loudly_case_insensitively() {
        for name in ["In-Reply-To", "References", "in-reply-to", "REFERENCES"] {
            let refusal = merge_transport_headers(
                Some("<parent@example.com>"),
                &per_mail(&[(name, "<rogue@example.com>")]),
            )
            .expect_err("collision must refuse");
            assert_eq!(refusal.failure_type, "unknown");
            assert!(
                refusal.message.contains("ambiguous threading"),
                "message must name the collision: {}",
                refusal.message
            );
        }
    }

    #[test]
    fn crlf_smuggle_through_the_gateway_is_refused() {
        let refusal = merge_transport_headers(
            None,
            &per_mail(&[("X-Campaign", "a\r\nBcc: victim@example.com")]),
        )
        .expect_err("CRLF must refuse at the gateway too (defense in depth)");
        assert_eq!(refusal.failure_type, "unknown");
        assert!(refusal.message.contains("CR/LF"));
        // The name side too.
        assert!(merge_transport_headers(None, &per_mail(&[("Bad\r\nName", "v")])).is_err());
    }
}
