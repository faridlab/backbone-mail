//! The presence write service (hand-written; user-owned).
//!
//! Port of `mail.presence` update paths (MAIL-M11): `update_bus_presence`
//! (the poll-driven liveness write — Odoo ties it to an active websocket
//! session; the port ties it to a valid SSE session proof, so a dead client's
//! cron can't keep them "online") and `set_manual_im_status` (the user's
//! explicit away/online override, broadcast on their channel).

use uuid::Uuid;

use crate::application::service::chatter_acl::MessagingIdentity;
use crate::domain::event::constants::stage_bus_event;
use crate::infrastructure::persistence::presence_repository::PresenceRepository;

#[derive(Debug, thiserror::Error)]
pub enum PresenceError {
    #[error("db: {0}")]
    Db(#[from] sqlx::Error),
    #[error("invalid input: {0}")]
    Invalid(String),
    /// The Odoo `is_websocket_session` gate: without a live SSE session proof
    /// the liveness write is refused — presence must reflect REAL connections.
    #[error("presence update requires a live SSE session proof")]
    ProofRequired,
}

pub struct PresenceWriteService {
    pool: sqlx::PgPool,
}

impl PresenceWriteService {
    pub fn new(pool: sqlx::PgPool) -> Self {
        Self { pool }
    }

    /// `update_bus_presence`: refresh `last_poll` for the identity's presence
    /// row. PROOF-GATED — the route layer must verify the X-Messaging-Session
    /// proof before calling with `session_proof_valid = true`; the service
    /// refuses otherwise (the is_websocket_session equivalent).
    pub async fn update_bus_presence(
        &self,
        identity: &MessagingIdentity,
        session_proof_valid: bool,
    ) -> Result<(), PresenceError> {
        if !session_proof_valid {
            return Err(PresenceError::ProofRequired);
        }
        let (user_id, guest_id) = identity_columns(identity);
        let mut tx = self.pool.begin().await?;
        PresenceRepository::refresh_last_poll(&mut tx, user_id, guest_id).await?;
        tx.commit().await?;
        Ok(())
    }

    /// `set_manual_im_status`: the explicit status override (online/away/
    /// offline), broadcast as `bus.bus/im_status_updated` on the identity's
    /// own channel (Odoo converges user presence on the partner channel).
    pub async fn set_manual_im_status(
        &self,
        identity: &MessagingIdentity,
        status: &str,
    ) -> Result<(), PresenceError> {
        if !matches!(status, "online" | "away" | "offline") {
            return Err(PresenceError::Invalid(format!(
                "im_status must be online|away|offline, got {status:?}"
            )));
        }
        let (user_id, guest_id) = identity_columns(identity);
        let mut tx = self.pool.begin().await?;
        PresenceRepository::set_status(&mut tx, user_id, guest_id, status).await?;
        stage_bus_event(
            &mut tx,
            "ImStatusUpdated",
            "MailPresence",
            identity.partner_id().unwrap_or_else(Uuid::nil),
            identity.channel(),
            "bus.bus/im_status_updated",
            serde_json::json!({
                "partner_id": identity.partner_id(),
                "guest_id": match identity {
                    MessagingIdentity::Guest { guest_id } => Some(*guest_id),
                    _ => None,
                },
                "im_status": status,
            }),
        )
        .await?;
        tx.commit().await?;
        Ok(())
    }
}

/// Map an identity to (user-ish presence key, guest key). Odoo's presence is
/// keyed on res.users; the port keeps the partner's id in `user_id` (the
/// partner IS the messaging identity — see `partner_channel` convergence).
fn identity_columns(identity: &MessagingIdentity) -> (Option<Uuid>, Option<Uuid>) {
    match identity {
        MessagingIdentity::User { partner_id } => (Some(*partner_id), None),
        MessagingIdentity::Guest { guest_id } => (None, Some(*guest_id)),
    }
}
