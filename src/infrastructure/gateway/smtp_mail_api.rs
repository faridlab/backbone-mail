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
    MailApiPort, MailSendFailure, MailSendOutcome, MailSendRequest,
};
use crate::application::service::MailServerQueryService;
use crate::infrastructure::persistence::smtp_selection_repository::SmtpEndpoint;

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

        let mut headers = HashMap::new();
        if let Some(parent) = &req.in_reply_to {
            headers.insert("In-Reply-To".into(), parent.clone());
            headers.insert("References".into(), parent.clone());
        }
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
