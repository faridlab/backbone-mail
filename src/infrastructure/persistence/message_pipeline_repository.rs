//! Repository for the message_post pipeline (hand-written; user-owned).
//!
//! Holds the SQL the [`crate::application::service::MessageWriteService`] orchestrates:
//! minting `mail_messages`, subtype resolution, the per-channel notification/mail/sms
//! fan-out inserts, and the monotonic notification-status advance (the tracker pump's
//! sink, SM-M1). Runtime queries (no compile-time macros) — hence no `.sqlx` cache.
//!
//! NOTE: this file deliberately does NOT import the generated per-entity structs —
//! at authoring time the generated `domain/entity/mod.rs` is still the skeleton and
//! does not re-export the 21 messaging entities. The row structs below mirror the
//! committed migration shapes directly, which also keeps this file immune to
//! entity-regen churn.

use sqlx::{PgConnection, Row};
use uuid::Uuid;

use crate::application::service::message_write_service::NotificationChannel;

/// One recipient's notification as minted by the notify pump.
#[derive(Debug, Clone)]
pub struct MintedNotification {
    pub id: Uuid,
    pub res_partner_id: Option<Uuid>,
    pub notification_type: NotificationChannel,
    pub notification_status: String,
}

/// The exact `mail_messages` row a post writes.
pub struct NewMailMessageRow<'a> {
    pub id: Uuid,
    pub subject: Option<&'a str>,
    pub body: &'a str,
    pub message_type: &'a str,
    pub subtype_id: Option<Uuid>,
    pub is_internal: bool,
    pub author_id: Option<Uuid>,
    pub author_guest_id: Option<Uuid>,
    pub email_from: Option<&'a str>,
    pub message_id: Option<&'a str>,
    pub reply_to: Option<&'a str>,
    pub model: Option<&'a str>,
    pub res_id: Option<Uuid>,
    pub record_name: Option<&'a str>,
}

/// The exact `mail_notifications` row the pump writes per recipient/channel.
pub struct NewMailNotificationRow<'a> {
    pub id: Uuid,
    pub mail_message_id: Uuid,
    pub res_partner_id: Option<Uuid>,
    pub notification_type: &'a str,
    pub notification_status: &'a str,
    pub mail_mail_id_int: Option<Uuid>,
}

/// The exact `mails` row the email channel enqueues (MAIL-M2).
pub struct NewMailQueueRow<'a> {
    pub id: Uuid,
    pub mail_message_id: Uuid,
    pub email_to: &'a str,
    pub email_cc: Option<&'a str>,
    pub reply_to: Option<&'a str>,
    pub scheduled_date: Option<chrono::DateTime<chrono::Utc>>,
}

/// The `sms` + `sms_tracker` pair the sms channel mints (SM-M1/SM-M21: correlated
/// by uuid, deliberately NO FK — the tracker must outlive the sms row's GC).
pub struct NewSmsRow<'a> {
    pub id: Uuid,
    pub uuid: String,
    pub number: &'a str,
    pub body: &'a str,
    pub mail_message_id: Option<Uuid>,
    /// The per-recipient notification this sms will drive (sms channel, 1:many by uuid).
    pub notification_id: Option<Uuid>,
}

/// Hand-written messaging SQL. Services orchestrate the unit of work; this holds SQL.
pub struct MessagePipelineRepository;

impl MessagePipelineRepository {
    pub fn new() -> Self {
        Self
    }
}

impl Default for MessagePipelineRepository {
    fn default() -> Self {
        Self::new()
    }
}

impl MessagePipelineRepository {
    /// Resolve the subtype for a post: explicit id wins, else by name, else the
    /// module default (`default = true`). `Ok(None)` = no subtype (a bare note).
    pub async fn resolve_subtype(
        conn: &mut PgConnection,
        subtype_id: Option<Uuid>,
        subtype_name: Option<&str>,
    ) -> Result<Option<Uuid>, sqlx::Error> {
        if let Some(id) = subtype_id {
            return Ok(sqlx::query_scalar::<_, Uuid>(
                r#"SELECT id FROM messaging.mail_message_subtypes WHERE id = $1"#,
            )
            .bind(id)
            .fetch_optional(&mut *conn)
            .await?);
        }
        if let Some(name) = subtype_name {
            return Ok(sqlx::query_scalar::<_, Uuid>(
                r#"SELECT id FROM messaging.mail_message_subtypes
                   WHERE name = $1 AND (metadata->>'deleted_at') IS NULL
                   ORDER BY id LIMIT 1"#,
            )
            .bind(name)
            .fetch_optional(&mut *conn)
            .await?);
        }
        Ok(sqlx::query_scalar::<_, Uuid>(
            r#"SELECT id FROM messaging.mail_message_subtypes
               WHERE "default" = TRUE AND (metadata->>'deleted_at') IS NULL
               ORDER BY id LIMIT 1"#,
        )
        .fetch_optional(&mut *conn)
        .await?)
    }

    /// Mint the `mail_messages` row (TR-MAIL-1 `_message_create`).
    pub async fn insert_mail_message(
        conn: &mut PgConnection,
        m: &NewMailMessageRow<'_>,
    ) -> Result<(), sqlx::Error> {
        sqlx::query(
            r#"INSERT INTO messaging.mail_messages
                 (id, subject, body, message_type, subtype_id, is_internal, author_id,
                  author_guest_id, email_from, message_id, reply_to, model, res_id, record_name)
               VALUES ($1,$2,$3,$4::mail_message_type,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14)"#,
        )
        .bind(m.id)
        .bind(m.subject)
        .bind(m.body)
        .bind(m.message_type)
        .bind(m.subtype_id)
        .bind(m.is_internal)
        .bind(m.author_id)
        .bind(m.author_guest_id)
        .bind(m.email_from)
        .bind(m.message_id)
        .bind(m.reply_to)
        .bind(m.model)
        .bind(m.res_id)
        .bind(m.record_name)
        .execute(&mut *conn)
        .await?;
        Ok(())
    }

    /// Mint one per-recipient `mail_notifications` row (TR-MAIL-2). The status is
    /// channel-decided by the caller: inbox mints `'sent'` instantly (instant
    /// delivered — no MTA hop), email mints `'ready'`, sms mints `'ready'` (the
    /// mirror of the sms row's `'outgoing'`, per SMS_STATE_TO_NOTIFICATION_STATUS).
    pub async fn insert_mail_notification(
        conn: &mut PgConnection,
        n: &NewMailNotificationRow<'_>,
    ) -> Result<Uuid, sqlx::Error> {
        sqlx::query_scalar::<_, Uuid>(
            r#"INSERT INTO messaging.mail_notifications
                 (id, mail_message_id, res_partner_id, notification_type, notification_status,
                  mail_mail_id_int)
               VALUES ($1,$2,$3,$4::mail_notification_type,$5::mail_notification_status,$6)
               RETURNING id"#,
        )
        .bind(n.id)
        .bind(n.mail_message_id)
        .bind(n.res_partner_id)
        .bind(n.notification_type)
        .bind(n.notification_status)
        .bind(n.mail_mail_id_int)
        .fetch_one(&mut *conn)
        .await
    }

    /// Enqueue the `mails` send-queue row for the email channel (MAIL-M2). ONE row
    /// carries the whole recipient list (comma-joined) — the per-recipient status
    /// lives on the notification rows, not duplicated on the mail.
    pub async fn insert_mail(
        conn: &mut PgConnection,
        m: &NewMailQueueRow<'_>,
    ) -> Result<Uuid, sqlx::Error> {
        sqlx::query_scalar::<_, Uuid>(
            r#"INSERT INTO messaging.mails
                 (id, mail_message_id, state, email_to, email_cc, reply_to, scheduled_date)
               VALUES ($1,$2,'outgoing'::mail_state,$3,$4,$5,$6)
               RETURNING id"#,
        )
        .bind(m.id)
        .bind(m.mail_message_id)
        .bind(m.email_to)
        .bind(m.email_cc)
        .bind(m.reply_to)
        .bind(m.scheduled_date)
        .fetch_one(&mut *conn)
        .await
    }

    /// Mint the `sms` row + its uuid-correlated `sms_tracker` in one statement pair
    /// (SM-M1/SM-M21; TR-SM-11's inline tracker). The tracker starts `'process'`
    /// (its schema default) — the pump's ignore-table makes that a no-op until a
    /// real state lands on it.
    pub async fn insert_sms_with_tracker(
        conn: &mut PgConnection,
        s: &NewSmsRow<'_>,
    ) -> Result<(), sqlx::Error> {
        sqlx::query(
            r#"INSERT INTO messaging.sms (id, uuid, number, body, state, mail_message_id)
               VALUES ($1,$2,$3,$4,'outgoing'::sms_state,$5)"#,
        )
        .bind(s.id)
        .bind(&s.uuid)
        .bind(s.number)
        .bind(s.body)
        .bind(s.mail_message_id)
        .execute(&mut *conn)
        .await?;

        sqlx::query(
            r#"INSERT INTO messaging.sms_trackers (sms_uuid, message_id, notification_id, state)
               VALUES ($1,$2,$3,'process'::mail_notification_status)
               ON CONFLICT (sms_uuid) DO NOTHING"#,
        )
        .bind(&s.uuid)
        .bind(s.mail_message_id)
        .bind(s.notification_id)
        .execute(&mut *conn)
        .await?;
        Ok(())
    }

    /// Advance a notification's status through the monotonic lattice — the tracker
    /// pump's write (TR-SM-15), and the port of `notifications_statuses_to_ignore`
    /// as a service-level rank guard that MIRRORS the SM-B6 DB trigger. `Ok(None)` =
    /// the advance was a regression/equal no-op (a replayed webhook, an
    /// out-of-order receipt), exactly as Odoo's filter table intended.
    ///
    /// Ranks: ready=0 < process=1 < pending=2 < sent=3; the terminal statuses
    /// (bounce/exception/canceled) are 100 — once terminal, only equal-rank writes
    /// pass. Both sides rank the same through `messaging_status_rank` semantics.
    #[allow(clippy::too_many_arguments)]
    pub async fn advance_notification_status(
        conn: &mut PgConnection,
        notification_id: Uuid,
        target: &str,
        failure_type: Option<&str>,
        failure_reason: Option<&str>,
    ) -> Result<Option<String>, sqlx::Error> {
        sqlx::query_scalar::<_, String>(
            r#"UPDATE messaging.mail_notifications AS n
               SET notification_status = $2::mail_notification_status,
                   failure_type = COALESCE($3::notification_failure_type, n.failure_type),
                   failure_reason = COALESCE($4, n.failure_reason)
               WHERE n.id = $1
                 AND messaging.messaging_status_rank($2)
                     > messaging.messaging_status_rank(n.notification_status::text)
               RETURNING n.notification_status::text"#,
        )
        .bind(notification_id)
        .bind(target)
        .bind(failure_type)
        .bind(failure_reason)
        .fetch_optional(&mut *conn)
        .await
    }

    /// Find the notification a tracker drives, by the sms uuid correlation key.
    pub async fn find_notification_id_by_sms_uuid(
        conn: &mut PgConnection,
        sms_uuid: &str,
    ) -> Result<Option<Uuid>, sqlx::Error> {
        // Option<Option<Uuid>>: outer = no tracker row, inner = tracker row whose
        // notification link is NULL (a composer-sent sms with no message — the
        // common bulk case). A bare Uuid scalar would CRASH the drainer on that
        // row (UnexpectedNullError), so flatten here.
        Ok(sqlx::query_scalar::<_, Option<Uuid>>(
            r#"SELECT notification_id FROM messaging.sms_trackers WHERE sms_uuid = $1"#,
        )
        .bind(sms_uuid)
        .fetch_optional(&mut *conn)
        .await?
        .flatten())
    }

    /// Read a notification's current status (for staging NotificationStatusChanged
    /// with the right before/after shape).
    pub async fn notification_status(
        conn: &mut PgConnection,
        notification_id: Uuid,
    ) -> Result<Option<(Option<Uuid>, String)>, sqlx::Error> {
        let row = sqlx::query(
            r#"SELECT res_partner_id, notification_status::text AS status
               FROM messaging.mail_notifications WHERE id = $1"#,
        )
        .bind(notification_id)
        .fetch_optional(&mut *conn)
        .await?;
        Ok(row.map(|r| (r.get::<Option<Uuid>, _>("res_partner_id"), r.get::<String, _>("status"))))
    }
}
