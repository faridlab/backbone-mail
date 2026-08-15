//! Repository for discuss-channel-member writes (hand-written; user-owned).
//!
//! Holds the SQL the [`crate::application::service::ChannelMemberWriteService`]
//! orchestrates: the SKIP LOCKED seen-advance (the unread-race proof), the
//! new-message separator, join/leave, mute (with the '+infinity'
//! until-unmuted semantics), per-channel notification overrides, and sidebar
//! pin. Runtime queries (no compile-time macros) — hence no `.sqlx` cache.

use sqlx::PgConnection;
use uuid::Uuid;

use crate::application::service::chatter_acl::MessagingIdentity;

/// Identify a member row by (channel, identity) — the two partial uniques of
/// G-MAIL-8 make exactly one row live per pair.
pub struct MemberKey<'a> {
    pub channel_id: Uuid,
    pub identity: &'a MessagingIdentity,
}

/// Bind the partner/guest discriminator for a query. Returns (partner, guest).
fn pg(identity: &MessagingIdentity) -> (Option<Uuid>, Option<Uuid>) {
    match identity {
        MessagingIdentity::User { partner_id } => (Some(*partner_id), None),
        MessagingIdentity::Guest { guest_id } => (None, Some(*guest_id)),
    }
}

/// Hand-written member SQL. Services orchestrate; this holds SQL.
pub struct ChannelMemberRepository;

impl ChannelMemberRepository {
    pub fn new() -> Self {
        Self
    }

    /// The live member row id for (channel, identity), or None.
    /// `FOR NO KEY UPDATE SKIP LOCKED` variant is the claim probe used by
    /// mark_as_read — a racing transaction simply doesn't see the row and
    /// treats it as not-currently-claimable (skips), which is exactly the
    /// Odoo semantics: last-writer-wins per message, no double-counter-write.
    pub async fn member_id(
        conn: &mut PgConnection,
        key: &MemberKey<'_>,
        lock: bool,
    ) -> Result<Option<Uuid>, sqlx::Error> {
        let (partner, guest) = pg(key.identity);
        let sql = format!(
            r#"
            SELECT id FROM messaging.discuss_channel_members
            WHERE channel_id = $1
              AND partner_id IS NOT DISTINCT FROM $2
              AND guest_id IS NOT DISTINCT FROM $3
              AND (metadata->>'deleted_at') IS NULL
            {}
            "#,
            if lock { "FOR NO KEY UPDATE SKIP LOCKED" } else { "" }
        );
        sqlx::query_scalar::<_, Uuid>(&sql)
            .bind(key.channel_id)
            .bind(partner)
            .bind(guest)
            .fetch_optional(&mut *conn)
            .await
    }

    /// Advance the seen pointer MONOTONICALLY (mark_as_read): the write only
    /// lands when the target message is strictly newer than the current seen
    /// message (created_at ordering — uuid PKs have no order; documented
    /// int→uuid adaptation). Counters reset on the same write, `last_seen_dt`
    /// refreshed.
    pub async fn advance_seen(
        conn: &mut PgConnection,
        member_id: Uuid,
        message_id: Uuid,
    ) -> Result<bool, sqlx::Error> {
        let res = sqlx::query(r#"
            UPDATE messaging.discuss_channel_members m
            SET seen_message_id = $2,
                message_unread_counter = 0,
                unread_counter = 0,
                last_seen_dt = NOW()
            WHERE m.id = $1
              AND (
                    m.seen_message_id IS NULL
                    OR (SELECT n.date FROM messaging.mail_messages n WHERE n.id = $2)
                       > (SELECT o.date FROM messaging.mail_messages o WHERE o.id = m.seen_message_id)
                  )
        "#)
            .bind(member_id)
            .bind(message_id)
            .execute(&mut *conn)
            .await?;
        Ok(res.rows_affected() > 0)
    }

    /// Advance the fetched pointer (`channel_fetched` — the lighter "message
    /// reached my client" write; does NOT reset unread counters).
    pub async fn advance_fetched(
        conn: &mut PgConnection,
        member_id: Uuid,
        message_id: Uuid,
    ) -> Result<bool, sqlx::Error> {
        let res = sqlx::query(r#"
            UPDATE messaging.discuss_channel_members m
            SET fetched_message_id = $2
            WHERE m.id = $1
              AND (
                    m.fetched_message_id IS NULL
                    OR (SELECT n.date FROM messaging.mail_messages n WHERE n.id = $2)
                       > (SELECT o.date FROM messaging.mail_messages o WHERE o.id = m.fetched_message_id)
                  )
        "#)
            .bind(member_id)
            .bind(message_id)
            .execute(&mut *conn)
            .await?;
        Ok(res.rows_affected() > 0)
    }

    /// Set the new-message separator (MAIL-M38): the boundary "everything
    /// before this is old". Zeroes the unread counters on the same write.
    pub async fn set_separator(
        conn: &mut PgConnection,
        member_id: Uuid,
        message_id: Uuid,
    ) -> Result<(), sqlx::Error> {
        sqlx::query(r#"
            UPDATE messaging.discuss_channel_members
            SET new_message_separator = $2,
                message_unread_counter = 0,
                unread_counter = 0
            WHERE id = $1
        "#)
            .bind(member_id)
            .bind(message_id)
            .execute(&mut *conn)
            .await?;
        Ok(())
    }

    /// Join: create the member row. Idempotent per G-MAIL-8 uniques — a
    /// re-join of a live membership is a no-op (returns false); a soft-deleted
    /// membership is resurrected (returns true).
    pub async fn upsert_join(
        conn: &mut PgConnection,
        key: &MemberKey<'_>,
    ) -> Result<bool, sqlx::Error> {
        let (partner, guest) = pg(key.identity);
        // Resurrect a soft-deleted row first (the uniques span deleted rows).
        let res = sqlx::query(r#"
            UPDATE messaging.discuss_channel_members
            SET metadata = metadata - 'deleted_at'
            WHERE channel_id = $1
              AND partner_id IS NOT DISTINCT FROM $2
              AND guest_id IS NOT DISTINCT FROM $3
              AND (metadata->>'deleted_at') IS NOT NULL
        "#)
            .bind(key.channel_id)
            .bind(partner)
            .bind(guest)
            .execute(&mut *conn)
            .await?;
        if res.rows_affected() > 0 {
            return Ok(true);
        }
        let res = sqlx::query(r#"
            INSERT INTO messaging.discuss_channel_members (id, channel_id, partner_id, guest_id)
            VALUES ($1, $2, $3, $4)
            ON CONFLICT DO NOTHING
        "#)
            .bind(Uuid::new_v4())
            .bind(key.channel_id)
            .bind(partner)
            .bind(guest)
            .execute(&mut *conn)
            .await?;
        Ok(res.rows_affected() > 0)
    }

    /// Leave: soft-delete the member row. Returns false when there was no
    /// live membership (idempotent leave).
    pub async fn soft_delete_member(
        conn: &mut PgConnection,
        member_id: Uuid,
    ) -> Result<bool, sqlx::Error> {
        let res = sqlx::query(r#"
            UPDATE messaging.discuss_channel_members
            SET metadata = jsonb_set(metadata, '{deleted_at}', to_jsonb(NOW()))
            WHERE id = $1 AND (metadata->>'deleted_at') IS NULL
        "#)
            .bind(member_id)
            .execute(&mut *conn)
            .await?;
        Ok(res.rows_affected() > 0)
    }

    /// Mute (MAIL-M38). `until = None` = unmute (NULL); the until-unmuted
    /// sentinel is `'+infinity'::timestamptz` (Odoo's -1 adapted).
    pub async fn set_mute(
        conn: &mut PgConnection,
        member_id: Uuid,
        until: Option<chrono::DateTime<chrono::Utc>>,
        forever: bool,
    ) -> Result<(), sqlx::Error> {
        sqlx::query(r#"
            UPDATE messaging.discuss_channel_members
            SET mute_until_dt = CASE
                    WHEN $3 THEN '+infinity'::timestamptz
                    ELSE $2
                END
            WHERE id = $1
        "#)
            .bind(member_id)
            .bind(until)
            .bind(forever)
            .execute(&mut *conn)
            .await?;
        Ok(())
    }

    /// Per-channel notification override (all/mentions/no_notif; NULL inherits).
    pub async fn set_custom_notifications(
        conn: &mut PgConnection,
        member_id: Uuid,
        value: Option<&str>,
    ) -> Result<(), sqlx::Error> {
        sqlx::query(r#"
            UPDATE messaging.discuss_channel_members
            SET custom_notifications = $2::member_custom_notifications
            WHERE id = $1
        "#)
            .bind(member_id)
            .bind(value)
            .execute(&mut *conn)
            .await?;
        Ok(())
    }

    /// Toggle the sidebar pin. PORT DELTA (documented): Odoo's pin write is a
    /// raw UPDATE that skips write_date; the port's audit trigger advances
    /// metadata->updated_at. Unpinning stamps `unpin_dt` (drives
    /// re-pin-vs-stay-unpinned UX); pinning clears it.
    pub async fn set_pinned(
        conn: &mut PgConnection,
        member_id: Uuid,
        pinned: bool,
    ) -> Result<(), sqlx::Error> {
        sqlx::query(r#"
            UPDATE messaging.discuss_channel_members
            SET is_pinned = $2,
                unpin_dt = CASE WHEN $2 THEN NULL ELSE NOW() END
            WHERE id = $1
        "#)
            .bind(member_id)
            .bind(pinned)
            .execute(&mut *conn)
            .await?;
        Ok(())
    }

    /// Set the sidebar fold state (open/closed/NULL=undefined).
    pub async fn set_fold_state(
        conn: &mut PgConnection,
        member_id: Uuid,
        fold: Option<&str>,
    ) -> Result<(), sqlx::Error> {
        sqlx::query(r#"
            UPDATE messaging.discuss_channel_members
            SET fold_state = $2::sidebar_fold_state
            WHERE id = $1
        "#)
            .bind(member_id)
            .bind(fold)
            .execute(&mut *conn)
            .await?;
        Ok(())
    }
}

impl Default for ChannelMemberRepository {
    fn default() -> Self {
        Self::new()
    }
}
