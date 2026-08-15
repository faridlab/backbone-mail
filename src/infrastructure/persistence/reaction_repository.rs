//! Repository for message-reaction writes (hand-written; user-owned).
//!
//! Holds the SQL the [`crate::application::service::ReactionWriteService`]
//! orchestrates: the toggle's delete-then-insert pair and the thread lookup
//! that addresses the bus event. Runtime queries — no `.sqlx` cache.

use sqlx::PgConnection;
use uuid::Uuid;

/// Hand-written reaction SQL. Services orchestrate; this holds SQL.
pub struct ReactionRepository;

impl ReactionRepository {
    pub fn new() -> Self {
        Self
    }

    /// The (model, res_id) thread edge of a live message, for event routing.
    pub async fn thread_of_message(
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

    /// Remove the (message, identity, content) reaction row. True when one
    /// was removed (the toggle's "off" arm).
    pub async fn delete_reaction(
        conn: &mut PgConnection,
        message_id: Uuid,
        partner_id: Option<Uuid>,
        guest_id: Option<Uuid>,
        content: &str,
    ) -> Result<bool, sqlx::Error> {
        let res = sqlx::query(r#"
            DELETE FROM messaging.mail_message_reactions
            WHERE message_id = $1
              AND partner_id IS NOT DISTINCT FROM $2
              AND guest_id IS NOT DISTINCT FROM $3
              AND content = $4
        "#)
            .bind(message_id)
            .bind(partner_id)
            .bind(guest_id)
            .bind(content)
            .execute(&mut *conn)
            .await?;
        Ok(res.rows_affected() > 0)
    }

    /// Insert the reaction row (the toggle's "on" arm). True when inserted.
    pub async fn insert_reaction(
        conn: &mut PgConnection,
        id: Uuid,
        message_id: Uuid,
        partner_id: Option<Uuid>,
        guest_id: Option<Uuid>,
        content: &str,
    ) -> Result<bool, sqlx::Error> {
        let res = sqlx::query(r#"
            INSERT INTO messaging.mail_message_reactions (id, message_id, partner_id, guest_id, content)
            VALUES ($1, $2, $3, $4, $5)
            ON CONFLICT DO NOTHING
        "#)
            .bind(id)
            .bind(message_id)
            .bind(partner_id)
            .bind(guest_id)
            .bind(content)
            .execute(&mut *conn)
            .await?;
        Ok(res.rows_affected() > 0)
    }
}

impl Default for ReactionRepository {
    fn default() -> Self {
        Self::new()
    }
}
