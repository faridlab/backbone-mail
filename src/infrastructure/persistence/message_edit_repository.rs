//! Repository for message edit + star writes (hand-written; user-owned).
//!
//! Holds the SQL the [`crate::application::service::MessageEditService`]
//! orchestrates: content edits (author-gated), the event-routing channel
//! lookup, and the starred-m2m toggle pair (MAIL-M45 adjunct). Runtime
//! queries — no `.sqlx` cache.

use sqlx::PgConnection;
use uuid::Uuid;

/// Hand-written edit/star SQL. Services orchestrate; this holds SQL.
pub struct MessageEditRepository;

impl MessageEditRepository {
    pub fn new() -> Self {
        Self
    }

    /// The (author_id, author_guest_id) pair of a live message — the author
    /// gate input.
    pub async fn message_authors(
        conn: &mut PgConnection,
        message_id: Uuid,
    ) -> Result<Option<(Option<Uuid>, Option<Uuid>)>, sqlx::Error> {
        sqlx::query_as(
            r#"SELECT author_id, author_guest_id FROM messaging.mail_messages
               WHERE id = $1 AND (metadata->>'deleted_at') IS NULL"#,
        )
        .bind(message_id)
        .fetch_optional(&mut *conn)
        .await
    }

    /// COALESCE-apply subject/body (None = leave unchanged). True when the
    /// row updated.
    pub async fn update_content(
        conn: &mut PgConnection,
        message_id: Uuid,
        subject: Option<&str>,
        body: Option<&str>,
    ) -> Result<bool, sqlx::Error> {
        let res = sqlx::query(r#"
            UPDATE messaging.mail_messages
            SET subject = COALESCE($2, subject),
                body = COALESCE($3, body)
            WHERE id = $1
        "#)
            .bind(message_id)
            .bind(subject)
            .bind(body)
            .execute(&mut *conn)
            .await?;
        Ok(res.rows_affected() > 0)
    }

    /// `model || '_' || res_id` for the message's channel key (None for
    /// channel-less messages — the caller falls back to the message channel).
    pub async fn channel_key(
        conn: &mut PgConnection,
        message_id: Uuid,
    ) -> Result<Option<String>, sqlx::Error> {
        sqlx::query_scalar::<_, Option<String>>(
            r#"SELECT model || '_' || res_id::text FROM messaging.mail_messages WHERE id = $1"#,
        )
        .bind(message_id)
        .fetch_one(&mut *conn)
        .await
    }

    /// Remove the (partner, message) star. True when one was removed.
    pub async fn delete_star(
        conn: &mut PgConnection,
        partner_id: Uuid,
        message_id: Uuid,
    ) -> Result<bool, sqlx::Error> {
        let res = sqlx::query(r#"
            DELETE FROM messaging.mail_message_stars
            WHERE partner_id = $1 AND message_id = $2
        "#)
            .bind(partner_id)
            .bind(message_id)
            .execute(&mut *conn)
            .await?;
        Ok(res.rows_affected() > 0)
    }

    /// Insert the (partner, message) star. True when inserted.
    pub async fn insert_star(
        conn: &mut PgConnection,
        id: Uuid,
        partner_id: Uuid,
        message_id: Uuid,
    ) -> Result<bool, sqlx::Error> {
        let res = sqlx::query(r#"
            INSERT INTO messaging.mail_message_stars (id, partner_id, message_id)
            VALUES ($1, $2, $3)
            ON CONFLICT DO NOTHING
        "#)
            .bind(id)
            .bind(partner_id)
            .bind(message_id)
            .execute(&mut *conn)
            .await?;
        Ok(res.rows_affected() > 0)
    }
}

impl Default for MessageEditRepository {
    fn default() -> Self {
        Self::new()
    }
}
