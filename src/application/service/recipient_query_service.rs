//! The recipient query service (hand-written; user-owned).
//!
//! Port of `_message_get_suggested_recipients` — the composer's @-mention and
//! "notify" picker for a host-document thread: the thread's followers plus
//! everyone who already posted there. Read-gated by the SAME MAIL-B1 resolver
//! as the message fetch (a caller who cannot read the thread cannot enumerate
//! its participants).

use std::sync::Arc;
use uuid::Uuid;

use crate::application::service::chatter_acl::{MessagingIdentity, ThreadAccessResolver};
use crate::domain::event::constants::partner_channel;
use crate::infrastructure::persistence::chatter_repository::ChatterRepository;
use crate::infrastructure::persistence::follower_repository::FollowerRepository;

/// One suggested recipient.
#[derive(Debug, Clone, serde::Serialize)]
pub struct SuggestedRecipient {
    pub partner_id: Uuid,
    /// "follower" | "author" (a partner can be both — follower wins, it is
    /// the stronger signal in Odoo's dedup).
    pub reason: &'static str,
}

#[derive(Debug, thiserror::Error)]
pub enum RecipientQueryError {
    #[error("db: {0}")]
    Db(#[from] sqlx::Error),
    #[error("forbidden: no read access to {0} {1}")]
    Forbidden(String, Uuid),
    #[error("recipients need a partner identity")]
    NeedsPartner,
}

pub struct RecipientQueryService {
    pool: sqlx::PgPool,
    thread_acl: Arc<dyn ThreadAccessResolver>,
}

impl RecipientQueryService {
    pub fn new(pool: sqlx::PgPool, thread_acl: Arc<dyn ThreadAccessResolver>) -> Self {
        Self { pool, thread_acl }
    }

    /// Suggested recipients for a thread: followers ∪ authors, follower
    /// reason winning on overlap. MAIL-B1 gate first.
    pub async fn suggested(
        &self,
        identity: &MessagingIdentity,
        model: &str,
        res_id: Uuid,
    ) -> Result<Vec<SuggestedRecipient>, RecipientQueryError> {
        if !self.thread_acl.can_read(&self.pool, identity, model, res_id).await {
            return Err(RecipientQueryError::Forbidden(model.into(), res_id));
        }
        let mut conn = self.pool.acquire().await?;
        let followers = FollowerRepository::list_followers(&mut conn, model, res_id).await?;
        let authors = ChatterRepository::thread_authors(&mut conn, model, res_id).await?;

        let mut out: Vec<SuggestedRecipient> = followers
            .iter()
            .map(|(partner_id, _)| SuggestedRecipient { partner_id: *partner_id, reason: "follower" })
            .collect();
        for author in authors {
            if !followers.iter().any(|(pid, _)| *pid == author) {
                out.push(SuggestedRecipient { partner_id: author, reason: "author" });
            }
        }
        Ok(out)
    }

    /// The partner lookup behind `/mail/partner/from_email`-adjacent
    /// suggestions: which partner channel a partner resolves to (kept as a
    /// query-service helper so routes never build channel keys themselves —
    /// BUS-B2 server-side construction).
    pub fn partner_channel_of(partner_id: Uuid) -> String {
        partner_channel(partner_id)
    }
}
