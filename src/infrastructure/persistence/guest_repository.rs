//! Repository for guest-persona writes (hand-written; user-owned).
//!
//! Holds the SQL the [`crate::application::service::GuestWriteService`]
//! orchestrates: mint, self-rename (last_connection_dt refreshed — the GC
//! liveness pointer), and touch. Runtime queries — no `.sqlx` cache.

use sqlx::{PgConnection, PgPool};
use uuid::Uuid;

/// Hand-written guest SQL. Services orchestrate; this holds SQL.
pub struct GuestRepository;

impl GuestRepository {
    pub fn new() -> Self {
        Self
    }

    /// Insert a fresh guest persona (active; last_connection_dt now).
    pub async fn insert_guest(
        conn: &mut PgConnection,
        guest_id: Uuid,
        name: &str,
    ) -> Result<(), sqlx::Error> {
        sqlx::query(r#"
            INSERT INTO messaging.mail_guests (id, name, last_connection_dt, active)
            VALUES ($1, $2, NOW(), TRUE)
        "#)
            .bind(guest_id)
            .bind(name)
            .execute(&mut *conn)
            .await?;
        Ok(())
    }

    /// Rename a live guest + refresh the liveness pointer. False when the row
    /// is gone or already soft-deleted.
    pub async fn update_name(
        conn: &mut PgConnection,
        guest_id: Uuid,
        new_name: &str,
    ) -> Result<bool, sqlx::Error> {
        let res = sqlx::query(r#"
            UPDATE messaging.mail_guests
            SET name = $2, last_connection_dt = NOW()
            WHERE id = $1 AND active AND (metadata->>'deleted_at') IS NULL
        "#)
            .bind(guest_id)
            .bind(new_name)
            .execute(&mut *conn)
            .await?;
        Ok(res.rows_affected() > 0)
    }

    /// Refresh the liveness pointer only (every guest-visible request path).
    pub async fn touch(pool: &PgPool, guest_id: Uuid) -> Result<(), sqlx::Error> {
        sqlx::query(r#"
            UPDATE messaging.mail_guests SET last_connection_dt = NOW()
            WHERE id = $1 AND active
        "#)
            .bind(guest_id)
            .execute(pool)
            .await?;
        Ok(())
    }
}

impl Default for GuestRepository {
    fn default() -> Self {
        Self::new()
    }
}
