//! The typing indicator service (hand-written; user-owned).
//!
//! Port of `discuss.channel.notify_typing` (MAIL-M38 adjunct): typing is a
//! PURE BUS EVENT — nothing is persisted, by design (Odoo stores nothing
//! either; a typing row would be write-amplified chat noise). The one state
//! touch: when a NON-member's client claims to type, we find-or-create their
//! membership first (the plan's typing=true find-or-create rule) so the
//! indicator can't be spoofed by a stranger onto a channel stream.

use uuid::Uuid;

use crate::application::service::chatter_acl::MessagingIdentity;
use crate::domain::event::constants::{discuss_channel, stage_bus_event};
use crate::infrastructure::persistence::channel_member_repository::{
    ChannelMemberRepository, MemberKey,
};

#[derive(Debug, thiserror::Error)]
pub enum TypingError {
    #[error("db: {0}")]
    Db(#[from] sqlx::Error),
}

pub struct TypingService {
    pool: sqlx::PgPool,
}

impl TypingService {
    pub fn new(pool: sqlx::PgPool) -> Self {
        Self { pool }
    }

    /// Broadcast a typing indicator on the channel stream. `typing = true`
    /// first ensures the sender's membership exists (find-or-create); typing
    /// = false is fire-and-forget (a stale stop after leaving must not fail).
    pub async fn notify_typing(
        &self,
        channel_id: Uuid,
        identity: &MessagingIdentity,
        typing: bool,
    ) -> Result<(), TypingError> {
        let mut tx = self.pool.begin().await?;
        let key = MemberKey { channel_id, identity };
        if typing {
            // Find-or-create: the indicator implies presence.
            ChannelMemberRepository::upsert_join(&mut tx, &key).await?;
        }
        stage_bus_event(
            &mut tx,
            "TypingNotification",
            "DiscussChannel",
            channel_id,
            discuss_channel(channel_id),
            if typing { "discuss.channel/typing" } else { "discuss.channel/typing_stopped" },
            serde_json::json!({
                "channel_id": channel_id,
                "partner_id": identity.partner_id(),
                "guest_id": match identity {
                    MessagingIdentity::Guest { guest_id } => Some(*guest_id),
                    _ => None,
                },
                "typing": typing,
            }),
        )
        .await?;
        tx.commit().await?;
        Ok(())
    }
}
