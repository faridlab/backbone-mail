//! The message query service (hand-written; user-owned).
//!
//! Port of `_message_fetch` (MAIL-M1 read side). The load-bearing rule is
//! **MAIL-B1 applied procedurally**: every fetch mode gates the caller
//! through code BEFORE the query runs — `can_read` on the thread resolver for
//! host-document threads, live-membership for discuss channels — and the
//! repository layer never sees an ir.rule-shaped WHERE. A new thread kind is
//! reachable only by teaching the resolver, never by editing SQL.

use std::sync::Arc;
use uuid::Uuid;

use crate::application::service::chatter_acl::{MessagingIdentity, ThreadAccessResolver};
use crate::infrastructure::persistence::channel_member_repository::{
    ChannelMemberRepository, MemberKey,
};
use crate::infrastructure::persistence::chatter_repository::{
    ChatterRepository, FetchedMessage,
};

#[derive(Debug, thiserror::Error)]
pub enum MessageQueryError {
    #[error("db: {0}")]
    Db(#[from] sqlx::Error),
    #[error("forbidden: no read access to {0} {1}")]
    Forbidden(String, Uuid),
    #[error("not a channel member")]
    NotAMember,
}

pub struct MessageQueryService {
    pool: sqlx::PgPool,
    thread_acl: Arc<dyn ThreadAccessResolver>,
    /// The fetch page size bound (Odoo fetches 30 by default).
    pub page_limit: i64,
}

impl MessageQueryService {
    pub fn new(pool: sqlx::PgPool, thread_acl: Arc<dyn ThreadAccessResolver>) -> Self {
        Self { pool, thread_acl, page_limit: 30 }
    }

    /// Fetch a host document's chatter thread (MAIL-B1: can_read FIRST —
    /// denied readers never reach the query).
    pub async fn fetch_thread(
        &self,
        identity: &MessagingIdentity,
        model: &str,
        res_id: Uuid,
        after_id: Option<Uuid>,
    ) -> Result<Vec<FetchedMessage>, MessageQueryError> {
        if !self.thread_acl.can_read(&self.pool, identity, model, res_id).await {
            return Err(MessageQueryError::Forbidden(model.into(), res_id));
        }
        let mut conn = self.pool.acquire().await?;
        ChatterRepository::thread_messages(&mut conn, model, res_id, after_id, self.page_limit)
            .await
            .map_err(Into::into)
    }

    /// Fetch a discuss channel's messages (gate: LIVE membership — the
    /// member row must exist and not be soft-deleted).
    pub async fn fetch_channel(
        &self,
        identity: &MessagingIdentity,
        channel_id: Uuid,
        after_id: Option<Uuid>,
    ) -> Result<Vec<FetchedMessage>, MessageQueryError> {
        self.require_member(channel_id, identity).await?;
        let mut conn = self.pool.acquire().await?;
        ChatterRepository::channel_messages(&mut conn, channel_id, after_id, self.page_limit)
            .await
            .map_err(Into::into)
    }

    /// Fetch the identity's inbox (partner-only — guests have no inbox).
    pub async fn fetch_inbox(
        &self,
        identity: &MessagingIdentity,
    ) -> Result<Vec<FetchedMessage>, MessageQueryError> {
        let partner_id = match identity {
            MessagingIdentity::User { partner_id } => *partner_id,
            MessagingIdentity::Guest { .. } => {
                return Err(MessageQueryError::Forbidden("res.partner".into(), Uuid::nil()))
            }
        };
        let mut conn = self.pool.acquire().await?;
        ChatterRepository::inbox_messages(&mut conn, partner_id, self.page_limit)
            .await
            .map_err(Into::into)
    }

    /// Fetch the partner's starred messages (partner-only by the m2m shape).
    pub async fn fetch_starred(
        &self,
        identity: &MessagingIdentity,
    ) -> Result<Vec<FetchedMessage>, MessageQueryError> {
        let partner_id = match identity {
            MessagingIdentity::User { partner_id } => *partner_id,
            MessagingIdentity::Guest { .. } => {
                return Err(MessageQueryError::Forbidden("res.partner".into(), Uuid::nil()))
            }
        };
        let mut conn = self.pool.acquire().await?;
        ChatterRepository::starred_messages(&mut conn, partner_id, self.page_limit)
            .await
            .map_err(Into::into)
    }

    /// The inbox unread counter (partner-only).
    pub async fn inbox_unread_count(
        &self,
        identity: &MessagingIdentity,
    ) -> Result<i64, MessageQueryError> {
        let partner_id = match identity {
            MessagingIdentity::User { partner_id } => *partner_id,
            MessagingIdentity::Guest { .. } => return Ok(0),
        };
        let mut conn = self.pool.acquire().await?;
        ChatterRepository::inbox_unread_count(&mut conn, partner_id)
            .await
            .map_err(Into::into)
    }

    /// Per-channel unread counters (partner-only; the sidebar badge input).
    pub async fn channel_unread_counts(
        &self,
        identity: &MessagingIdentity,
    ) -> Result<Vec<(Uuid, i64)>, MessageQueryError> {
        let partner_id = match identity {
            MessagingIdentity::User { partner_id } => *partner_id,
            MessagingIdentity::Guest { .. } => return Ok(Vec::new()),
        };
        let mut conn = self.pool.acquire().await?;
        ChatterRepository::channel_unread_counts(&mut conn, partner_id)
            .await
            .map_err(Into::into)
    }

    /// The MAIL-B1 walk for a single message's gate: resolve the thread edge,
    /// then the thread's gate. Discuss channels route to membership; anything
    /// else to the resolver.
    pub async fn can_read_message(
        &self,
        identity: &MessagingIdentity,
        message_id: Uuid,
    ) -> Result<bool, MessageQueryError> {
        let mut conn = self.pool.acquire().await?;
        let thread = ChatterRepository::message_thread(&mut conn, message_id).await?;
        drop(conn);
        match thread {
            Some((Some(model), Some(res_id))) => {
                if model == "discuss.channel" {
                    self.require_member(res_id, identity).await.map(|_| true).or_else(|e| match e {
                        MessageQueryError::NotAMember => Ok(false),
                        other => Err(other),
                    })
                } else {
                    Ok(self.thread_acl.can_read(&self.pool, identity, &model, res_id).await)
                }
            }
            _ => Ok(false),
        }
    }

    /// Membership gate: the member row must exist live for this identity.
    async fn require_member(
        &self,
        channel_id: Uuid,
        identity: &MessagingIdentity,
    ) -> Result<(), MessageQueryError> {
        let mut conn = self.pool.acquire().await?;
        let member = ChannelMemberRepository::member_id(
            &mut conn,
            &MemberKey { channel_id, identity },
            false,
        )
        .await?;
        match member {
            Some(_) => Ok(()),
            None => Err(MessageQueryError::NotAMember),
        }
    }
}
