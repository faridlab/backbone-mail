//! The discuss-channel write service (hand-written; user-owned).
//!
//! Port of `discuss.channel` create/`_get_or_create_chat`/guarded writes
//! (MAIL-M37). Every verb is one transaction with its bus event staged in-tx;
//! the creator-membership injection ("a channel is born with its creator as
//! the first member") is the same transaction as the channel INSERT, never a
//! second write. `channel_type` is immutable post-create — there is no UPDATE
//! path for it anywhere in this service (enforced at the write layer, per
//! port-notes M37-IMMUTABLE).

use uuid::Uuid;

use crate::application::service::chatter_acl::MessagingIdentity;
use crate::domain::event::constants::{discuss_channel, stage_bus_event};
use crate::infrastructure::persistence::channel_repository::{
    ChannelRepository, NewChannelRow,
};

#[derive(Debug, thiserror::Error)]
pub enum ChannelError {
    #[error("db: {0}")]
    Db(#[from] sqlx::Error),
    #[error("invalid input: {0}")]
    Invalid(String),
    #[error("not found: channel {0}")]
    NotFound(Uuid),
}

/// The create command (MAIL-M37).
#[derive(Debug, Clone, Default)]
pub struct CreateChannelCommand {
    pub name: Option<String>,
    /// chat | channel | group. Immutable after create.
    pub channel_type: String,
    /// Public invitation target (uuid string; minted by the caller).
    pub uuid: Option<String>,
    /// public / private / group (channel-type only).
    pub default_access_mode: Option<String>,
    pub email_send: bool,
    /// The creator — injected as the first member in the create transaction.
    pub creator: Option<MessagingIdentity>,
}

pub struct ChannelWriteService {
    pool: sqlx::PgPool,
}

impl ChannelWriteService {
    pub fn new(pool: sqlx::PgPool) -> Self {
        Self { pool }
    }

    /// Create a channel; the creator (when present) becomes the first member
    /// atomically (MAIL-M37: "create injects the creator").
    pub async fn create(&self, cmd: CreateChannelCommand) -> Result<Uuid, ChannelError> {
        let channel_type = match cmd.channel_type.as_str() {
            "" | "channel" => "channel",
            "chat" => "chat",
            "group" => "group",
            other => {
                return Err(ChannelError::Invalid(format!(
                    "unknown channel_type {other:?}"
                )))
            }
        };
        if channel_type == "chat" {
            return Err(ChannelError::Invalid(
                "chats are minted via get_or_create_chat, not create (1:1 dedup)".into(),
            ));
        }

        let mut tx = self.pool.begin().await?;
        let channel_id = Uuid::new_v4();
        ChannelRepository::insert_channel(
            &mut tx,
            &NewChannelRow {
                id: channel_id,
                name: cmd.name.as_deref(),
                channel_type,
                uuid: cmd.uuid.as_deref(),
                default_access_mode: cmd.default_access_mode.as_deref(),
                email_send: cmd.email_send,
            },
        )
        .await?;
        if let Some(creator) = &cmd.creator {
            ChannelRepository::insert_creator_member(&mut tx, channel_id, creator).await?;
        }
        stage_bus_event(
            &mut tx,
            "DiscussChannelCreated",
            "DiscussChannel",
            channel_id,
            discuss_channel(channel_id),
            "discuss.channel/insert",
            serde_json::json!({
                "channel_id": channel_id,
                "channel_type": channel_type,
                "name": cmd.name,
            }),
        )
        .await?;
        tx.commit().await?;
        Ok(channel_id)
    }

    /// `_get_or_create_chat` (source-verified :1340-1360): find the chat whose
    /// live partner-member set is EXACTLY {a, b}; else mint it with both
    /// members. Returns (channel_id, created).
    pub async fn get_or_create_chat(
        &self,
        partner_a: Uuid,
        partner_b: Uuid,
    ) -> Result<(Uuid, bool), ChannelError> {
        if partner_a == partner_b {
            return Err(ChannelError::Invalid(
                "a 1:1 chat needs two distinct partners (Odoo allows self-chat via a different path; not ported)".into(),
            ));
        }
        let set = [partner_a, partner_b];

        // Fast path: existing chat (own short tx — read-only).
        let mut tx = self.pool.begin().await?;
        if let Some(id) = ChannelRepository::find_chat_by_member_set(&mut tx, &set).await? {
            tx.commit().await?;
            return Ok((id, false));
        }
        tx.commit().await?;

        // Mint. A concurrent mint can race past the find; the member uniques
        // (G-MAIL-8) make the loser fail on insert — retry the find once,
        // then surface the error if it still misses (extremely unlikely).
        let mut tx = self.pool.begin().await?;
        if let Some(id) = ChannelRepository::find_chat_by_member_set(&mut tx, &set).await? {
            tx.commit().await?;
            return Ok((id, false));
        }
        let channel_id = Uuid::new_v4();
        ChannelRepository::insert_channel(
            &mut tx,
            &NewChannelRow {
                id: channel_id,
                name: None, // 1:1 names are derived per-viewer, not stored
                channel_type: "chat",
                uuid: None,
                default_access_mode: None,
                email_send: false,
            },
        )
        .await?;
        ChannelRepository::insert_creator_member(
            &mut tx,
            channel_id,
            &MessagingIdentity::User { partner_id: partner_a },
        )
        .await?;
        ChannelRepository::insert_creator_member(
            &mut tx,
            channel_id,
            &MessagingIdentity::User { partner_id: partner_b },
        )
        .await?;
        stage_bus_event(
            &mut tx,
            "DiscussChannelCreated",
            "DiscussChannel",
            channel_id,
            discuss_channel(channel_id),
            "discuss.channel/insert",
            serde_json::json!({
                "channel_id": channel_id,
                "channel_type": "chat",
                "members": set,
            }),
        )
        .await?;
        tx.commit().await?;
        Ok((channel_id, true))
    }

    /// Guarded field updates (name/description/email_send). There is NO verb
    /// that writes channel_type — immutability is the absence of the path.
    pub async fn update_fields(
        &self,
        channel_id: Uuid,
        name: Option<&str>,
        description: Option<&str>,
        email_send: Option<bool>,
    ) -> Result<(), ChannelError> {
        if name.is_none() && description.is_none() && email_send.is_none() {
            return Err(ChannelError::Invalid("nothing to update".into()));
        }
        let mut tx = self.pool.begin().await?;
        Self::must_exist(&mut tx, channel_id).await?;
        ChannelRepository::update_channel_fields(&mut tx, channel_id, name, description, email_send)
            .await?;
        stage_bus_event(
            &mut tx,
            "DiscussChannelUpdated",
            "DiscussChannel",
            channel_id,
            discuss_channel(channel_id),
            "discuss.channel/update",
            serde_json::json!({
                "channel_id": channel_id,
                "name": name,
                "description": description,
                "email_send": email_send,
            }),
        )
        .await?;
        tx.commit().await?;
        Ok(())
    }

    /// Pin/unpin a message in a channel (MAIL-M37.1). Membership of the
    /// caller is checked by the route layer; the service re-checks the
    /// channel exists (a pin on a dead channel is a 404, not a silent no-op).
    pub async fn set_message_pinned(
        &self,
        channel_id: Uuid,
        message_id: Uuid,
        pinned: bool,
    ) -> Result<(), ChannelError> {
        let mut tx = self.pool.begin().await?;
        Self::must_exist(&mut tx, channel_id).await?;
        // The message must belong to this channel (model/res_id edge).
        let owner: Option<(Option<String>, Option<Uuid>)> = sqlx::query_as(
            r#"SELECT model, res_id FROM messaging.mail_messages WHERE id = $1"#,
        )
        .bind(message_id)
        .fetch_optional(&mut *tx)
        .await?;
        match owner {
            Some((model, res_id))
                if model.as_deref() == Some("discuss.channel") && res_id == Some(channel_id) => {}
            _ => {
                return Err(ChannelError::Invalid(format!(
                    "message {message_id} does not belong to channel {channel_id}"
                )))
            }
        }
        ChannelRepository::set_message_pinned(&mut tx, message_id, pinned).await?;
        stage_bus_event(
            &mut tx,
            "MessagePinnedChanged",
            "MailMessage",
            message_id,
            discuss_channel(channel_id),
            if pinned { "mail.message/pin" } else { "mail.message/unpin" },
            serde_json::json!({
                "channel_id": channel_id,
                "message_id": message_id,
                "pinned": pinned,
            }),
        )
        .await?;
        tx.commit().await?;
        Ok(())
    }

    /// Soft-delete (archive) a channel. Route layer gates this to
    /// moderators/admins; the service only proves the channel exists.
    pub async fn archive(&self, channel_id: Uuid) -> Result<(), ChannelError> {
        let mut tx = self.pool.begin().await?;
        Self::must_exist(&mut tx, channel_id).await?;
        ChannelRepository::soft_delete_channel(&mut tx, channel_id).await?;
        stage_bus_event(
            &mut tx,
            "DiscussChannelDeleted",
            "DiscussChannel",
            channel_id,
            discuss_channel(channel_id),
            "discuss.channel/delete",
            serde_json::json!({ "channel_id": channel_id }),
        )
        .await?;
        tx.commit().await?;
        Ok(())
    }

    async fn must_exist(tx: &mut sqlx::PgConnection, channel_id: Uuid) -> Result<(), ChannelError> {
        ChannelRepository::channel_type(tx, channel_id)
            .await?
            .map(|_| ())
            .ok_or(ChannelError::NotFound(channel_id))
    }
}
