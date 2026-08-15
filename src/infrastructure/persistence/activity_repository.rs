//! Repository for the activity schedule/done pipeline (hand-written; user-owned).
//!
//! Holds the SQL for [`crate::application::service::ActivityWriteService`]: schedule
//! (single + plan-based), the archive-to-done transition (MAIL-M17 — `state='done'`
//! is computed-from-archive; the ONLY path is `action_done`), and the next-activity
//! read for the chaining pump.

use chrono::NaiveDate;
use serde_json::Value as Json;
use sqlx::{PgConnection, Row};
use uuid::Uuid;

/// An activity row as the chaining pump reads it.
pub struct ActivityRow {
    pub id: Uuid,
    pub res_model: String,
    pub res_id: Uuid,
    pub activity_type_id: Option<Uuid>,
    pub summary: Option<String>,
    pub note: Option<String>,
    pub user_id: Uuid,
    pub date_deadline: NaiveDate,
    pub active: bool,
    /// The port's carrier for the next-activity spec (the port of Odoo's
    /// `triggered_next_type_id` chain): `{activity_type_id, summary, note,
    /// user_id, date_deadline}` — consumed only when chaining_type == trigger.
    pub chained_next_activity: Option<Json>,
}

/// The activity-type knobs the pump needs (MAIL-M19).
pub struct ActivityTypeRow {
    pub id: Uuid,
    pub chaining_type: String,
    pub delay_count: i32,
    pub delay_unit: String,
    pub delay_from: String,
    pub default_user_id: Option<Uuid>,
    pub active: bool,
}

/// One `mail_activity_plan_templates` row (MAIL-M21) — the "seed-from-type,
/// user-overridable" hybrid fields land here as plain columns.
pub struct PlanTemplateRow {
    pub id: Uuid,
    pub activity_type_id: Uuid,
    pub summary: Option<String>,
    pub note: Option<String>,
    pub delay_count: Option<i32>,
    pub delay_unit: Option<String>,
    pub user_id: Option<Uuid>,
}

/// The exact `mail_activities` row a schedule writes.
pub struct NewActivityRow<'a> {
    pub id: Uuid,
    pub res_model: &'a str,
    pub res_id: Uuid,
    pub activity_type_id: Option<Uuid>,
    pub summary: Option<&'a str>,
    pub note: Option<&'a str>,
    pub date_deadline: NaiveDate,
    pub user_id: Uuid,
    pub requested_user_id: Option<Uuid>,
    pub state: &'a str,
    pub chained_next_activity: Option<&'a Json>,
}

/// Hand-written activity SQL.
pub struct ActivityRepository;

impl ActivityRepository {
    pub fn new() -> Self {
        Self
    }
}

impl Default for ActivityRepository {
    fn default() -> Self {
        Self::new()
    }
}

impl ActivityRepository {
    /// Schedule one activity. G-MAIL-3/4 (CHECK `res_model IS NULL OR res_id/user_id
    /// IS NOT NULL`) are service-enforced by the caller before this runs — the
    /// guards are `enforcement: both` in the hook DSL.
    pub async fn insert_activity(
        conn: &mut PgConnection,
        a: &NewActivityRow<'_>,
    ) -> Result<Uuid, sqlx::Error> {
        sqlx::query_scalar::<_, Uuid>(
            r#"INSERT INTO messaging.mail_activities
                 (id, res_model, res_id, activity_type_id, summary, note, date_deadline,
                  user_id, requested_user_id, state, chained_next_activity)
               VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10::mail_activity_state,$11)
               RETURNING id"#,
        )
        .bind(a.id)
        .bind(a.res_model)
        .bind(a.res_id)
        .bind(a.activity_type_id)
        .bind(a.summary)
        .bind(a.note)
        .bind(a.date_deadline)
        .bind(a.user_id)
        .bind(a.requested_user_id)
        .bind(a.state)
        .bind(a.chained_next_activity)
        .fetch_one(&mut *conn)
        .await
    }

    /// Load an activity (the pump's input).
    pub async fn find_activity(
        conn: &mut PgConnection,
        id: Uuid,
    ) -> Result<Option<ActivityRow>, sqlx::Error> {
        let row = sqlx::query(
            r#"SELECT id, res_model, res_id, activity_type_id, summary, note, user_id,
                      date_deadline, active, chained_next_activity
               FROM messaging.mail_activities WHERE id = $1"#,
        )
        .bind(id)
        .fetch_optional(&mut *conn)
        .await?;
        Ok(row.map(|r| ActivityRow {
            id: r.get("id"),
            res_model: r.get("res_model"),
            res_id: r.get("res_id"),
            activity_type_id: r.get("activity_type_id"),
            summary: r.get("summary"),
            note: r.get("note"),
            user_id: r.get("user_id"),
            date_deadline: r.get("date_deadline"),
            active: r.get("active"),
            chained_next_activity: r.get("chained_next_activity"),
        }))
    }

    /// Load an activity type (the chaining knob).
    pub async fn find_activity_type(
        conn: &mut PgConnection,
        id: Uuid,
    ) -> Result<Option<ActivityTypeRow>, sqlx::Error> {
        let row = sqlx::query(
            r#"SELECT id, chaining_type::text AS chaining_type, delay_count,
                      delay_unit::text AS delay_unit, delay_from::text AS delay_from,
                      default_user_id, active
               FROM messaging.mail_activity_types WHERE id = $1"#,
        )
        .bind(id)
        .fetch_optional(&mut *conn)
        .await?;
        Ok(row.map(|r| ActivityTypeRow {
            id: r.get("id"),
            chaining_type: r.get("chaining_type"),
            delay_count: r.get("delay_count"),
            delay_unit: r.get("delay_unit"),
            delay_from: r.get("delay_from"),
            default_user_id: r.get("default_user_id"),
            active: r.get("active"),
        }))
    }

    /// The archive-to-done transition (MAIL-M17): `active = FALSE` and the stored
    /// state projection set to `'done'` in the same write, guarded on `active` so
    /// double-done is a no-op. There is deliberately no path that writes
    /// `state='done'` on an active row — done-ness IS the archive.
    pub async fn archive_done(conn: &mut PgConnection, id: Uuid) -> Result<bool, sqlx::Error> {
        let updated = sqlx::query_scalar::<_, Uuid>(
            r#"UPDATE messaging.mail_activities
               SET active = FALSE, state = 'done'::mail_activity_state
               WHERE id = $1 AND active = TRUE
               RETURNING id"#,
        )
        .bind(id)
        .fetch_optional(&mut *conn)
        .await?;
        Ok(updated.is_some())
    }

    /// Recompute the stored state projection (overdue/today/planned) for an active
    /// row against a reference "today" — the compute Odoo derives on read. Called
    /// after schedule; the state column is a stored projection, so it must be
    /// written whenever the row is (re)written.
    pub async fn project_state(
        conn: &mut PgConnection,
        id: Uuid,
        today: NaiveDate,
    ) -> Result<(), sqlx::Error> {
        sqlx::query(
            r#"UPDATE messaging.mail_activities
               SET state = CASE
                     WHEN date_deadline < $2 THEN 'overdue'::mail_activity_state
                     WHEN date_deadline = $2 THEN 'today'::mail_activity_state
                     ELSE 'planned'::mail_activity_state
                   END
               WHERE id = $1 AND active = TRUE"#,
        )
        .bind(id)
        .bind(today)
        .execute(&mut *conn)
        .await?;
        Ok(())
    }

    /// A plan's templates (MAIL-M20/M21) — one activity per template on schedule_plan.
    pub async fn list_plan_templates(
        conn: &mut PgConnection,
        plan_id: Uuid,
    ) -> Result<Vec<PlanTemplateRow>, sqlx::Error> {
        let rows = sqlx::query(
            r#"SELECT id, activity_type_id, summary, note,
                      delay_count, delay_unit::text AS delay_unit, user_id
               FROM messaging.mail_activity_plan_templates
               WHERE plan_id = $1 AND (metadata->>'deleted_at') IS NULL
               ORDER BY id"#,
        )
        .bind(plan_id)
        .fetch_all(&mut *conn)
        .await?;
        Ok(rows
            .iter()
            .map(|r| PlanTemplateRow {
                id: r.get("id"),
                activity_type_id: r.get("activity_type_id"),
                summary: r.get("summary"),
                note: r.get("note"),
                delay_count: r.get("delay_count"),
                delay_unit: r.get("delay_unit"),
                user_id: r.get("user_id"),
            })
            .collect())
    }
}
