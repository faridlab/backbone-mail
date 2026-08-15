//! Repository for scheduled-message writes (hand-written; user-owned).
//!
//! Holds the SQL the [`crate::application::service::ScheduleWriteService`]
//! orchestrates: M8 arm/cancel/claim (notify-later — delete-on-dispatch, NO
//! state column; claim safety from FOR UPDATE SKIP LOCKED per ADR-0020) and
//! M9 arm/cancel/claim (post-later composer values). Runtime queries — no
//! `.sqlx` cache.

use sqlx::{PgConnection, Row};
use uuid::Uuid;

/// One claimed M8 schedule row (the whole dispatch input).
#[derive(Debug, Clone)]
pub struct ClaimedNotifySchedule {
    pub id: Uuid,
    pub mail_message_id: Uuid,
    pub notification_parameters: Option<String>,
}

/// One claimed M9 scheduled message (composer values + target + author).
#[derive(Debug, Clone)]
pub struct ClaimedScheduledMessage {
    pub id: Uuid,
    pub subject: Option<String>,
    pub body: String,
    pub composition_comment_option: Option<String>,
    pub model: String,
    pub res_id: Uuid,
    pub author_party_id: Uuid,
    pub recipient_party_ids: Option<serde_json::Value>,
    pub is_note: bool,
    pub notification_parameters: Option<String>,
    pub send_context: Option<serde_json::Value>,
}

/// Hand-written schedule SQL. Services orchestrate; this holds SQL.
pub struct ScheduleRepository;

impl ScheduleRepository {
    pub fn new() -> Self {
        Self
    }

    /// Arm M8: queue the notify-later row.
    pub async fn arm_notify(
        conn: &mut PgConnection,
        id: Uuid,
        mail_message_id: Uuid,
        notification_parameters: Option<&str>,
        scheduled_datetime: chrono::DateTime<chrono::Utc>,
    ) -> Result<(), sqlx::Error> {
        sqlx::query(r#"
            INSERT INTO messaging.mail_message_schedules
                (id, mail_message_id, notification_parameters, scheduled_datetime)
            VALUES ($1, $2, $3, $4)
        "#)
            .bind(id)
            .bind(mail_message_id)
            .bind(notification_parameters)
            .bind(scheduled_datetime)
            .execute(&mut *conn)
            .await?;
        Ok(())
    }

    /// Cancel M8/M9: delete the row (both models are delete-shaped — no state).
    pub async fn cancel_notify(
        conn: &mut PgConnection,
        id: Uuid,
    ) -> Result<bool, sqlx::Error> {
        let res = sqlx::query(
            r#"DELETE FROM messaging.mail_message_schedules WHERE id = $1"#,
        )
        .bind(id)
        .execute(&mut *conn)
        .await?;
        Ok(res.rows_affected() > 0)
    }

    /// Cancel M9 by id.
    pub async fn cancel_scheduled_message(
        conn: &mut PgConnection,
        id: Uuid,
    ) -> Result<bool, sqlx::Error> {
        let res = sqlx::query(
            r#"DELETE FROM messaging.mail_scheduled_messages WHERE id = $1"#,
        )
        .bind(id)
        .execute(&mut *conn)
        .await?;
        Ok(res.rows_affected() > 0)
    }

    /// Claim due M8 rows: FOR UPDATE SKIP LOCKED in the claiming transaction,
    /// and the dispatch DELETEs them on the same tx — a crashed worker's rows
    /// become claimable again the moment its tx rolls back (no state column,
    /// no cleanup job; the row's EXISTENCE is the pending state).
    pub async fn claim_due_notify(
        conn: &mut PgConnection,
        now: chrono::DateTime<chrono::Utc>,
        limit: i64,
    ) -> Result<Vec<ClaimedNotifySchedule>, sqlx::Error> {
        let rows = sqlx::query(r#"
            SELECT id, mail_message_id, notification_parameters
            FROM messaging.mail_message_schedules
            WHERE scheduled_datetime <= $1
            ORDER BY scheduled_datetime
            LIMIT $2
            FOR UPDATE SKIP LOCKED
        "#)
            .bind(now)
            .bind(limit)
            .fetch_all(&mut *conn)
            .await?;
        Ok(rows
            .into_iter()
            .map(|r| ClaimedNotifySchedule {
                id: r.get("id"),
                mail_message_id: r.get("mail_message_id"),
                notification_parameters: r.get("notification_parameters"),
            })
            .collect())
    }

    /// Delete a dispatched M8 row (on the claiming tx — dispatch + removal
    /// are atomic).
    pub async fn delete_notify(
        conn: &mut PgConnection,
        id: Uuid,
    ) -> Result<(), sqlx::Error> {
        sqlx::query(r#"DELETE FROM messaging.mail_message_schedules WHERE id = $1"#)
            .bind(id)
            .execute(&mut *conn)
            .await?;
        Ok(())
    }

    /// Arm M9: queue the composer values.
    #[allow(clippy::too_many_arguments)]
    pub async fn arm_scheduled_message(
        conn: &mut PgConnection,
        id: Uuid,
        subject: Option<&str>,
        body: &str,
        scheduled_date: chrono::DateTime<chrono::Utc>,
        composition_comment_option: Option<&str>,
        model: &str,
        res_id: Uuid,
        author_party_id: Uuid,
        recipient_party_ids: Option<&serde_json::Value>,
        is_note: bool,
        notification_parameters: Option<&str>,
        send_context: Option<&serde_json::Value>,
    ) -> Result<(), sqlx::Error> {
        sqlx::query(r#"
            INSERT INTO messaging.mail_scheduled_messages
                (id, subject, body, scheduled_date, composition_comment_option,
                 model, res_id, author_party_id, recipient_party_ids, is_note,
                 notification_parameters, send_context)
            VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12)
        "#)
            .bind(id)
            .bind(subject)
            .bind(body)
            .bind(scheduled_date)
            .bind(composition_comment_option)
            .bind(model)
            .bind(res_id)
            .bind(author_party_id)
            .bind(recipient_party_ids)
            .bind(is_note)
            .bind(notification_parameters)
            .bind(send_context)
            .execute(&mut *conn)
            .await?;
        Ok(())
    }

    /// Claim due M9 rows (SKIP LOCKED; deletion happens post-dispatch on the
    /// same tx).
    pub async fn claim_due_scheduled_messages(
        conn: &mut PgConnection,
        now: chrono::DateTime<chrono::Utc>,
        limit: i64,
    ) -> Result<Vec<ClaimedScheduledMessage>, sqlx::Error> {
        let rows = sqlx::query(r#"
            SELECT id, subject, body, composition_comment_option, model, res_id,
                   author_party_id, recipient_party_ids, is_note,
                   notification_parameters, send_context
            FROM messaging.mail_scheduled_messages
            WHERE scheduled_date <= $1
            ORDER BY scheduled_date
            LIMIT $2
            FOR UPDATE SKIP LOCKED
        "#)
            .bind(now)
            .bind(limit)
            .fetch_all(&mut *conn)
            .await?;
        Ok(rows
            .into_iter()
            .map(|r| ClaimedScheduledMessage {
                id: r.get("id"),
                subject: r.get("subject"),
                body: r.get("body"),
                composition_comment_option: r.get("composition_comment_option"),
                model: r.get("model"),
                res_id: r.get("res_id"),
                author_party_id: r.get("author_party_id"),
                recipient_party_ids: r.get("recipient_party_ids"),
                is_note: r.get("is_note"),
                notification_parameters: r.get("notification_parameters"),
                send_context: r.get("send_context"),
            })
            .collect())
    }

    /// Delete a dispatched (or failed-permanently) M9 row.
    pub async fn delete_scheduled_message(
        conn: &mut PgConnection,
        id: Uuid,
    ) -> Result<(), sqlx::Error> {
        sqlx::query(r#"DELETE FROM messaging.mail_scheduled_messages WHERE id = $1"#)
            .bind(id)
            .execute(&mut *conn)
            .await?;
        Ok(())
    }

    /// The (model, res_id) thread edge of a message — the M8 dispatch input.
    pub async fn thread_of_message(
        conn: &mut PgConnection,
        message_id: Uuid,
    ) -> Result<Option<(Option<String>, Option<Uuid>)>, sqlx::Error> {
        sqlx::query_as(
            r#"SELECT model, res_id FROM messaging.mail_messages WHERE id = $1"#,
        )
        .bind(message_id)
        .fetch_optional(&mut *conn)
        .await
    }

    /// The latest message on the author's own wall — the notification
    /// carrier lookup for the M9 failure path.
    pub async fn latest_author_wall_message(
        conn: &mut PgConnection,
        author_party_id: Uuid,
    ) -> Result<Option<Uuid>, sqlx::Error> {
        sqlx::query_scalar(
            r#"SELECT id FROM messaging.mail_messages
               WHERE model = 'res.partner' AND res_id = $1 AND author_id = $1
               ORDER BY date DESC LIMIT 1"#,
        )
        .bind(author_party_id)
        .fetch_optional(&mut *conn)
        .await
    }
}

impl Default for ScheduleRepository {
    fn default() -> Self {
        Self::new()
    }
}
