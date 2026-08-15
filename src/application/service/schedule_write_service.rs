//! The scheduled-message write service (hand-written; user-owned).
//!
//! Ports BOTH deferral mechanisms (source-verified pair, easily conflated —
//! see schema/models/schedule.model.yaml header):
//!
//! - **M8 notify-later** (`mail.message.schedule`): the message EXISTS; the
//!   notification fan-out is held. `dispatch_due_notify` claims due rows FOR
//!   UPDATE SKIP LOCKED, mints inbox notifications for the thread's followers
//!   on the same tx, then DELETES the row — no state column, existence is the
//!   pending state (ADR-0020).
//! - **M9 post-later** (`mail.scheduled.message`): the composer's post
//!   values held until `scheduled_date`. `dispatch_due_scheduled` re-checks
//!   the creator's post permission AT FIRE TIME (MAIL-B1 resolver — a
//!   permission revoked after arming must still block), posts through the
//!   message_post pump AS the creator, and on failure NOTIFIES the author
//!   instead of raising (the cron must survive one bad row).

use std::sync::Arc;
use uuid::Uuid;

use crate::application::service::chatter_acl::{
    MessagingIdentity, ThreadAccessResolver,
};
use crate::application::service::message_write_service::{
    MessagePostCommand, MessageWriteService, PostRecipient,
};
use crate::domain::event::constants::stage_bus_event;
use crate::infrastructure::persistence::follower_repository::FollowerRepository;
use crate::infrastructure::persistence::message_pipeline_repository::{
    MessagePipelineRepository, NewMailNotificationRow,
};
use crate::infrastructure::persistence::schedule_repository::ScheduleRepository;

#[derive(Debug, thiserror::Error)]
pub enum ScheduleError {
    #[error("db: {0}")]
    Db(#[from] sqlx::Error),
    #[error("invalid input: {0}")]
    Invalid(String),
    #[error("not found: schedule {0}")]
    NotFound(Uuid),
    #[error("forbidden: cannot post on {0} {1}")]
    CannotPost(String, Uuid),
}

pub struct ScheduleWriteService {
    pool: sqlx::PgPool,
    thread_acl: Arc<dyn ThreadAccessResolver>,
    /// The M8/M9 claim batch bound (commit_per_batch — bounds replay).
    pub batch_limit: i64,
}

impl ScheduleWriteService {
    pub fn new(pool: sqlx::PgPool, thread_acl: Arc<dyn ThreadAccessResolver>) -> Self {
        Self { pool, thread_acl, batch_limit: 100 }
    }

    // =========================================================================
    // M8 — notify-later
    // =========================================================================

    /// Arm M8: delay an already-posted message's notifications.
    pub async fn arm_notify(
        &self,
        mail_message_id: Uuid,
        scheduled_datetime: chrono::DateTime<chrono::Utc>,
        notification_parameters: Option<&str>,
    ) -> Result<Uuid, ScheduleError> {
        if scheduled_datetime <= chrono::Utc::now() {
            return Err(ScheduleError::Invalid(
                "scheduled_datetime must be in the future (a past schedule is just a post)".into(),
            ));
        }
        let id = Uuid::new_v4();
        let mut tx = self.pool.begin().await?;
        ScheduleRepository::arm_notify(
            &mut tx, id, mail_message_id, notification_parameters, scheduled_datetime,
        )
        .await?;
        stage_bus_event(
            &mut tx,
            "MailMessageScheduleCreated",
            "MailMessageSchedule",
            id,
            format!("mail.schedule_{id}"),
            "mail.message.schedule/insert",
            serde_json::json!({
                "schedule_id": id,
                "mail_message_id": mail_message_id,
                "scheduled_datetime": scheduled_datetime,
            }),
        )
        .await?;
        tx.commit().await?;
        Ok(id)
    }

    /// Cancel M8 (before it fires).
    pub async fn cancel_notify(&self, id: Uuid) -> Result<(), ScheduleError> {
        let mut tx = self.pool.begin().await?;
        if !ScheduleRepository::cancel_notify(&mut tx, id).await? {
            return Err(ScheduleError::NotFound(id));
        }
        tx.commit().await?;
        Ok(())
    }

    /// The M8 dispatch (self-arming job body): claim due rows SKIP LOCKED,
    /// mint the follower inbox notifications, delete the row — all one tx
    /// per batch. Returns the dispatched count.
    pub async fn dispatch_due_notify(&self) -> Result<usize, ScheduleError> {
        let mut tx = self.pool.begin().await?;
        let due = ScheduleRepository::claim_due_notify(&mut tx, chrono::Utc::now(), self.batch_limit)
            .await?;
        for s in &due {
            // Resolve the thread edge from the message itself.
            let thread = ScheduleRepository::thread_of_message(&mut tx, s.mail_message_id).await?;
            if let Some((Some(model), Some(res_id))) = thread {
                for (partner_id, _) in
                    FollowerRepository::list_followers(&mut tx, &model, res_id).await?
                {
                    MessagePipelineRepository::insert_mail_notification(
                        &mut tx,
                        &NewMailNotificationRow {
                            id: Uuid::new_v4(),
                            mail_message_id: s.mail_message_id,
                            res_partner_id: Some(partner_id),
                            notification_type: "inbox",
                            notification_status: "sent",
                            mail_mail_id_int: None,
                        },
                    )
                    .await?;
                }
            }
            ScheduleRepository::delete_notify(&mut tx, s.id).await?;
        }
        let n = due.len();
        tx.commit().await?;
        Ok(n)
    }

    // =========================================================================
    // M9 — post-later
    // =========================================================================

    /// Arm M9: hold composer values to post later. Checks the creator's post
    /// permission NOW (fast fail) — it is re-checked at fire time too.
    #[allow(clippy::too_many_arguments)]
    pub async fn arm_scheduled_message(
        &self,
        author_party_id: Uuid,
        model: &str,
        res_id: Uuid,
        body: &str,
        subject: Option<&str>,
        scheduled_date: chrono::DateTime<chrono::Utc>,
        recipient_party_ids: Option<serde_json::Value>,
        is_note: bool,
    ) -> Result<Uuid, ScheduleError> {
        if body.trim().is_empty() {
            return Err(ScheduleError::Invalid("body is required".into()));
        }
        if scheduled_date <= chrono::Utc::now() {
            return Err(ScheduleError::Invalid(
                "scheduled_date must be in the future".into(),
            ));
        }
        // MAIL-B1: the target must be a chatter host the author may post on.
        let identity = MessagingIdentity::User { partner_id: author_party_id };
        if !self
            .thread_acl
            .can_post(&self.pool, &identity, model, res_id)
            .await
        {
            return Err(ScheduleError::CannotPost(model.into(), res_id));
        }

        let id = Uuid::new_v4();
        let mut tx = self.pool.begin().await?;
        ScheduleRepository::arm_scheduled_message(
            &mut tx,
            id,
            subject,
            body,
            scheduled_date,
            None,
            model,
            res_id,
            author_party_id,
            recipient_party_ids.as_ref(),
            is_note,
            None,
            None,
        )
        .await?;
        stage_bus_event(
            &mut tx,
            "MailScheduledMessageCreated",
            "MailScheduledMessage",
            id,
            crate::domain::event::constants::partner_channel(author_party_id),
            "mail.scheduled.message/insert",
            serde_json::json!({
                "scheduled_message_id": id,
                "model": model,
                "res_id": res_id,
                "author_party_id": author_party_id,
                "scheduled_date": scheduled_date,
            }),
        )
        .await?;
        tx.commit().await?;
        Ok(id)
    }

    /// Cancel M9 (before it fires). Only the author (route-gated) reaches here.
    pub async fn cancel_scheduled_message(&self, id: Uuid) -> Result<(), ScheduleError> {
        let mut tx = self.pool.begin().await?;
        if !ScheduleRepository::cancel_scheduled_message(&mut tx, id).await? {
            return Err(ScheduleError::NotFound(id));
        }
        tx.commit().await?;
        Ok(())
    }

    /// The M9 dispatch (self-arming job body): claim due rows SKIP LOCKED,
    /// re-check post permission AT FIRE TIME, post via the message_post pump
    /// as the creator, delete the row. A row whose permission check (or post)
    /// fails is DELETED with a notification to the author — the cron path
    /// never raises. Returns (posted, skipped).
    pub async fn dispatch_due_scheduled(
        &self,
        poster: &MessageWriteService,
    ) -> Result<(usize, usize), ScheduleError> {
        let mut tx = self.pool.begin().await?;
        let due = ScheduleRepository::claim_due_scheduled_messages(
            &mut tx,
            chrono::Utc::now(),
            self.batch_limit,
        )
        .await?;
        // Claim rows, then dispatch each through the pump (the pump opens its
        // own tx — so delete on THIS tx only after the pump succeeded; a pump
        // failure means we delete-and-notify rather than roll everything back).
        let mut posted = 0usize;
        let mut skipped = 0usize;
        for m in &due {
            ScheduleRepository::delete_scheduled_message(&mut tx, m.id).await?;
            let ok = self.dispatch_one(poster, m).await;
            match ok {
                true => posted += 1,
                false => {
                    skipped += 1;
                    self.notify_author_of_failure(m).await;
                }
            }
        }
        tx.commit().await?;
        Ok((posted, skipped))
    }

    /// Fire one M9 row. Permission re-check at fire time (MAIL-B1), then the
    /// pump as the creator. `false` = denied or invalid (never panics, never
    /// raises — the caller records the author notification).
    async fn dispatch_one(
        &self,
        poster: &MessageWriteService,
        m: &crate::infrastructure::persistence::schedule_repository::ClaimedScheduledMessage,
    ) -> bool {
        let identity = MessagingIdentity::User { partner_id: m.author_party_id };
        if !self.thread_acl.can_post(&self.pool, &identity, &m.model, m.res_id).await {
            return false;
        }
        let recipients: Vec<PostRecipient> = m
            .recipient_party_ids
            .as_ref()
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|p| p.as_str().and_then(|s| Uuid::parse_str(s).ok()))
                    .map(|partner_id| PostRecipient {
                        res_partner_id: Some(partner_id),
                        channel: crate::application::service::message_write_service::NotificationChannel::Inbox,
                        email: None,
                        number: None,
                    })
                    .collect()
            })
            .unwrap_or_default();
        poster
            .message_post(MessagePostCommand {
                body: m.body.clone(),
                subject: m.subject.clone(),
                message_type: if m.is_note { "notification" } else { "comment" }.into(),
                is_internal: m.is_note,
                author_id: Some(m.author_party_id),
                model: Some(m.model.clone()),
                res_id: Some(m.res_id),
                recipients,
                ..Default::default()
            })
            .await
            .is_ok()
    }

    /// The M9 failure path: an inbox notification on the author's own wall —
    /// "your scheduled message could not be posted". Best-effort (a failure
    /// here is logged by the job runner's tracing, not propagated).
    async fn notify_author_of_failure(
        &self,
        m: &crate::infrastructure::persistence::schedule_repository::ClaimedScheduledMessage,
    ) {
        let note_id = Uuid::new_v4();
        let mut tx = match self.pool.begin().await {
            Ok(tx) => tx,
            Err(_) => return,
        };
        let posted = poster_message_id(&mut tx, m.author_party_id).await;
        if let Some(author_wall) = posted {
            let _ = MessagePipelineRepository::insert_mail_notification(
                &mut tx,
                &NewMailNotificationRow {
                    id: note_id,
                    mail_message_id: author_wall,
                    res_partner_id: Some(m.author_party_id),
                    notification_type: "inbox",
                    notification_status: "sent",
                    mail_mail_id_int: None,
                },
            )
            .await;
            let _ = tx.commit().await;
        }
    }
}

/// Find (or mint) a notification carrier message on the author's own wall.
/// Reuses the latest wall message when present; else mints a minimal note.
async fn poster_message_id(
    tx: &mut sqlx::PgConnection,
    author_party_id: Uuid,
) -> Option<Uuid> {
    if let Some(id) = ScheduleRepository::latest_author_wall_message(&mut *tx, author_party_id)
        .await
        .ok()
        .flatten()
    {
        return Some(id);
    }
    let id = Uuid::new_v4();
    MessagePipelineRepository::insert_mail_message(
        tx,
        &crate::infrastructure::persistence::message_pipeline_repository::NewMailMessageRow {
            id,
            subject: Some("Scheduled message not posted"),
            body: "A scheduled message could not be posted (permission was revoked or the target is gone).",
            message_type: "notification",
            subtype_id: None,
            is_internal: true,
            author_id: None,
            author_guest_id: None,
            email_from: None,
            message_id: None,
            reply_to: None,
            model: Some("res.partner"),
            res_id: Some(author_party_id),
            record_name: None,
        },
    )
    .await
    .ok()?;
    Some(id)
}
