//! The discuss-channel-member write service (hand-written; user-owned).
//!
//! Port of the member-side discuss behaviors (MAIL-M38): mark-as-read /
//! channel-fetched (the unread race, proven by the SKIP LOCKED advance), the
//! new-message separator, join/leave, mute (until-unmuted = '+infinity'),
//! per-channel notification overrides, sidebar pin and fold state. Every verb
//! opens its own transaction and stages its bus event in-tx; events are
//! addressed to the channel stream (all members see read-state changes) and
//! the member's own channel (their client reconciles).

use uuid::Uuid;

use crate::application::service::chatter_acl::MessagingIdentity;
use crate::domain::event::constants::{discuss_channel, stage_bus_event};
use crate::infrastructure::persistence::channel_member_repository::{
    ChannelMemberRepository, MemberKey,
};

#[derive(Debug, thiserror::Error)]
pub enum MemberError {
    #[error("db: {0}")]
    Db(#[from] sqlx::Error),
    #[error("invalid input: {0}")]
    Invalid(String),
    #[error("not a member of channel {0}")]
    NotMember(Uuid),
}

pub struct ChannelMemberWriteService {
    pool: sqlx::PgPool,
}

impl ChannelMemberWriteService {
    pub fn new(pool: sqlx::PgPool) -> Self {
        Self { pool }
    }

    /// `mark_as_read`: advance the seen pointer to `message_id`. The advance
    /// is monotonic (an older message never rewinds the pointer) and claimed
    /// under FOR NO KEY UPDATE SKIP LOCKED, so two racing reads can't corrupt
    /// the counters — the loser's write is simply a no-op.
    pub async fn mark_as_read(
        &self,
        channel_id: Uuid,
        identity: &MessagingIdentity,
        message_id: Uuid,
    ) -> Result<bool, MemberError> {
        let mut tx = self.pool.begin().await?;
        let key = MemberKey { channel_id, identity };
        let member_id = ChannelMemberRepository::member_id(&mut tx, &key, true)
            .await?
            .ok_or(MemberError::NotMember(channel_id))?;
        let advanced =
            ChannelMemberRepository::advance_seen(&mut tx, member_id, message_id).await?;
        if advanced {
            stage_bus_event(
                &mut tx,
                "MemberSeenAdvanced",
                "DiscussChannelMember",
                member_id,
                discuss_channel(channel_id),
                "discuss.channel.member/seen",
                serde_json::json!({
                    "channel_id": channel_id,
                    "member_id": member_id,
                    "seen_message_id": message_id,
                }),
            )
            .await?;
        }
        tx.commit().await?;
        Ok(advanced)
    }

    /// `channel_fetched`: the lighter client-acknowledged pointer (no counter
    /// reset, no seen semantics) — the typing-adjacent liveness write.
    pub async fn mark_fetched(
        &self,
        channel_id: Uuid,
        identity: &MessagingIdentity,
        message_id: Uuid,
    ) -> Result<bool, MemberError> {
        let mut tx = self.pool.begin().await?;
        let key = MemberKey { channel_id, identity };
        let member_id = ChannelMemberRepository::member_id(&mut tx, &key, true)
            .await?
            .ok_or(MemberError::NotMember(channel_id))?;
        let advanced =
            ChannelMemberRepository::advance_fetched(&mut tx, member_id, message_id).await?;
        tx.commit().await?;
        Ok(advanced)
    }

    /// Set the new-message separator (MAIL-M38): everything before
    /// `message_id` counts as old; counters zero on the same write.
    pub async fn set_new_message_separator(
        &self,
        channel_id: Uuid,
        identity: &MessagingIdentity,
        message_id: Uuid,
    ) -> Result<(), MemberError> {
        let mut tx = self.pool.begin().await?;
        let key = MemberKey { channel_id, identity };
        let member_id = ChannelMemberRepository::member_id(&mut tx, &key, false)
            .await?
            .ok_or(MemberError::NotMember(channel_id))?;
        ChannelMemberRepository::set_separator(&mut tx, member_id, message_id).await?;
        tx.commit().await?;
        Ok(())
    }

    /// Join a channel. Idempotent; membership gates are the route layer's
    /// (public/private checks) — the service only records the fact.
    pub async fn join(
        &self,
        channel_id: Uuid,
        identity: &MessagingIdentity,
    ) -> Result<bool, MemberError> {
        let mut tx = self.pool.begin().await?;
        let key = MemberKey { channel_id, identity };
        let joined = ChannelMemberRepository::upsert_join(&mut tx, &key).await?;
        if joined {
            stage_bus_event(
                &mut tx,
                "MemberJoined",
                "DiscussChannelMember",
                channel_id,
                discuss_channel(channel_id),
                "discuss.channel/member_added",
                serde_json::json!({
                    "channel_id": channel_id,
                    "partner_id": identity.partner_id(),
                }),
            )
            .await?;
        }
        tx.commit().await?;
        Ok(joined)
    }

    /// Leave a channel. Idempotent; guests leaving releases their persona
    /// (guest GC still applies).
    pub async fn leave(
        &self,
        channel_id: Uuid,
        identity: &MessagingIdentity,
    ) -> Result<bool, MemberError> {
        let mut tx = self.pool.begin().await?;
        let key = MemberKey { channel_id, identity };
        let member_id = ChannelMemberRepository::member_id(&mut tx, &key, false)
            .await?
            .ok_or(MemberError::NotMember(channel_id))?;
        let left = ChannelMemberRepository::soft_delete_member(&mut tx, member_id).await?;
        if left {
            stage_bus_event(
                &mut tx,
                "MemberLeft",
                "DiscussChannelMember",
                member_id,
                discuss_channel(channel_id),
                "discuss.channel/member_removed",
                serde_json::json!({
                    "channel_id": channel_id,
                    "partner_id": identity.partner_id(),
                }),
            )
            .await?;
        }
        tx.commit().await?;
        Ok(left)
    }

    /// Mute notifications until an instant, until unmuted (`forever`), or
    /// unmute (`neither until nor forever`).
    pub async fn set_mute(
        &self,
        channel_id: Uuid,
        identity: &MessagingIdentity,
        until: Option<chrono::DateTime<chrono::Utc>>,
        forever: bool,
    ) -> Result<(), MemberError> {
        let mut tx = self.pool.begin().await?;
        let key = MemberKey { channel_id, identity };
        let member_id = ChannelMemberRepository::member_id(&mut tx, &key, false)
            .await?
            .ok_or(MemberError::NotMember(channel_id))?;
        ChannelMemberRepository::set_mute(&mut tx, member_id, until, forever).await?;
        tx.commit().await?;
        Ok(())
    }

    /// Per-channel notification override (all/mentions/no_notif; None inherits
    /// the user's global setting).
    pub async fn set_custom_notifications(
        &self,
        channel_id: Uuid,
        identity: &MessagingIdentity,
        value: Option<&str>,
    ) -> Result<(), MemberError> {
        if let Some(v) = value {
            if !matches!(v, "all" | "mentions" | "no_notif") {
                return Err(MemberError::Invalid(format!(
                    "custom_notifications must be all|mentions|no_notif, got {v:?}"
                )));
            }
        }
        let mut tx = self.pool.begin().await?;
        let key = MemberKey { channel_id, identity };
        let member_id = ChannelMemberRepository::member_id(&mut tx, &key, false)
            .await?
            .ok_or(MemberError::NotMember(channel_id))?;
        ChannelMemberRepository::set_custom_notifications(&mut tx, member_id, value).await?;
        tx.commit().await?;
        Ok(())
    }

    /// Toggle the sidebar pin.
    pub async fn set_pinned(
        &self,
        channel_id: Uuid,
        identity: &MessagingIdentity,
        pinned: bool,
    ) -> Result<(), MemberError> {
        let mut tx = self.pool.begin().await?;
        let key = MemberKey { channel_id, identity };
        let member_id = ChannelMemberRepository::member_id(&mut tx, &key, false)
            .await?
            .ok_or(MemberError::NotMember(channel_id))?;
        ChannelMemberRepository::set_pinned(&mut tx, member_id, pinned).await?;
        tx.commit().await?;
        Ok(())
    }

    /// Set the sidebar fold state (open/closed; None = undefined).
    pub async fn set_fold_state(
        &self,
        channel_id: Uuid,
        identity: &MessagingIdentity,
        fold: Option<&str>,
    ) -> Result<(), MemberError> {
        if let Some(f) = fold {
            if !matches!(f, "open" | "closed") {
                return Err(MemberError::Invalid(format!(
                    "fold_state must be open|closed, got {f:?}"
                )));
            }
        }
        let mut tx = self.pool.begin().await?;
        let key = MemberKey { channel_id, identity };
        let member_id = ChannelMemberRepository::member_id(&mut tx, &key, false)
            .await?
            .ok_or(MemberError::NotMember(channel_id))?;
        ChannelMemberRepository::set_fold_state(&mut tx, member_id, fold).await?;
        tx.commit().await?;
        Ok(())
    }
}
