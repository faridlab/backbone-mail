//! Repository for discuss-channel writes (hand-written; user-owned).
//!
//! Holds the SQL the [`crate::application::service::ChannelWriteService`]
//! orchestrates: channel creation with the creator membership injected in the
//! SAME transaction, the 1:1-chat get-or-create dedup (source-verified
//! `_get_or_create_chat`: member-set equality via a NOT EXISTS anti-join, not
//! a name lookup), sub-channel creator-only ops, and message pinning.
//! Runtime queries (no compile-time macros) — hence no `.sqlx` cache.

use sqlx::{PgConnection, Row};
use uuid::Uuid;

use crate::application::service::chatter_acl::MessagingIdentity;

/// The channel fields a create may set (MAIL-M37).
pub struct NewChannelRow<'a> {
    pub id: Uuid,
    pub name: Option<&'a str>,
    pub channel_type: &'a str,
    pub uuid: Option<&'a str>,
    pub default_access_mode: Option<&'a str>,
    pub email_send: bool,
}

/// Hand-written channel SQL. Services orchestrate; this holds SQL.
pub struct ChannelRepository;

impl ChannelRepository {
    pub fn new() -> Self {
        Self
    }

    /// Insert the channel row (soft-delete-aware reads elsewhere; the write is
    /// a plain INSERT — new rows are live by construction).
    pub async fn insert_channel(
        conn: &mut PgConnection,
        row: &NewChannelRow<'_>,
    ) -> Result<(), sqlx::Error> {
        sqlx::query(r#"
            INSERT INTO messaging.discuss_channels
                (id, name, channel_type, uuid, default_access_mode, email_send)
            VALUES ($1, $2, $3::discuss_channel_type, $4, $5, $6)
        "#)
            .bind(row.id)
            .bind(row.name)
            .bind(row.channel_type)
            .bind(row.uuid)
            .bind(row.default_access_mode)
            .bind(row.email_send)
            .execute(&mut *conn)
            .await?;
        Ok(())
    }

    /// Insert the creator's membership on the caller's open transaction — the
    /// M37 rule "create injects the creator as first member" is one tx with
    /// the channel insert, never two writes.
    pub async fn insert_creator_member(
        conn: &mut PgConnection,
        channel_id: Uuid,
        identity: &MessagingIdentity,
    ) -> Result<Uuid, sqlx::Error> {
        let member_id = Uuid::new_v4();
        sqlx::query(r#"
            INSERT INTO messaging.discuss_channel_members
                (id, channel_id, partner_id, guest_id, is_pinned)
            VALUES ($1, $2, $3, $4, TRUE)
        "#)
            .bind(member_id)
            .bind(channel_id)
            .bind(identity.partner_id())
            .bind(match identity {
                MessagingIdentity::Guest { guest_id } => Some(*guest_id),
                _ => None,
            })
            .execute(&mut *conn)
            .await?;
        Ok(member_id)
    }

    /// The 1:1-chat dedup lookup (source-verified `_get_or_create_chat`
    /// :1340-1360): a chat channel matches iff its live partner-member set is
    /// EXACTLY the two given partners — no member outside the set (anti-join)
    /// and no missing member (count). This is deliberately NOT a name lookup:
    /// partner renames would silently fork chats.
    pub async fn find_chat_by_member_set(
        conn: &mut PgConnection,
        partner_ids: &[Uuid],
    ) -> Result<Option<Uuid>, sqlx::Error> {
        let row = sqlx::query(r#"
            SELECT c.id
            FROM messaging.discuss_channels c
            WHERE c.channel_type = 'chat'
              AND (c.metadata->>'deleted_at') IS NULL
              AND NOT EXISTS (
                    SELECT 1 FROM messaging.discuss_channel_members m
                    WHERE m.channel_id = c.id
                      AND (m.metadata->>'deleted_at') IS NULL
                      AND m.partner_id IS DISTINCT FROM NULL
                      AND NOT (m.partner_id = ANY($1))
                  )
              AND (
                    SELECT COUNT(*) FROM messaging.discuss_channel_members m
                    WHERE m.channel_id = c.id
                      AND (m.metadata->>'deleted_at') IS NULL
                      AND m.partner_id IS NOT NULL
                  ) = $2
            ORDER BY c.metadata->>'created_at'
            LIMIT 1
        "#)
            .bind(partner_ids)
            .bind(partner_ids.len() as i64)
            .fetch_optional(&mut *conn)
            .await?;
        Ok(row.map(|r| r.get::<Uuid, _>("id")))
    }

    /// Load one channel's channel_type (guard for immutable-type + moderation
    /// checks). `None` = no such live channel.
    pub async fn channel_type(
        conn: &mut PgConnection,
        channel_id: Uuid,
    ) -> Result<Option<String>, sqlx::Error> {
        sqlx::query_scalar::<_, String>(r#"
            SELECT channel_type::text FROM messaging.discuss_channels
            WHERE id = $1 AND (metadata->>'deleted_at') IS NULL
        "#)
            .bind(channel_id)
            .fetch_optional(&mut *conn)
            .await
    }

    /// Update name/description/email_send (guarded writes — channel_type is
    /// immutable post-create and has no UPDATE path at all).
    pub async fn update_channel_fields(
        conn: &mut PgConnection,
        channel_id: Uuid,
        name: Option<&str>,
        description: Option<&str>,
        email_send: Option<bool>,
    ) -> Result<(), sqlx::Error> {
        sqlx::query(r#"
            UPDATE messaging.discuss_channels
            SET name = COALESCE($2, name),
                description = COALESCE($3, description),
                email_send = COALESCE($4, email_send)
            WHERE id = $1
        "#)
            .bind(channel_id)
            .bind(name)
            .bind(description)
            .bind(email_send)
            .execute(&mut *conn)
            .await?;
        Ok(())
    }

    /// Advance `last_message_id` after a post (channel-list ordering pointer).
    pub async fn set_last_message(
        conn: &mut PgConnection,
        channel_id: Uuid,
        message_id: Uuid,
    ) -> Result<(), sqlx::Error> {
        sqlx::query(r#"
            UPDATE messaging.discuss_channels SET last_message_id = $2
            WHERE id = $1
        "#)
            .bind(channel_id)
            .bind(message_id)
            .execute(&mut *conn)
            .await?;
        Ok(())
    }

    /// Pin/unpin a message (MAIL-M37.1, source-verified shape: pinned_at is a
    /// column ON mail.message). Minimal UPDATE touching only pinned_at.
    ///
    /// PORT DELTA (documented): Odoo's raw UPDATE deliberately skips
    /// write_date; the port's generated audit trigger advances
    /// metadata->updated_at on every UPDATE, and pin writes are no exception.
    /// Recording the delta here beats weakening the audit trigger.
    pub async fn set_message_pinned(
        conn: &mut PgConnection,
        message_id: Uuid,
        pinned: bool,
    ) -> Result<(), sqlx::Error> {
        sqlx::query(r#"
            UPDATE messaging.mail_messages
            SET pinned_at = CASE WHEN $2 THEN NOW() ELSE NULL END
            WHERE id = $1
        "#)
            .bind(message_id)
            .bind(pinned)
            .execute(&mut *conn)
            .await?;
        Ok(())
    }

    /// Soft-delete a channel (archive semantics; members keep their rows).
    pub async fn soft_delete_channel(
        conn: &mut PgConnection,
        channel_id: Uuid,
    ) -> Result<(), sqlx::Error> {
        sqlx::query(r#"
            UPDATE messaging.discuss_channels
            SET metadata = jsonb_set(metadata, '{deleted_at}', to_jsonb(NOW()))
            WHERE id = $1 AND (metadata->>'deleted_at') IS NULL
        "#)
            .bind(channel_id)
            .execute(&mut *conn)
            .await?;
        Ok(())
    }

    // =====================================================================
    // Query side (ChannelQueryService) — pre-gated by the service (MAIL-B1:
    // membership is checked in code before these run).
    // =====================================================================

    /// Live members of a channel: (member_id, partner_id?, guest_id?).
    pub async fn list_members(
        conn: &mut PgConnection,
        channel_id: Uuid,
    ) -> Result<Vec<(Uuid, Option<Uuid>, Option<Uuid>)>, sqlx::Error> {
        let rows = sqlx::query(
            r#"SELECT id, partner_id, guest_id FROM messaging.discuss_channel_members
               WHERE channel_id = $1 AND (metadata->>'deleted_at') IS NULL
               ORDER BY id"#,
        )
        .bind(channel_id)
        .fetch_all(&mut *conn)
        .await?;
        Ok(rows
            .into_iter()
            .map(|r| (r.get("id"), r.get("partner_id"), r.get("guest_id")))
            .collect())
    }

    /// The channel's pinned message ids (MAIL-M37 adjunct shape: pinned_at
    /// lives ON mail.message over the (model='discuss.channel', res_id) edge).
    pub async fn pinned_message_ids(
        conn: &mut PgConnection,
        channel_id: Uuid,
    ) -> Result<Vec<Uuid>, sqlx::Error> {
        sqlx::query_scalar(
            r#"SELECT id FROM messaging.mail_messages
               WHERE model = 'discuss.channel' AND res_id = $1
                 AND pinned_at IS NOT NULL
                 AND (metadata->>'deleted_at') IS NULL
               ORDER BY pinned_at DESC"#,
        )
        .bind(channel_id)
        .fetch_all(&mut *conn)
        .await
    }

    /// Channel search (Odoo's `/discuss/search` — name ILIKE over the
    /// channels the identity may see): their own member channels plus public
    /// channels (joinable), with `is_member` distinguishing them.
    pub async fn search_channels(
        executor: impl sqlx::Executor<'_, Database = sqlx::Postgres>,
        partner_id: Option<Uuid>,
        guest_id: Option<Uuid>,
        term: &str,
        limit: i64,
    ) -> Result<Vec<(Uuid, Option<String>, String, bool)>, sqlx::Error> {
        let rows = sqlx::query(
            r#"
            SELECT c.id, c.name, c.channel_type::text AS channel_type,
                   (m.id IS NOT NULL) AS is_member
            FROM messaging.discuss_channels c
            LEFT JOIN messaging.discuss_channel_members m
              ON m.channel_id = c.id
             AND m.partner_id IS NOT DISTINCT FROM $1
             AND m.guest_id IS NOT DISTINCT FROM $2
             AND (m.metadata->>'deleted_at') IS NULL
            WHERE (c.metadata->>'deleted_at') IS NULL
              AND c.channel_type IN ('channel', 'group')
              AND ($3::text IS NULL OR c.name ILIKE '%' || $3 || '%')
              AND (
                m.id IS NOT NULL
                OR (c.default_access_mode = 'public' AND $1 IS NOT NULL)
              )
            ORDER BY is_member DESC, c.name
            LIMIT $4
            "#,
        )
        .bind(partner_id)
        .bind(guest_id)
        .bind(if term.trim().is_empty() { None } else { Some(term.trim()) })
        .bind(limit)
        .fetch_all(executor)
        .await?;
        Ok(rows
            .into_iter()
            .map(|r| (r.get("id"), r.get("name"), r.get("channel_type"), r.get("is_member")))
            .collect())
    }

    /// Public-channel lookup for the unauthenticated bootstrap: (id, name,
    /// channel_type) when the invitation uuid matches a LIVE channel whose
    /// `default_access_mode` is 'public'. Non-public and soft-deleted
    /// channels are indistinguishable from "no such channel" — the bootstrap
    /// must not leak private channel existence.
    pub async fn find_public_by_uuid(
        executor: impl sqlx::Executor<'_, Database = sqlx::Postgres>,
        uuid: &str,
    ) -> Result<Option<(Uuid, Option<String>, String)>, sqlx::Error> {
        let row = sqlx::query(
            r#"
            SELECT id, name, channel_type::text AS channel_type
            FROM messaging.discuss_channels
            WHERE uuid = $1
              AND default_access_mode = 'public'
              AND (metadata->>'deleted_at') IS NULL
            "#,
        )
        .bind(uuid)
        .fetch_optional(executor)
        .await?;
        Ok(row.map(|r| (r.get("id"), r.get("name"), r.get("channel_type"))))
    }
}

impl Default for ChannelRepository {
    fn default() -> Self {
        Self::new()
    }
}
