//! The message-reaction write service (hand-written; user-owned).
//!
//! Port of `mail.message.reaction` toggle (MAIL-M5): one row per
//! (message, partner, content) — toggling removes it. Emits the
//! `mail.message/update` reaction payload on the thread's channel so live
//! clients re-render counts without a refetch.

use uuid::Uuid;

use crate::application::service::chatter_acl::MessagingIdentity;
use crate::domain::event::constants::stage_bus_event;
use crate::infrastructure::persistence::reaction_repository::ReactionRepository;

#[derive(Debug, thiserror::Error)]
pub enum ReactionError {
    #[error("db: {0}")]
    Db(#[from] sqlx::Error),
    #[error("invalid input: {0}")]
    Invalid(String),
    #[error("reactions need an identity (partner or guest)")]
    NeedsIdentity,
    #[error("not found: message {0}")]
    NotFound(Uuid),
}

pub struct ReactionWriteService {
    pool: sqlx::PgPool,
}

impl ReactionWriteService {
    pub fn new(pool: sqlx::PgPool) -> Self {
        Self { pool }
    }

    /// Toggle a reaction (emoji content string) on a message. Guests react
    /// as guests (source-verified: the reaction uniques are per partner AND
    /// per guest, G-MAIL-9 shape). Returns the new state (true = reacted).
    pub async fn toggle(
        &self,
        message_id: Uuid,
        identity: &MessagingIdentity,
        content: &str,
    ) -> Result<bool, ReactionError> {
        let (partner_id, guest_id) = match identity {
            MessagingIdentity::User { partner_id } => (Some(*partner_id), None),
            MessagingIdentity::Guest { guest_id } => (None, Some(*guest_id)),
        };
        if partner_id.is_none() && guest_id.is_none() {
            return Err(ReactionError::NeedsIdentity);
        }
        if content.trim().is_empty() || content.len() > 64 {
            return Err(ReactionError::Invalid("content must be 1..=64 chars".into()));
        }

        let mut tx = self.pool.begin().await?;
        // The message must exist (and be live).
        let (model, res_id) = ReactionRepository::thread_of_message(&mut tx, message_id)
            .await?
            .ok_or(ReactionError::NotFound(message_id))?;

        // Toggle: delete-if-present else insert (one live reaction per
        // (message, identity, content) — the partial uniques).
        let deleted = ReactionRepository::delete_reaction(
            &mut tx, message_id, partner_id, guest_id, content,
        )
        .await?;
        let reacted = if deleted {
            false
        } else {
            ReactionRepository::insert_reaction(
                &mut tx, Uuid::new_v4(), message_id, partner_id, guest_id, content,
            )
            .await?
        };

        let channel_key = match (&model, res_id) {
            (Some(m), Some(r)) if m == "discuss.channel" => {
                crate::domain::event::constants::discuss_channel(r)
            }
            _ => format!("mail.message_{message_id}"),
        };
        stage_bus_event(
            &mut tx,
            "ReactionToggled",
            "MailMessageReaction",
            message_id,
            channel_key,
            "mail.message/update",
            serde_json::json!({
                "message_id": message_id,
                "partner_id": partner_id,
                "guest_id": guest_id,
                "content": content,
                "reacted": reacted,
            }),
        )
        .await?;
        tx.commit().await?;
        Ok(reacted)
    }
}
