//! Repository for chatter/message READ paths (hand-written; user-owned).
//!
//! Holds the SQL the [`crate::application::service::MessageQueryService`]
//! orchestrates — the `_message_fetch` port's four fetch modes plus counters.
//! ALL rows returned here are pre-gated by the service (MAIL-B1 procedural
//! walk happens BEFORE the query runs — this repository never applies an
//! ir.rule-shaped WHERE itself; the gate is code, not a clause). Runtime
//! queries — no `.sqlx` cache.

use sqlx::{PgConnection, Row};
use uuid::Uuid;

/// One fetched message (the wire DTO the SSE-side clients consume).
#[derive(Debug, Clone, serde::Serialize)]
pub struct FetchedMessage {
    pub id: Uuid,
    pub subject: Option<String>,
    pub body: String,
    pub message_type: String,
    pub author_id: Option<Uuid>,
    pub author_guest_id: Option<Uuid>,
    pub email_from: Option<String>,
    pub model: Option<String>,
    pub res_id: Option<Uuid>,
    pub record_name: Option<String>,
    pub is_internal: bool,
    pub pinned_at: Option<chrono::DateTime<chrono::Utc>>,
    /// The attachment ids joined through mail_message_attachments.
    pub attachment_ids: Vec<Uuid>,
}

/// Hand-written chatter read SQL. Services orchestrate; this holds SQL.
pub struct ChatterRepository;

const MESSAGE_SELECT: &str = r#"
    SELECT m.id, m.subject, m.body, m.message_type::text AS message_type,
           m.author_id, m.author_guest_id, m.email_from,
           m.model, m.res_id, m.record_name, m.is_internal, m.pinned_at,
           COALESCE(
               (SELECT jsonb_agg(a.attachment_id ORDER BY a.attachment_id)
                FROM messaging.mail_message_attachments a
                WHERE a.message_id = m.id), '[]'
           ) AS attachment_ids
    FROM messaging.mail_messages m
    WHERE (m.metadata->>'deleted_at') IS NULL
"#;

fn map_fetched(row: sqlx::postgres::PgRow) -> Result<FetchedMessage, sqlx::Error> {
    let raw: serde_json::Value = row.try_get("attachment_ids")?;
    Ok(FetchedMessage {
        id: row.try_get("id")?,
        subject: row.try_get("subject")?,
        body: row.try_get("body")?,
        message_type: row.try_get("message_type")?,
        author_id: row.try_get("author_id")?,
        author_guest_id: row.try_get("author_guest_id")?,
        email_from: row.try_get("email_from")?,
        model: row.try_get("model")?,
        res_id: row.try_get("res_id")?,
        record_name: row.try_get("record_name")?,
        is_internal: row.try_get("is_internal")?,
        pinned_at: row.try_get("pinned_at")?,
        attachment_ids: raw
            .as_array()
            .map(|a| a.iter().filter_map(|v| v.as_str().and_then(|s| Uuid::parse_str(s).ok())).collect())
            .unwrap_or_default(),
    })
}

impl ChatterRepository {
    pub fn new() -> Self {
        Self
    }

    /// Fetch a chatter thread's messages, newest-last (created_at then id —
    /// uuids have no natural order, so id is the deterministic tiebreaker).
    /// `after_id` resumes from a cursor when present.
    pub async fn thread_messages(
        conn: &mut PgConnection,
        model: &str,
        res_id: Uuid,
        after_id: Option<Uuid>,
        limit: i64,
    ) -> Result<Vec<FetchedMessage>, sqlx::Error> {
        let rows = sqlx::query(&format!(
            r#"{MESSAGE_SELECT}
                 AND m.model = $1 AND m.res_id = $2
                 AND ($3::uuid IS NULL
                      OR (m.date, m.id) >
                         (SELECT x.date, x.id
                          FROM messaging.mail_messages x WHERE x.id = $3))
                 ORDER BY m.date, m.id
                 LIMIT $4"#
        ))
        .bind(model)
        .bind(res_id)
        .bind(after_id)
        .bind(limit)
        .fetch_all(&mut *conn)
        .await?;
        rows.into_iter().map(map_fetched).collect()
    }

    /// Fetch a discuss channel's messages (model='discuss.channel' under the
    /// hood — same edge, kept explicit for the channel fetch mode).
    pub async fn channel_messages(
        conn: &mut PgConnection,
        channel_id: Uuid,
        after_id: Option<Uuid>,
        limit: i64,
    ) -> Result<Vec<FetchedMessage>, sqlx::Error> {
        Self::thread_messages(conn, "discuss.channel", channel_id, after_id, limit).await
    }

    /// Fetch the partner's INBOX: messages linked through notifications of
    /// type 'inbox' (the inbox view — Odoo's `message_fetch` with
    /// `default_model=res.partner`).
    pub async fn inbox_messages(
        conn: &mut PgConnection,
        partner_id: Uuid,
        limit: i64,
    ) -> Result<Vec<FetchedMessage>, sqlx::Error> {
        let rows = sqlx::query(&format!(
            r#"{MESSAGE_SELECT}
                 AND m.id IN (
                     SELECT n.mail_message_id FROM messaging.mail_notifications n
                     WHERE n.res_partner_id = $1
                       AND n.notification_type = 'inbox'
                       AND (n.metadata->>'deleted_at') IS NULL
                 )
                 ORDER BY m.date DESC, m.id DESC
                 LIMIT $2"#
        ))
        .bind(partner_id)
        .bind(limit)
        .fetch_all(&mut *conn)
        .await?;
        rows.into_iter().map(map_fetched).collect()
    }

    /// Fetch the partner's STARRED messages (the starred m2m materialization,
    /// MAIL-M45 adjunct).
    pub async fn starred_messages(
        conn: &mut PgConnection,
        partner_id: Uuid,
        limit: i64,
    ) -> Result<Vec<FetchedMessage>, sqlx::Error> {
        let rows = sqlx::query(&format!(
            r#"{MESSAGE_SELECT}
                 AND m.id IN (
                     SELECT s.message_id FROM messaging.mail_message_stars s
                     WHERE s.partner_id = $1
                 )
                 ORDER BY m.date DESC, m.id DESC
                 LIMIT $2"#
        ))
        .bind(partner_id)
        .bind(limit)
        .fetch_all(&mut *conn)
        .await?;
        rows.into_iter().map(map_fetched).collect()
    }

    /// The inbox UNREAD counter: notifications of type 'inbox' still in
    /// status 'ready' (the pre-'sent' state — the Systray badge input).
    pub async fn inbox_unread_count(
        conn: &mut PgConnection,
        partner_id: Uuid,
    ) -> Result<i64, sqlx::Error> {
        sqlx::query_scalar(
            r#"SELECT COUNT(*) FROM messaging.mail_notifications
               WHERE res_partner_id = $1
                 AND notification_type = 'inbox'
                 AND notification_status = 'ready'
                 AND (metadata->>'deleted_at') IS NULL"#,
        )
        .bind(partner_id)
        .fetch_one(&mut *conn)
        .await
    }

    /// Per-channel unread counters for a member: channels where the member's
    /// `message_unread_counter` is > 0 (the counter the write path resets on
    /// mark_as_read). Returns (channel_id, count).
    pub async fn channel_unread_counts(
        conn: &mut PgConnection,
        partner_id: Uuid,
    ) -> Result<Vec<(Uuid, i64)>, sqlx::Error> {
        let rows = sqlx::query(
            r#"SELECT channel_id, message_unread_counter
               FROM messaging.discuss_channel_members
               WHERE partner_id = $1
                 AND (metadata->>'deleted_at') IS NULL
                 AND message_unread_counter > 0"#,
        )
        .bind(partner_id)
        .fetch_all(&mut *conn)
        .await?;
        Ok(rows.into_iter().map(|r| (r.get("channel_id"), r.get("message_unread_counter"))).collect())
    }

    /// The (model, res_id) edge of one message — the MAIL-B1 walk input for
    /// per-message gating (reactions/stars read-back).
    pub async fn message_thread(
        conn: &mut PgConnection,
        message_id: Uuid,
    ) -> Result<Option<(Option<String>, Option<Uuid>)>, sqlx::Error> {
        sqlx::query_as(
            r#"SELECT model, res_id FROM messaging.mail_messages
               WHERE id = $1 AND (metadata->>'deleted_at') IS NULL"#,
        )
        .bind(message_id)
        .fetch_optional(&mut *conn)
        .await
    }

    /// Distinct partner authors who posted on a thread — the second half of
    /// the recipient suggestion set (Odoo's `_message_get_suggested_recipients`
    /// = followers + people already in the conversation).
    pub async fn thread_authors(
        conn: &mut PgConnection,
        model: &str,
        res_id: Uuid,
    ) -> Result<Vec<Uuid>, sqlx::Error> {
        sqlx::query_scalar(
            r#"SELECT DISTINCT author_id FROM messaging.mail_messages
               WHERE model = $1 AND res_id = $2
                 AND author_id IS NOT NULL
                 AND (metadata->>'deleted_at') IS NULL"#,
        )
        .bind(model)
        .bind(res_id)
        .fetch_all(&mut *conn)
        .await
    }
}

impl Default for ChatterRepository {
    fn default() -> Self {
        Self::new()
    }
}
