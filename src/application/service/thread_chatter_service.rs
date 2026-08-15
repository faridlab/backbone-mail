//! The thread-chatter facade (hand-written; user-owned).
//!
//! THE edge API a host module calls to get chatter on its documents
//! (MAIL-M16): post, read, follow/unfollow, and schedule — each verb gated by
//! the registered [`ThreadAccessResolver`] (MAIL-B1 procedural, deny-by-
//! default for host docs until the host registers). Hosts never touch the
//! underlying services directly; this facade is the stable seam.

use std::sync::Arc;
use uuid::Uuid;

use crate::application::service::chatter_acl::{MessagingIdentity, ThreadAccessResolver};
use crate::application::service::message_query_service::MessageQueryService;
use crate::application::service::message_write_service::{
    MessagePostCommand, MessageWriteService, PostedMessage,
};
use crate::application::service::schedule_write_service::ScheduleWriteService;
use crate::infrastructure::persistence::chatter_repository::FetchedMessage;

#[derive(Debug, thiserror::Error)]
pub enum ChatterError {
    #[error("db: {0}")]
    Db(#[from] sqlx::Error),
    #[error("forbidden: cannot post on {0} {1}")]
    CannotPost(String, Uuid),
    #[error("forbidden: cannot read {0} {1}")]
    CannotRead(String, Uuid),
    #[error("invalid input: {0}")]
    Invalid(String),
    #[error("mail: {0}")]
    Mail(#[from] crate::application::service::message_write_service::MailError),
    #[error("schedule: {0}")]
    Schedule(#[from] crate::application::service::schedule_write_service::ScheduleError),
    #[error("query: {0}")]
    Query(#[from] crate::application::service::message_query_service::MessageQueryError),
    #[error("follower: {0}")]
    Follower(#[from] crate::application::service::follower_write_service::FollowerError),
}

pub struct ThreadChatterService {
    pool: sqlx::PgPool,
    thread_acl: Arc<dyn ThreadAccessResolver>,
    poster: MessageWriteService,
    queries: MessageQueryService,
    schedules: ScheduleWriteService,
}

impl ThreadChatterService {
    pub fn new(
        pool: sqlx::PgPool,
        thread_acl: Arc<dyn ThreadAccessResolver>,
    ) -> Self {
        Self {
            poster: MessageWriteService::new(pool.clone()),
            queries: MessageQueryService::new(pool.clone(), Arc::clone(&thread_acl)),
            schedules: ScheduleWriteService::new(pool.clone(), Arc::clone(&thread_acl)),
            thread_acl,
            pool,
        }
    }

    /// The registered resolver (for host-side introspection/tests).
    pub fn resolver(&self) -> Arc<dyn ThreadAccessResolver> {
        Arc::clone(&self.thread_acl)
    }

    /// Post a user comment onto a host document's thread (the `message_post`
    /// edge). MAIL-B1: `can_post` is checked HERE — hosts cannot bypass the
    /// gate by calling the pump directly through this seam.
    pub async fn post(
        &self,
        identity: &MessagingIdentity,
        model: &str,
        res_id: Uuid,
        body: &str,
        subject: Option<&str>,
        is_note: bool,
    ) -> Result<PostedMessage, ChatterError> {
        if !self.thread_acl.can_post(&self.pool, identity, model, res_id).await {
            return Err(ChatterError::CannotPost(model.into(), res_id));
        }
        let author = match identity {
            MessagingIdentity::User { partner_id } => Some(*partner_id),
            MessagingIdentity::Guest { .. } => None,
        };
        let guest = match identity {
            MessagingIdentity::Guest { guest_id } => Some(*guest_id),
            _ => None,
        };
        Ok(self
            .poster
            .message_post(MessagePostCommand {
                body: body.to_string(),
                subject: subject.map(str::to_string),
                message_type: if is_note { "notification" } else { "comment" }.into(),
                is_internal: is_note,
                author_id: author,
                author_guest_id: guest,
                model: Some(model.to_string()),
                res_id: Some(res_id),
                ..Default::default()
            })
            .await?)
    }

    /// Read a host document's thread (MAIL-B1 can_read gate).
    pub async fn read_thread(
        &self,
        identity: &MessagingIdentity,
        model: &str,
        res_id: Uuid,
        after_id: Option<Uuid>,
    ) -> Result<Vec<FetchedMessage>, ChatterError> {
        Ok(self.queries.fetch_thread(identity, model, res_id, after_id).await?)
    }

    /// Follow a document's thread as a partner (the host-side subscription
    /// verb — routed to the follower write service; default subtypes, Skip on
    /// an existing subscription).
    pub async fn follow(
        &self,
        partner_id: Uuid,
        model: &str,
        res_id: Uuid,
    ) -> Result<
        crate::infrastructure::persistence::follower_repository::SubscribeOutcome,
        ChatterError,
    > {
        use crate::application::service::follower_write_service::FollowerWriteService;
        use crate::infrastructure::persistence::follower_repository::ExistingPolicy;
        Ok(FollowerWriteService::new(self.pool.clone())
            .subscribe(model, res_id, partner_id, Vec::new(), ExistingPolicy::Skip)
            .await?)
    }

    /// Unfollow (idempotent — a missing follower row is not an error).
    pub async fn unfollow(
        &self,
        partner_id: Uuid,
        model: &str,
        res_id: Uuid,
    ) -> Result<u64, ChatterError> {
        use crate::application::service::follower_write_service::FollowerWriteService;
        Ok(FollowerWriteService::new(self.pool.clone())
            .unsubscribe(model, res_id, vec![partner_id])
            .await?)
    }

    /// Schedule a post for later (M9 through the facade — same can_post gate
    /// at arm time; the fire-time re-check is the scheduler's).
    #[allow(clippy::too_many_arguments)]
    pub async fn schedule_post(
        &self,
        author_party_id: Uuid,
        model: &str,
        res_id: Uuid,
        body: &str,
        subject: Option<&str>,
        scheduled_date: chrono::DateTime<chrono::Utc>,
        is_note: bool,
    ) -> Result<Uuid, ChatterError> {
        Ok(self
            .schedules
            .arm_scheduled_message(
                author_party_id,
                model,
                res_id,
                body,
                subject,
                scheduled_date,
                None,
                is_note,
            )
            .await?)
    }

    /// The M9 dispatch entry the app-service job runner calls (self-arming
    /// job body — not part of the host edge, but lives with the facade so the
    /// fire-time re-check runs through the SAME resolver).
    pub async fn dispatch_due_scheduled(&self) -> Result<(usize, usize), ChatterError> {
        let (posted, skipped) = self.schedules.dispatch_due_scheduled(&self.poster).await?;
        Ok((posted, skipped))
    }

    /// The M8 dispatch entry the app-service job runner calls.
    pub async fn dispatch_due_notify(&self) -> Result<usize, ChatterError> {
        Ok(self.schedules.dispatch_due_notify().await?)
    }
}
