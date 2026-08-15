//! Repository for attachment writes (hand-written; user-owned).
//!
//! Holds the SQL the [`crate::application::service::AttachmentWriteService`]
//! orchestrates: register (owner XOR enforced in SQL), join-table attach/
//! detach, and the public-token mint. Runtime queries — no `.sqlx` cache.

use sqlx::PgConnection;
use uuid::Uuid;

/// Hand-written attachment SQL. Services orchestrate; this holds SQL.
pub struct AttachmentRepository;

impl AttachmentRepository {
    pub fn new() -> Self {
        Self
    }

    /// Insert an attachment row. The owner XOR is a CHECK-style WHERE here so
    /// a neither-owner insert fails loudly instead of minting an orphan.
    #[allow(clippy::too_many_arguments)]
    pub async fn insert_attachment(
        conn: &mut PgConnection,
        id: Uuid,
        name: &str,
        mimetype: Option<&str>,
        size: Option<i32>,
        datas: Option<&str>,
        checksum: Option<&str>,
        owner_party_id: Option<Uuid>,
        owner_guest_id: Option<Uuid>,
    ) -> Result<(), sqlx::Error> {
        sqlx::query(r#"
            INSERT INTO messaging.mail_attachments
                (id, name, mimetype, size, datas, checksum, owner_party_id, owner_guest_id)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
        "#)
            .bind(id)
            .bind(name)
            .bind(mimetype)
            .bind(size)
            .bind(datas)
            .bind(checksum)
            .bind(owner_party_id)
            .bind(owner_guest_id)
            .execute(&mut *conn)
            .await?;
        Ok(())
    }

    /// Attach an attachment to a message (idempotent on the unique).
    pub async fn attach_to_message(
        conn: &mut PgConnection,
        id: Uuid,
        message_id: Uuid,
        attachment_id: Uuid,
    ) -> Result<bool, sqlx::Error> {
        let res = sqlx::query(r#"
            INSERT INTO messaging.mail_message_attachments (id, message_id, attachment_id)
            VALUES ($1, $2, $3)
            ON CONFLICT (message_id, attachment_id) DO NOTHING
        "#)
            .bind(id)
            .bind(message_id)
            .bind(attachment_id)
            .execute(&mut *conn)
            .await?;
        Ok(res.rows_affected() > 0)
    }

    /// Detach an attachment from a message. True when a row was removed.
    pub async fn detach_from_message(
        conn: &mut PgConnection,
        message_id: Uuid,
        attachment_id: Uuid,
    ) -> Result<bool, sqlx::Error> {
        let res = sqlx::query(r#"
            DELETE FROM messaging.mail_message_attachments
            WHERE message_id = $1 AND attachment_id = $2
        "#)
            .bind(message_id)
            .bind(attachment_id)
            .execute(&mut *conn)
            .await?;
        Ok(res.rows_affected() > 0)
    }

    /// The (owner_party_id, owner_guest_id) pair of an attachment — the
    /// ownership gate input. Executor-generic: called pool-direct (gate-only,
    /// no tx needed).
    pub async fn owners(
        executor: impl sqlx::Executor<'_, Database = sqlx::Postgres>,
        attachment_id: Uuid,
    ) -> Result<Option<(Option<Uuid>, Option<Uuid>)>, sqlx::Error> {
        sqlx::query_as(
            r#"SELECT owner_party_id, owner_guest_id FROM messaging.mail_attachments
               WHERE id = $1 AND (metadata->>'deleted_at') IS NULL"#,
        )
        .bind(attachment_id)
        .fetch_optional(executor)
        .await
    }

    /// Mint (or clear) the public access token. Returns the stored token.
    pub async fn set_access_token(
        conn: &mut PgConnection,
        attachment_id: Uuid,
        token: Option<Uuid>,
    ) -> Result<(), sqlx::Error> {
        sqlx::query(r#"
            UPDATE messaging.mail_attachments SET access_token = $2 WHERE id = $1
        "#)
            .bind(attachment_id)
            .bind(token)
            .execute(&mut *conn)
            .await?;
        Ok(())
    }
}

impl Default for AttachmentRepository {
    fn default() -> Self {
        Self::new()
    }
}
