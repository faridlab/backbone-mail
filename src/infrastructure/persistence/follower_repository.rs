//! Repository for the follower registry (hand-written; user-owned).
//!
//! Holds the SQL for [`crate::application::service::FollowerWriteService`]:
//! subscribe with an `existing_policy` (MAIL-M10 — Odoo's `_insert_followers`
//! policies are service logic riding the G-MAIL-1 SQL-level
//! `unique(res_model, res_id, partner_id)`), and unsubscribe.
//!
//! `subtype_ids` is stored as a JSONB array of subtype uuids on the row (the
//! per-follower filter the notify pump intersects with a post's subtype).

use sqlx::{PgConnection, Row};
use uuid::Uuid;

/// What to do when the partner already follows the document (MAIL-M10,
/// `_insert_followers` existing-policy — service logic, not SQL).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExistingPolicy {
    /// Already-followed → leave the existing subscription untouched.
    Skip,
    /// Already-followed → ignore that fact and (re)write the subtype list anyway.
    /// Included for parity with Odoo's policy name; behaves as overwrite.
    Force,
    /// Already-followed → overwrite `subtype_ids` with the given list.
    Replace,
    /// Already-followed → UNION the given subtypes into the existing list.
    Update,
}

/// Outcome of a subscribe, for the caller's audit surface.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SubscribeOutcome {
    Inserted,
    Updated,
    Skipped,
}

/// Hand-written follower SQL.
pub struct FollowerRepository;

impl FollowerRepository {
    pub fn new() -> Self {
        Self
    }
}

impl Default for FollowerRepository {
    fn default() -> Self {
        Self::new()
    }
}

impl FollowerRepository {
    /// Subscribe a partner to a document under `policy`. The G-MAIL-1 partial unique
    /// index `(res_model, res_id, partner_id)` is the floor; this method decides
    /// what happens when it fires.
    pub async fn subscribe(
        conn: &mut PgConnection,
        res_model: &str,
        res_id: Uuid,
        partner_id: Uuid,
        subtype_ids: &[Uuid],
        policy: ExistingPolicy,
    ) -> Result<SubscribeOutcome, sqlx::Error> {
        let subtypes = serde_json::to_value(subtype_ids).unwrap_or(serde_json::json!([]));

        // Insert-first (the common case); on conflict, apply the policy explicitly so
        // each policy's semantics stay visible in SQL rather than encoded in
        // ON CONFLICT magic.
        let inserted = sqlx::query_scalar::<_, Uuid>(
            r#"INSERT INTO messaging.mail_followers (res_model, res_id, partner_id, subtype_ids)
               VALUES ($1,$2,$3,$4)
               -- The G-MAIL-1 arbiter is a PARTIAL unique index, so the inference
               -- clause must repeat the index predicate (a bare column list cannot
               -- match a partial index).
               ON CONFLICT (res_model, res_id, partner_id) WHERE partner_id IS NOT NULL DO NOTHING
               RETURNING id"#,
        )
        .bind(res_model)
        .bind(res_id)
        .bind(partner_id)
        .bind(&subtypes)
        .fetch_optional(&mut *conn)
        .await?;

        if inserted.is_some() {
            return Ok(SubscribeOutcome::Inserted);
        }

        match policy {
            ExistingPolicy::Skip => Ok(SubscribeOutcome::Skipped),
            ExistingPolicy::Force | ExistingPolicy::Replace => {
                sqlx::query(
                    r#"UPDATE messaging.mail_followers
                       SET subtype_ids = $4
                       WHERE res_model = $1 AND res_id = $2 AND partner_id = $3"#,
                )
                .bind(res_model)
                .bind(res_id)
                .bind(partner_id)
                .bind(&subtypes)
                .execute(&mut *conn)
                .await?;
                Ok(SubscribeOutcome::Updated)
            }
            // Union: merge the given subtypes into the existing list, deduped.
            ExistingPolicy::Update => {
                sqlx::query(
                    r#"UPDATE messaging.mail_followers AS f
                       SET subtype_ids = (
                           SELECT COALESCE(jsonb_agg(DISTINCT v), '[]'::jsonb)
                           FROM jsonb_array_elements(f.subtype_ids || $4) AS t(v)
                       )
                       WHERE f.res_model = $1 AND f.res_id = $2 AND f.partner_id = $3"#,
                )
                .bind(res_model)
                .bind(res_id)
                .bind(partner_id)
                .bind(&subtypes)
                .execute(&mut *conn)
                .await?;
                Ok(SubscribeOutcome::Updated)
            }
        }
    }

    /// Unsubscribe partners from a document. Physical delete — the Odoo row is the
    /// subscription itself (no archive pattern here, unlike mail.blacklist).
    pub async fn unsubscribe(
        conn: &mut PgConnection,
        res_model: &str,
        res_id: Uuid,
        partner_ids: &[Uuid],
    ) -> Result<u64, sqlx::Error> {
        let res = sqlx::query(
            r#"DELETE FROM messaging.mail_followers
               WHERE res_model = $1 AND res_id = $2 AND partner_id = ANY($3)"#,
        )
        .bind(res_model)
        .bind(res_id)
        .bind(partner_ids)
        .execute(&mut *conn)
        .await?;
        Ok(res.rows_affected())
    }

    /// The partners following a document (with their subtype filters) — the notify
    /// pump's recipient seed. Returns `(partner_id, subtype_ids)`.
    pub async fn list_followers(
        conn: &mut PgConnection,
        res_model: &str,
        res_id: Uuid,
    ) -> Result<Vec<(Uuid, serde_json::Value)>, sqlx::Error> {
        let rows = sqlx::query(
            r#"SELECT partner_id, subtype_ids FROM messaging.mail_followers
               WHERE res_model = $1 AND res_id = $2 AND partner_id IS NOT NULL"#,
        )
        .bind(res_model)
        .bind(res_id)
        .fetch_all(&mut *conn)
        .await?;
        Ok(rows
            .iter()
            .map(|r| (r.get::<Uuid, _>("partner_id"), r.get::<serde_json::Value, _>("subtype_ids")))
            .collect())
    }
}
