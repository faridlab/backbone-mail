//! The message edit + star service (hand-written; user-owned).
//!
//! Port of `mail.message.update_content` (author-or-admin gate, body/subject
//! only — edits never rewrite history silently: the bus event carries the
//! fact) and `toggle_star` (the starred m2m materialized as
//! `mail_message_stars`, MAIL-M45 adjunct).

use uuid::Uuid;

use crate::application::service::chatter_acl::MessagingIdentity;
use crate::domain::event::constants::stage_bus_event;
use crate::infrastructure::persistence::message_edit_repository::MessageEditRepository;

#[derive(Debug, thiserror::Error)]
pub enum MessageEditError {
    #[error("db: {0}")]
    Db(#[from] sqlx::Error),
    #[error("invalid input: {0}")]
    Invalid(String),
    #[error("not found: message {0}")]
    NotFound(Uuid),
    #[error("forbidden: only the author (or an admin route) may edit {0}")]
    Forbidden(Uuid),
    #[error("starring needs a partner identity")]
    NeedsPartner,
}

pub struct MessageEditService {
    pool: sqlx::PgPool,
}

impl MessageEditService {
    pub fn new(pool: sqlx::PgPool) -> Self {
        Self { pool }
    }

    /// `update_content`: edit subject/body. The AUTHOR gate is in-DB (the
    /// write is conditional on authorship) unless `is_admin` (a route that
    /// already proved admin). Returns false when nothing changed.
    pub async fn update_content(
        &self,
        message_id: Uuid,
        identity: &MessagingIdentity,
        is_admin: bool,
        subject: Option<&str>,
        body: Option<&str>,
    ) -> Result<bool, MessageEditError> {
        if subject.is_none() && body.is_none() {
            return Err(MessageEditError::Invalid("nothing to edit".into()));
        }
        let mut tx = self.pool.begin().await?;
        let (author_id, author_guest_id) =
            MessageEditRepository::message_authors(&mut tx, message_id)
                .await?
                .ok_or(MessageEditError::NotFound(message_id))?;

        if !is_admin {
            let is_author = match identity {
                MessagingIdentity::User { partner_id } => author_id == Some(*partner_id),
                MessagingIdentity::Guest { guest_id } => author_guest_id == Some(*guest_id),
            };
            if !is_author {
                return Err(MessageEditError::Forbidden(message_id));
            }
        }

        let changed =
            MessageEditRepository::update_content(&mut tx, message_id, subject, body).await?;

        let channel_key = MessageEditRepository::channel_key(&mut tx, message_id).await?;
        stage_bus_event(
            &mut tx,
            "MessageEdited",
            "MailMessage",
            message_id,
            channel_key.unwrap_or_else(|| format!("mail.message_{message_id}")),
            "mail.message/update",
            serde_json::json!({
                "message_id": message_id,
                "subject_changed": subject.is_some(),
                "body_changed": body.is_some(),
            }),
        )
        .await?;
        tx.commit().await?;
        Ok(changed)
    }

    /// `toggle_star`: materialize/remove the (partner, message) star row.
    /// Returns the new state (true = starred). Guests cannot star (the m2m
    /// is partner-keyed in Odoo; guest starring doesn't exist).
    pub async fn toggle_star(
        &self,
        message_id: Uuid,
        identity: &MessagingIdentity,
    ) -> Result<bool, MessageEditError> {
        let Some(partner_id) = identity.partner_id() else {
            return Err(MessageEditError::NeedsPartner);
        };
        let mut tx = self.pool.begin().await?;
        let deleted =
            MessageEditRepository::delete_star(&mut tx, partner_id, message_id).await?;
        let starred = if deleted {
            false
        } else {
            MessageEditRepository::insert_star(&mut tx, Uuid::new_v4(), partner_id, message_id)
                .await?
        };
        stage_bus_event(
            &mut tx,
            "MessageStarToggled",
            "MailMessageStar",
            message_id,
            crate::domain::event::constants::partner_channel(partner_id),
            "mail.message/toggle_star",
            serde_json::json!({
                "message_id": message_id,
                "partner_id": partner_id,
                "starred": starred,
            }),
        )
        .await?;
        tx.commit().await?;
        Ok(starred)
    }
}
