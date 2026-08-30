//! Repository for the phone-blacklist verbs (hand-written; user-owned).
//!
//! Holds the SQL for
//! [`crate::application::service::PhoneBlacklistWriteService`]: the
//! canonical upsert (`ON CONFLICT (number) DO UPDATE SET active` — the
//! archive-pattern add), the conditional archive (remove), and the
//! point/batch membership lookups. Distinct name from the generated
//! per-entity repository; raw-SQL runtime queries (no sqlx macros, so no
//! .sqlx cache needed).
//!
//! The verb layer keys on the `active` archive flag only: the verbs never
//! physically delete (the substrate contract), so there is no soft-delete
//! marker to honor here — mutation outside the sanctioned verb path is the
//! explicitly-unguarded generic-CRUD surface's documented hazard, not this
//! layer's concern.

use sqlx::{PgConnection, Row};
use uuid::Uuid;

/// What the upsert found: the surviving row, whether a row already existed
/// for the number, and whether that existing row was still active.
#[derive(Debug, Clone, Copy)]
pub struct UpsertState {
    pub row_id: Uuid,
    pub existed: bool,
    pub was_active: bool,
}

/// What the archive found: the row, and whether it was still active before
/// the conditional update ran.
#[derive(Debug, Clone, Copy)]
pub struct ArchiveState {
    pub row_id: Uuid,
    pub was_active: bool,
    /// Did this statement actually flip `active` to false?
    pub changed: bool,
}

/// Hand-written phone-blacklist verb SQL.
pub struct PhoneBlacklistVerbRepository;

impl PhoneBlacklistVerbRepository {
    /// Canonical upsert: insert the number as active, or reactivate the
    /// existing row for it. Idempotent and concurrency-safe — parallel adds
    /// converge on ONE row (the unique index plus `ON CONFLICT DO UPDATE`;
    /// no near-duplicate can survive because the value is canonical before
    /// it ever reaches here). `prev` reads the statement-start snapshot, so
    /// the outcome classification reflects the pre-upsert state.
    pub async fn upsert_active(
        conn: &mut PgConnection,
        number: &str,
        new_id: Uuid,
    ) -> Result<UpsertState, sqlx::Error> {
        let row = sqlx::query(
            r#"WITH prev AS (
                   SELECT id, active FROM messaging.phone_blacklists WHERE number = $1
               ), upsert AS (
                   INSERT INTO messaging.phone_blacklists (id, number, active)
                   VALUES ($2, $1, TRUE)
                   ON CONFLICT (number) DO UPDATE SET active = TRUE
                   RETURNING id
               )
               SELECT upsert.id AS row_id,
                      (prev.id IS NOT NULL) AS existed,
                      COALESCE(prev.active, FALSE) AS was_active
               FROM upsert LEFT JOIN prev ON TRUE"#,
        )
        .bind(number)
        .bind(new_id)
        .fetch_one(&mut *conn)
        .await?;
        Ok(UpsertState {
            row_id: row.get("row_id"),
            existed: row.get("existed"),
            was_active: row.get("was_active"),
        })
    }

    /// Conditional archive: flip `active` to false for the number, only if
    /// it was true. `Ok(None)` = no row exists for the number at all (the
    /// idempotent-absent case). The row is NEVER deleted.
    pub async fn archive_if_active(
        conn: &mut PgConnection,
        number: &str,
    ) -> Result<Option<ArchiveState>, sqlx::Error> {
        let row = sqlx::query(
            r#"WITH prev AS (
                   SELECT id, active FROM messaging.phone_blacklists WHERE number = $1
               ), upd AS (
                   UPDATE messaging.phone_blacklists SET active = FALSE
                   WHERE number = $1 AND active
                   RETURNING id
               )
               SELECT prev.id AS row_id,
                      prev.active AS was_active,
                      (upd.id IS NOT NULL) AS changed
               FROM prev LEFT JOIN upd ON TRUE"#,
        )
        .bind(number)
        .fetch_optional(&mut *conn)
        .await?;
        Ok(row.map(|r| ArchiveState {
            row_id: r.get("row_id"),
            was_active: r.get("was_active"),
            changed: r.get("changed"),
        }))
    }

    /// The active-listing point lookup (the unique index serves it).
    pub async fn exists_active(conn: &mut PgConnection, number: &str) -> Result<bool, sqlx::Error> {
        let listed: bool = sqlx::query_scalar(
            r#"SELECT EXISTS(
                   SELECT 1 FROM messaging.phone_blacklists WHERE number = $1 AND active
               )"#,
        )
        .bind(number)
        .fetch_one(&mut *conn)
        .await?;
        Ok(listed)
    }

    /// The active subset of `numbers`, via ONE parameterized `= ANY($1)`
    /// query. Deliberately NOT a load-all: the unbounded
    /// `search([])`-then-filter blacklist read is a known Odoo defect this
    /// port refuses to copy — membership is answered per batch, at index
    /// cost.
    pub async fn active_numbers(
        conn: &mut PgConnection,
        numbers: &[String],
    ) -> Result<Vec<String>, sqlx::Error> {
        let rows: Vec<String> = sqlx::query_scalar(
            r#"SELECT number FROM messaging.phone_blacklists
               WHERE active AND number = ANY($1)"#,
        )
        .bind(numbers)
        .fetch_all(&mut *conn)
        .await?;
        Ok(rows)
    }
}
