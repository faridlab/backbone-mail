//! The message_post write service — the notify pump (hand-written; user-owned).
//!
//! The port of Odoo `mail.thread.message_post` + `_notify_thread` (TR-MAIL-1/TR-MAIL-2,
//! MAIL-M16/B): mint the `mail_message`, then fan the recipients out through the
//! CHANNEL-DISPATCHED pump — one strategy per `notification_type`:
//!
//! | Channel | Strategy mints |
//! |---|---|
//! | `inbox` | `mail_notification(status='sent')` instantly — no MTA hop (instant delivered) |
//! | `email` | `mail_notification(status='ready')` per recipient + ONE `mail` queue row |
//! | `sms`   | `mail_notification(status='ready')` + `sms` row + uuid-correlated `sms_tracker` |
//!
//! Everything lands in ONE transaction, and the `MessagePosted` bus event (+ the
//! `SmsCreated` queue re-arm, TR-SM-1) is staged to the outbox IN that transaction —
//! a crash between the write and any downstream publish cannot drop the event.
//! To add a channel (web_push arrives with MAIL-M13), add a `NotificationChannel`
//! value + a strategy impl — that is the whole extension seam.

use uuid::Uuid;

use crate::domain::event::{record_channel, stage_bus_event};
use crate::infrastructure::persistence::message_pipeline_repository::{
    MessagePipelineRepository, MintedNotification, NewMailMessageRow, NewMailNotificationRow,
    NewMailQueueRow, NewSmsRow,
};

/// The notify channels base mail knows + the sms fold (MAIL-M3 + SM-M5). The enum
/// IS the channel-dispatch seam: a new member requires a new strategy (below).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NotificationChannel {
    Inbox,
    Email,
    Sms,
}

impl NotificationChannel {
    pub fn as_str(self) -> &'static str {
        match self {
            NotificationChannel::Inbox => "inbox",
            NotificationChannel::Email => "email",
            NotificationChannel::Sms => "sms",
        }
    }
}

/// One recipient of a post, already resolved to their notify channel by the caller
/// (Odoo's `_notify_thread_recipients` intersects followers' subtype filters with
/// the explicit partner list — that resolution is the caller's concern).
#[derive(Debug, Clone)]
pub struct PostRecipient {
    pub res_partner_id: Option<Uuid>,
    pub channel: NotificationChannel,
    /// The email address (email channel). Required for email.
    pub email: Option<String>,
    /// The phone number, international form (sms channel). Required for sms.
    pub number: Option<String>,
}

/// The canonical post command (TR-MAIL-1).
#[derive(Debug, Clone, Default)]
pub struct MessagePostCommand {
    pub body: String,
    pub subject: Option<String>,
    /// email | comment | notification | sms (message_type on the row; 'sms' per SM-M6).
    pub message_type: String,
    pub subtype_id: Option<Uuid>,
    /// Resolve by name when no explicit id (falls back to the module default subtype).
    pub subtype_name: Option<String>,
    pub is_internal: bool,
    pub author_id: Option<Uuid>,
    pub author_guest_id: Option<Uuid>,
    pub email_from: Option<String>,
    pub reply_to: Option<String>,
    /// The chatter host edge — which document this message belongs to.
    pub model: Option<String>,
    pub res_id: Option<Uuid>,
    pub record_name: Option<String>,
    pub recipients: Vec<PostRecipient>,
}

/// What a post minted — the audit surface for the caller.
#[derive(Debug, Clone, Default)]
pub struct PostedMessage {
    pub message_id: Uuid,
    pub subtype_id: Option<Uuid>,
    pub notifications: Vec<MintedNotification>,
    /// The email channel's single queue row, when any email recipient existed.
    pub mail_id: Option<Uuid>,
    /// The sms channel's rows: (sms_id, uuid correlation key).
    pub sms: Vec<(Uuid, String)>,
}

#[derive(Debug, thiserror::Error)]
pub enum MailError {
    #[error("db: {0}")]
    Db(#[from] sqlx::Error),
    #[error("invalid input: {0}")]
    Invalid(String),
}

/// The service. Stateless over a pool; every public verb opens its own unit of work.
/// (The repository is a stateless module of associated fns — SQL holders, no state.)
pub struct MessageWriteService {
    pool: sqlx::PgPool,
}

impl MessageWriteService {
    pub fn new(pool: sqlx::PgPool) -> Self {
        Self { pool }
    }

    /// `message_post` — mint the message + fan the recipients out through the
    /// channel-dispatched pump, ONE transaction, with the bus events staged in-tx.
    pub async fn message_post(&self, cmd: MessagePostCommand) -> Result<PostedMessage, MailError> {
        if cmd.body.trim().is_empty() {
            return Err(MailError::Invalid("message needs a body".into()));
        }
        if cmd.model.is_some() != cmd.res_id.is_some() {
            return Err(MailError::Invalid(
                "model and res_id travel together (the chatter edge is a pair)".into(),
            ));
        }

        let mut tx = self.pool.begin().await?;

        // _message_create — subtype resolution first (TR-MAIL-1).
        let subtype_id = MessagePipelineRepository::resolve_subtype(
            &mut tx, cmd.subtype_id, cmd.subtype_name.as_deref(),
        )
        .await?;
        let message_id = Uuid::new_v4();
        MessagePipelineRepository::insert_mail_message(
            &mut tx,
            &NewMailMessageRow {
                id: message_id,
                subject: cmd.subject.as_deref(),
                body: &cmd.body,
                message_type: if cmd.message_type.is_empty() { "comment" } else { &cmd.message_type },
                subtype_id,
                is_internal: cmd.is_internal,
                author_id: cmd.author_id,
                author_guest_id: cmd.author_guest_id,
                email_from: cmd.email_from.as_deref(),
                message_id: None,
                reply_to: cmd.reply_to.as_deref(),
                model: cmd.model.as_deref(),
                res_id: cmd.res_id,
                record_name: cmd.record_name.as_deref(),
            },
        )
        .await?;

        // _notify_thread — group recipients by channel, dispatch each group to its
        // strategy on the SAME open transaction.
        let ctx = PostContext {
            message_id,
            body: cmd.body.clone(),
            model: cmd.model.clone(),
            res_id: cmd.res_id,
            reply_to: cmd.reply_to.clone(),
        };
        let mut posted = PostedMessage { message_id, subtype_id, ..Default::default() };
        for (channel, recipients) in group_by_channel(cmd.recipients) {
            let minted = dispatch_channel(&mut tx, &ctx, channel, &recipients).await?;
            posted.notifications.extend(minted.notifications);
            if minted.mail_id.is_some() {
                posted.mail_id = minted.mail_id;
            }
            posted.sms.extend(minted.sms);
        }

        // The bus events, staged in-tx with the writes that produced them.
        let channel_key = match (&ctx.model, ctx.res_id) {
            (Some(m), Some(r)) => record_channel(m, r),
            // A model-less post (e.g. a Discuss message) has no record channel;
            // address it on the message pseudo-channel.
            _ => format!("mail.message_{message_id}"),
        };
        stage_bus_event(
            &mut tx,
            "MessagePosted",
            "MailMessage",
            message_id,
            channel_key.clone(),
            "MessagePosted",
            serde_json::json!({
                "message_id": message_id,
                "message_type": cmd.message_type,
                "model": cmd.model,
                "res_id": cmd.res_id,
                "notification_count": posted.notifications.len(),
            }),
        )
        .await?;
        // TR-SM-1: every sms mint re-arms the queue drainer (Odoo force-triggers the
        // sms cron on every sms.sms create).
        for (sms_id, sms_uuid) in &posted.sms {
            stage_bus_event(
                &mut tx,
                "SmsCreated",
                "Sms",
                sms_id,
                channel_key.clone(),
                "SmsCreated",
                serde_json::json!({ "sms_id": sms_id, "uuid": sms_uuid }),
            )
            .await?;
        }

        tx.commit().await?;
        Ok(posted)
    }
}

/// What the strategies see: the minted message + the post's addressing.
struct PostContext {
    message_id: Uuid,
    body: String,
    model: Option<String>,
    res_id: Option<Uuid>,
    reply_to: Option<String>,
}

/// What one strategy minted for its recipient group.
struct ChannelMint {
    notifications: Vec<MintedNotification>,
    mail_id: Option<Uuid>,
    sms: Vec<(Uuid, String)>,
}

fn group_by_channel(recipients: Vec<PostRecipient>) -> Vec<(NotificationChannel, Vec<PostRecipient>)> {
    let mut groups: Vec<(NotificationChannel, Vec<PostRecipient>)> = Vec::new();
    for r in recipients {
        match groups.iter_mut().find(|(c, _)| *c == r.channel) {
            Some((_, list)) => list.push(r),
            None => groups.push((r.channel, vec![r])),
        }
    }
    groups
}

/// THE channel dispatcher (TR-MAIL-2): each `NotificationChannel` resolves to its
/// strategy. Adding a channel = adding an arm here + a strategy impl — the
/// MAIL-M16/B seam.
async fn dispatch_channel(
    tx: &mut sqlx::PgConnection,
    ctx: &PostContext,
    channel: NotificationChannel,
    recipients: &[PostRecipient],
) -> Result<ChannelMint, MailError> {
    match channel {
        NotificationChannel::Inbox => inbox_strategy(tx, ctx, recipients).await,
        NotificationChannel::Email => email_strategy(tx, ctx, recipients).await,
        NotificationChannel::Sms => sms_strategy(tx, ctx, recipients).await,
    }
}

/// `_notify_thread_by_inbox`: `mail.notification(status='sent')` instantly — the
/// inbox channel has no MTA hop, delivery IS the row (MAIL-M3 `instant_delivered`).
async fn inbox_strategy(
    tx: &mut sqlx::PgConnection,
    ctx: &PostContext,
    recipients: &[PostRecipient],
) -> Result<ChannelMint, MailError> {
    let mut notifications = Vec::new();
    for r in recipients {
        let id = Uuid::new_v4();
        MessagePipelineRepository::insert_mail_notification(
            tx,
            &NewMailNotificationRow {
                id,
                mail_message_id: ctx.message_id,
                res_partner_id: r.res_partner_id,
                notification_type: "inbox",
                notification_status: "sent",
                mail_mail_id_int: None,
            },
        )
        .await?;
        notifications.push(MintedNotification {
            id,
            res_partner_id: r.res_partner_id,
            notification_type: NotificationChannel::Inbox,
            notification_status: "sent".into(),
        });
    }
    Ok(ChannelMint { notifications, mail_id: None, sms: Vec::new() })
}

/// `_notify_thread_by_email`: one `mail_notification(status='ready')` per recipient
/// plus ONE `mail` queue row carrying the whole recipient list — the per-recipient
/// status lives on the notification rows, not duplicated on the mail (§2.1).
async fn email_strategy(
    tx: &mut sqlx::PgConnection,
    ctx: &PostContext,
    recipients: &[PostRecipient],
) -> Result<ChannelMint, MailError> {
    let mut addresses = Vec::new();
    for r in recipients {
        let Some(email) = r.email.as_deref() else {
            return Err(MailError::Invalid(format!(
                "email-channel recipient {:?} has no email address",
                r.res_partner_id
            )));
        };
        addresses.push(email.to_string());
    }
    if addresses.is_empty() {
        return Ok(ChannelMint { notifications: Vec::new(), mail_id: None, sms: Vec::new() });
    }

    let mail_id = Uuid::new_v4();
    MessagePipelineRepository::insert_mail(
        tx,
        &NewMailQueueRow {
            id: mail_id,
            mail_message_id: ctx.message_id,
            email_to: &addresses.join(", "),
            email_cc: None,
            reply_to: ctx.reply_to.as_deref(),
            scheduled_date: None,
        },
    )
    .await?;

    let mut notifications = Vec::new();
    for r in recipients {
        let id = Uuid::new_v4();
        MessagePipelineRepository::insert_mail_notification(
            tx,
            &NewMailNotificationRow {
                id,
                mail_message_id: ctx.message_id,
                res_partner_id: r.res_partner_id,
                notification_type: "email",
                notification_status: "ready",
                mail_mail_id_int: Some(mail_id),
            },
        )
        .await?;
        notifications.push(MintedNotification {
            id,
            res_partner_id: r.res_partner_id,
            notification_type: NotificationChannel::Email,
            notification_status: "ready".into(),
        });
    }
    Ok(ChannelMint { notifications, mail_id: Some(mail_id), sms: Vec::new() })
}

/// `_notify_thread_by_sms` (the SM-M7 fold): per recipient one `sms` row
/// (`state='outgoing'`) + its uuid-correlated `sms_tracker` (TR-SM-11) + the
/// notification the pump will drive. Correlated by uuid, deliberately NO FK —
/// the tracker must outlive the sms row's GC (SM-M21).
async fn sms_strategy(
    tx: &mut sqlx::PgConnection,
    ctx: &PostContext,
    recipients: &[PostRecipient],
) -> Result<ChannelMint, MailError> {
    let mut notifications = Vec::new();
    let mut sms = Vec::new();
    for r in recipients {
        let Some(number) = r.number.as_deref() else {
            return Err(MailError::Invalid(format!(
                "sms-channel recipient {:?} has no number",
                r.res_partner_id
            )));
        };
        let notification_id = Uuid::new_v4();
        MessagePipelineRepository::insert_mail_notification(
            tx,
            &NewMailNotificationRow {
                id: notification_id,
                mail_message_id: ctx.message_id,
                res_partner_id: r.res_partner_id,
                notification_type: "sms",
                notification_status: "ready",
                mail_mail_id_int: None,
            },
        )
        .await?;
        notifications.push(MintedNotification {
            id: notification_id,
            res_partner_id: r.res_partner_id,
            notification_type: NotificationChannel::Sms,
            notification_status: "ready".into(),
        });

        let sms_id = Uuid::new_v4();
        let sms_uuid = Uuid::new_v4().simple().to_string();
        MessagePipelineRepository::insert_sms_with_tracker(
            tx,
            &NewSmsRow {
                id: sms_id,
                uuid: sms_uuid.clone(),
                number,
                body: &ctx.body,
                mail_message_id: Some(ctx.message_id),
                notification_id: Some(notification_id),
            },
        )
        .await?;
        sms.push((sms_id, sms_uuid));
    }
    Ok(ChannelMint { notifications, mail_id: None, sms })
}
