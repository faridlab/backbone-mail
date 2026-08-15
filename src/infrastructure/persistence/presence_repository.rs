//! Repository for presence writes (hand-written; user-owned).
//!
//! Holds the SQL the [`crate::application::service::PresenceWriteService`]
//! orchestrates: the insert-then-update upsert pairs keyed on
//! (user_id, guest_id) IS NOT DISTINCT FROM. Runtime queries — no `.sqlx` cache.

use sqlx::PgConnection;
use uuid::Uuid;

/// Hand-written presence SQL. Services orchestrate; this holds SQL.
pub struct PresenceRepository;

impl PresenceRepository {
    pub fn new() -> Self {
        Self
    }

    /// The liveness write: ensure the identity's row exists, then bump
    /// last_poll + status 'online' (the UPDATE catches the row the INSERT
    /// found already present).
    pub async fn refresh_last_poll(
        conn: &mut PgConnection,
        user_id: Option<Uuid>,
        guest_id: Option<Uuid>,
    ) -> Result<(), sqlx::Error> {
        sqlx::query(r#"
            INSERT INTO messaging.mail_presences (id, user_id, guest_id, last_poll, status)
            VALUES ($1, $2, $3, NOW(), 'online')
            ON CONFLICT DO NOTHING
        "#)
            .bind(Uuid::new_v4())
            .bind(user_id)
            .bind(guest_id)
            .execute(&mut *conn)
            .await?;
        sqlx::query(r#"
            UPDATE messaging.mail_presences SET last_poll = NOW(), status = 'online'
            WHERE user_id IS NOT DISTINCT FROM $1 AND guest_id IS NOT DISTINCT FROM $2
        "#)
            .bind(user_id)
            .bind(guest_id)
            .execute(&mut *conn)
            .await?;
        Ok(())
    }

    /// The manual im-status write: same upsert shape, but the status comes
    /// from the user (validated by the service BEFORE this runs).
    pub async fn set_status(
        conn: &mut PgConnection,
        user_id: Option<Uuid>,
        guest_id: Option<Uuid>,
        status: &str,
    ) -> Result<(), sqlx::Error> {
        sqlx::query(r#"
            INSERT INTO messaging.mail_presences (id, user_id, guest_id, last_poll, status)
            VALUES ($1, $2, $3, NOW(), $4::mail_presence_status)
            ON CONFLICT DO NOTHING
        "#)
            .bind(Uuid::new_v4())
            .bind(user_id)
            .bind(guest_id)
            .bind(status)
            .execute(&mut *conn)
            .await?;
        sqlx::query(r#"
            UPDATE messaging.mail_presences
            SET status = $3::mail_presence_status, last_poll = NOW()
            WHERE user_id IS NOT DISTINCT FROM $1 AND guest_id IS NOT DISTINCT FROM $2
        "#)
            .bind(user_id)
            .bind(guest_id)
            .bind(status)
            .execute(&mut *conn)
            .await?;
        Ok(())
    }
}

impl Default for PresenceRepository {
    fn default() -> Self {
        Self::new()
    }
}
