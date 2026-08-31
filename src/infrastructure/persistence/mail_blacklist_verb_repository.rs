//! Repository for the email-blacklist verbs (hand-written; user-owned).
//!
//! Holds the SQL for
//! [`crate::application::service::MailBlacklistWriteService`]: the canonical
//! upsert (`ON CONFLICT (email) DO UPDATE SET active` — the archive-pattern
//! add, carrying the opt-out reason), the conditional archive (remove), and
//! the point/batch membership lookups. Distinct name from the generated
//! per-entity repository (mail_blacklist_repository.rs); raw-SQL runtime
//! queries (no sqlx macros, so no .sqlx cache needed).
//!
//! Case-folding: the caller (the write service) lowercases every email
//! before it reaches here — the schema documents the unique as firing on the
//! AS-STORED value with the fold being app-layer. Keeping the fold in the
//! service keeps this layer's contract "canonical value in, canonical value
//! out", mirroring how the E.164 formatter gates the phone side.
//!
//! The `opt_out_reason_id` column is a nullable LOGICAL uuid ref to the
//! mailing module's OptOutReason catalog — opaque here (no FK, no join);
//! the upsert persists it, and a remove NEVER touches it (the archive
//! pattern retains the row, so the reason it was listed stays inspectable).

use sqlx::{PgConnection, Row};
use uuid::Uuid;

/// What the upsert found: the surviving row, whether a row already existed
/// for the email, and whether that existing row was still active.
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

/// Hand-written email-blacklist verb SQL.
pub struct MailBlacklistVerbRepository;

impl MailBlacklistVerbRepository {
    /// Canonical upsert: insert the email as active with the supplied
    /// opt-out reason, or reactivate the existing row for it. Idempotent and
    /// concurrency-safe — parallel adds converge on ONE row (the unique
    /// index plus `ON CONFLICT DO UPDATE`). The reason column is
    /// `COALESCE(EXCLUDED, existing)`: a re-add WITH a reason records it; a
    /// re-add WITHOUT one keeps whatever was already recorded (an unreasoned
    /// add never erases a reasoned listing). `prev` reads the
    /// statement-start snapshot, so the outcome classification reflects the
    /// pre-upsert state.
    pub async fn upsert_active(
        conn: &mut PgConnection,
        email: &str,
        new_id: Uuid,
        opt_out_reason_id: Option<Uuid>,
    ) -> Result<UpsertState, sqlx::Error> {
        let row = sqlx::query(
            r#"WITH prev AS (
                   SELECT id, active FROM messaging.mail_blacklists WHERE email = $1
               ), upsert AS (
                   INSERT INTO messaging.mail_blacklists (id, email, active, opt_out_reason_id)
                   VALUES ($2, $1, TRUE, $3)
                   ON CONFLICT (email) DO UPDATE SET
                       active = TRUE,
                       opt_out_reason_id = COALESCE(EXCLUDED.opt_out_reason_id,
                                                    messaging.mail_blacklists.opt_out_reason_id)
                   RETURNING id
               )
               SELECT upsert.id AS row_id,
                      (prev.id IS NOT NULL) AS existed,
                      COALESCE(prev.active, FALSE) AS was_active
               FROM upsert LEFT JOIN prev ON TRUE"#,
        )
        .bind(email)
        .bind(new_id)
        .bind(opt_out_reason_id)
        .fetch_one(&mut *conn)
        .await?;
        Ok(UpsertState {
            row_id: row.get("row_id"),
            existed: row.get("existed"),
            was_active: row.get("was_active"),
        })
    }

    /// Conditional archive: flip `active` to false for the email, only if it
    /// was true. `Ok(None)` = no row exists for the email at all (the
    /// idempotent-absent case). The row is NEVER deleted, and the recorded
    /// opt-out reason is left exactly as it was (the listing's history stays
    /// inspectable while the address is un-suppressed).
    pub async fn archive_if_active(
        conn: &mut PgConnection,
        email: &str,
    ) -> Result<Option<ArchiveState>, sqlx::Error> {
        let row = sqlx::query(
            r#"WITH prev AS (
                   SELECT id, active FROM messaging.mail_blacklists WHERE email = $1
               ), upd AS (
                   UPDATE messaging.mail_blacklists SET active = FALSE
                   WHERE email = $1 AND active
                   RETURNING id
               )
               SELECT prev.id AS row_id,
                      prev.active AS was_active,
                      (upd.id IS NOT NULL) AS changed
               FROM prev LEFT JOIN upd ON TRUE"#,
        )
        .bind(email)
        .fetch_optional(&mut *conn)
        .await?;
        Ok(row.map(|r| ArchiveState {
            row_id: r.get("row_id"),
            was_active: r.get("was_active"),
            changed: r.get("changed"),
        }))
    }

    /// The active-listing point lookup (the unique index serves it).
    pub async fn exists_active(conn: &mut PgConnection, email: &str) -> Result<bool, sqlx::Error> {
        let listed: bool = sqlx::query_scalar(
            r#"SELECT EXISTS(
                   SELECT 1 FROM messaging.mail_blacklists WHERE email = $1 AND active
               )"#,
        )
        .bind(email)
        .fetch_one(&mut *conn)
        .await?;
        Ok(listed)
    }

    /// The active subset of `emails`, via ONE parameterized `= ANY($1)`
    /// query. Deliberately NOT a load-all: the unbounded
    /// `search([])`-then-filter blacklist read is a known Odoo defect this
    /// port refuses to copy — membership is answered per batch, at index
    /// cost.
    pub async fn active_emails(
        conn: &mut PgConnection,
        emails: &[String],
    ) -> Result<Vec<String>, sqlx::Error> {
        let rows: Vec<String> = sqlx::query_scalar(
            r#"SELECT email FROM messaging.mail_blacklists
               WHERE active AND email = ANY($1)"#,
        )
        .bind(emails)
        .fetch_all(&mut *conn)
        .await?;
        Ok(rows)
    }
}
