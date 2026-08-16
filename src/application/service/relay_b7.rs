//! MAIL-B7 — the relay-side `UserEmailChanged` consumer (increment 3).
//!
//! The outbox relay's publish seam hands every durable event here. Exactly one
//! event type has a transport side today: `UserEmailChanged`, staged by the
//! sapiens User host when a user's email changes. The handler posts the
//! security warning to the **PREVIOUS** address through the mail queue
//! (`message_post` email channel) — the entire point is that the actor who
//! changed the address cannot suppress a warning to the address they took
//! over. Load-bearing: never send this to the new address (port-notes MAIL-B7).
//!
//! Everything else stays a logged pass-through (the relay is the carrier of
//! record; wiring other event types to transports is the 3b notification
//! composition seam).
//!
//! Failure semantics: a B7 staging error returns Err → the relay leaves THIS
//! row for the next pass (at-least-once) without stopping. `message_post` is
//! one transaction, so an Err means nothing was staged — the retry cannot
//! double-send.
//!
//! Promoted from backbone-messaging-app so every composing service gets the
//! same previous-address security consumer for free.

use backbone_outbox::OutboxRecord;

use crate::application::service::message_write_service::{
    MessagePostCommand, MessageWriteService, NotificationChannel, PostRecipient,
};
use crate::domain::event::constants::USER_EMAIL_CHANGED_EVENT;

/// Handle one relayed event. `Ok(true)` = a transport side effect was staged;
/// `Ok(false)` = pass-through (logged); `Err` = retry this row next pass.
pub async fn handle_relayed_event(
    mails: &MessageWriteService,
    rec: &OutboxRecord,
) -> Result<bool, backbone_outbox::error::OutboxError> {
    if rec.event_type != USER_EMAIL_CHANGED_EVENT {
        tracing::debug!(
            target: "outbox::relay",
            event = %rec.event_type,
            aggregate = %rec.aggregate_id,
            "outbox event relayed (no transport consumer)"
        );
        return Ok(false);
    }

    // The payload may be bus-enveloped ({channel, message:{payload}}) or flat,
    // depending on which host staged it — accept both.
    let inner = rec
        .payload
        .get("message")
        .and_then(|m| m.get("payload"))
        .unwrap_or(&rec.payload);
    let (user_id, previous_email, new_email, changed_at) = (
        inner.get("user_id").and_then(|v| v.as_str()),
        inner.get("previous_email").and_then(|v| v.as_str()),
        inner.get("new_email").and_then(|v| v.as_str()),
        inner.get("changed_at").and_then(|v| v.as_str()),
    );
    let (Some(user_id), Some(previous_email), Some(new_email)) = (user_id, previous_email, new_email)
    else {
        return Err(backbone_outbox::error::OutboxError::Publish(format!(
            "UserEmailChanged payload is malformed (needs user_id, previous_email, new_email): {inner}"
        )));
    };

    let body = security_body(previous_email, new_email);
    let posted = mails
        .message_post(MessagePostCommand {
            body,
            subject: Some("Your email address was changed".into()),
            message_type: "email".into(),
            email_from: None, // the SMTP ladder picks the envelope server
            record_name: Some("user".into()),
            recipients: vec![PostRecipient {
                res_partner_id: None, // the previous address need not be a partner
                channel: NotificationChannel::Email,
                email: Some(previous_email.to_string()),
                number: None,
            }],
            ..Default::default()
        })
        .await
        .map_err(|e| {
            // Loud: this is a security notice. Include the user + time so the
            // operator can reconstruct the change even from the log.
            tracing::error!(
                target: "mail::b7",
                user_id, previous_email, new_email,
                changed_at = changed_at.unwrap_or("?"),
                error = %e,
                "MAIL-B7: staging the previous-address warning FAILED — the relay will retry"
            );
            backbone_outbox::error::OutboxError::Publish(format!("MAIL-B7 staging failed: {e}"))
        })?;

    tracing::info!(
        target: "mail::b7",
        user_id, previous_email,
        mail_id = ?posted.mail_id,
        "MAIL-B7: security warning staged to the PREVIOUS address"
    );
    Ok(true)
}

/// The fixed warning template. The two addresses are reflected into HTML, so
/// they are escaped — the event payload is externally influenced input.
fn security_body(previous_email: &str, new_email: &str) -> String {
    format!(
        "<p>Your account's email address was changed.</p>\
         <p>Previous address: <strong>{}</strong></p>\
         <p>New address: <strong>{}</strong></p>\
         <p>If you did not request this change, contact your administrator \
         immediately — use a channel you already trust.</p>",
        html_escape(previous_email),
        html_escape(new_email),
    )
}

/// Minimal HTML escaping for text reflected into the warning body.
fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#x27;")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn warning_body_escapes_the_addresses() {
        let body = security_body("a@x.example", "b\"<script>@y.example");
        assert!(!body.contains("<script>"));
        assert!(body.contains("&quot;&lt;script&gt;@y.example"));
        assert!(body.contains("a@x.example"));
    }
}
