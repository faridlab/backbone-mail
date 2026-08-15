//! The channel query service (hand-written; user-owned).
//!
//! Read side of DiscussChannel (M37): member lists, pinned messages, and the
//! `/discuss/search` port. Every method gates on live membership FIRST
//! (MAIL-B1 procedural — the repository queries run only for callers the
//! gate admitted); search is the one exception by construction (the query
//! itself returns only member-visible or public-joinable channels).

use uuid::Uuid;

use crate::application::service::chatter_acl::MessagingIdentity;
use crate::infrastructure::persistence::channel_member_repository::{
    ChannelMemberRepository, MemberKey,
};
use crate::infrastructure::persistence::channel_repository::ChannelRepository;

/// One search hit: (channel_id, name, channel_type, is_member).
#[derive(Debug, Clone, serde::Serialize)]
pub struct ChannelSearchHit {
    pub channel_id: Uuid,
    pub name: Option<String>,
    pub channel_type: String,
    pub is_member: bool,
}

#[derive(Debug, thiserror::Error)]
pub enum ChannelQueryError {
    #[error("db: {0}")]
    Db(#[from] sqlx::Error),
    #[error("not a channel member")]
    NotAMember,
}

pub struct ChannelQueryService {
    pool: sqlx::PgPool,
    /// The search result bound (Odoo caps discussion search small).
    pub search_limit: i64,
}

impl ChannelQueryService {
    pub fn new(pool: sqlx::PgPool) -> Self {
        Self { pool, search_limit: 20 }
    }

    /// Live members of a channel (member-gated).
    pub async fn members(
        &self,
        identity: &MessagingIdentity,
        channel_id: Uuid,
    ) -> Result<Vec<(Uuid, Option<Uuid>, Option<Uuid>)>, ChannelQueryError> {
        self.require_member(channel_id, identity).await?;
        let mut conn = self.pool.acquire().await?;
        ChannelRepository::list_members(&mut conn, channel_id).await.map_err(Into::into)
    }

    /// The channel's pinned message ids (member-gated).
    pub async fn pinned_messages(
        &self,
        identity: &MessagingIdentity,
        channel_id: Uuid,
    ) -> Result<Vec<Uuid>, ChannelQueryError> {
        self.require_member(channel_id, identity).await?;
        let mut conn = self.pool.acquire().await?;
        ChannelRepository::pinned_message_ids(&mut conn, channel_id).await.map_err(Into::into)
    }

    /// `/discuss/search`: channels the identity may see — their memberships
    /// plus public-joinable channels, optionally name-filtered. Guests get
    /// ONLY their own member channels (the public-joinable arm requires a
    /// partner identity, matching Odoo's group_public gate).
    pub async fn search(
        &self,
        identity: &MessagingIdentity,
        term: Option<&str>,
    ) -> Result<Vec<ChannelSearchHit>, ChannelQueryError> {
        let (partner_id, guest_id) = match identity {
            MessagingIdentity::User { partner_id } => (Some(*partner_id), None),
            MessagingIdentity::Guest { guest_id } => (None, Some(*guest_id)),
        };
        let rows =
            ChannelRepository::search_channels(&self.pool, partner_id, guest_id, term.unwrap_or(""), self.search_limit)
                .await?;
        Ok(rows
            .into_iter()
            .map(|(channel_id, name, channel_type, is_member)| ChannelSearchHit {
                channel_id,
                name,
                channel_type,
                is_member,
            })
            .collect())
    }

    async fn require_member(
        &self,
        channel_id: Uuid,
        identity: &MessagingIdentity,
    ) -> Result<(), ChannelQueryError> {
        let mut conn = self.pool.acquire().await?;
        let member = ChannelMemberRepository::member_id(
            &mut conn,
            &MemberKey { channel_id, identity },
            false,
        )
        .await?;
        match member {
            Some(_) => Ok(()),
            None => Err(ChannelQueryError::NotAMember),
        }
    }
}
