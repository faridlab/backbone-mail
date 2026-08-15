//! The activity write service (hand-written; user-owned).
//!
//! The port of Odoo `mail.activity` scheduling + `_action_done` (MAIL-M17 /
//! TR-MAIL-5): schedule (single + plan-based), and the chaining pump — archive →
//! done COMPUTED (you cannot write state='done'; the only path is action_done),
//! next activity auto-created ONLY when the type's `chaining_type == 'trigger'`
//! ('suggest' leaves the recommendation to the user).
//!
//! The deadline→state projection (overdue/today/planned) is computed against a
//! caller-supplied "today" (the user's timezone decides the split in Odoo; the
//! caller passes the user-local date — we do not guess timezones here).

use chrono::{Datelike, Duration, NaiveDate};
use serde_json::Value as Json;
use uuid::Uuid;

use crate::domain::event::{record_channel, stage_bus_event};
use crate::infrastructure::persistence::activity_repository::{
    ActivityRepository, ActivityRow, NewActivityRow,
};
use crate::infrastructure::persistence::message_pipeline_repository::MessagePipelineRepository;

#[derive(Debug, thiserror::Error)]
pub enum ActivityError {
    #[error("db: {0}")]
    Db(#[from] sqlx::Error),
    #[error("invalid input: {0}")]
    Invalid(String),
    #[error("not found: {0}")]
    NotFound(String),
}

/// One scheduled activity.
#[derive(Debug, Clone)]
pub struct ScheduleActivity {
    pub res_model: String,
    pub res_id: Uuid,
    pub activity_type_id: Option<Uuid>,
    pub summary: Option<String>,
    pub note: Option<String>,
    pub date_deadline: NaiveDate,
    pub user_id: Uuid,
    pub requested_user_id: Option<Uuid>,
    /// The chaining pump's next-activity spec, consumed by action_done when
    /// chaining_type == 'trigger': `{activity_type_id?, summary?, note?, user_id?,
    /// date_deadline?}` (ISO date) — the port of Odoo's triggered-next-type chain.
    pub chained_next_activity: Option<Json>,
}

pub struct ActivityWriteService {
    pool: sqlx::PgPool,
}

impl ActivityWriteService {
    pub fn new(pool: sqlx::PgPool) -> Self {
        Self { pool }
    }

    /// Schedule ONE activity (the `activity.schedule` wizard's single mode).
    pub async fn schedule(
        &self,
        cmd: ScheduleActivity,
        today: NaiveDate,
    ) -> Result<Uuid, ActivityError> {
        self.validate(&cmd).await?;
        let mut tx = self.pool.begin().await?;
        let id = self.insert(&mut tx, &cmd, today).await?;
        stage_bus_event(
            &mut tx,
            "ActivityScheduled",
            "MailActivity",
            id,
            record_channel(&cmd.res_model, cmd.res_id),
            "ActivityScheduled",
            serde_json::json!({
                "activity_id": id, "res_model": cmd.res_model, "res_id": cmd.res_id,
                "user_id": cmd.user_id, "date_deadline": cmd.date_deadline.to_string(),
            }),
        )
        .await?;
        tx.commit().await?;
        Ok(id)
    }

    /// Schedule every template of a plan onto a document (MAIL-M20/M21 — plan-based
    /// batch scheduling). Returns the minted activity ids.
    pub async fn schedule_plan(
        &self,
        plan_id: Uuid,
        res_model: &str,
        res_id: Uuid,
        fallback_user_id: Uuid,
        today: NaiveDate,
    ) -> Result<Vec<Uuid>, ActivityError> {
        let mut tx = self.pool.begin().await?;
        let templates = ActivityRepository::list_plan_templates(&mut tx, plan_id).await?;
        if templates.is_empty() {
            return Err(ActivityError::NotFound(format!("plan {plan_id} has no templates")));
        }
        let mut ids = Vec::new();
        for t in &templates {
            // MAIL-M21: the template's user (seed-from-type, user-overridable) wins;
            // the caller's user is the fallback.
            let user_id = t.user_id.unwrap_or(fallback_user_id);
            // The template's delay (count/unit) shifts today's date; delay_from is
            // 'plan_date' semantics here (anchored on the scheduling date).
            let deadline = shift_date(today, t.delay_count.unwrap_or(0), t.delay_unit.as_deref());
            let cmd = ScheduleActivity {
                res_model: res_model.into(),
                res_id,
                activity_type_id: Some(t.activity_type_id),
                summary: t.summary.clone(),
                note: t.note.clone(),
                date_deadline: deadline,
                user_id,
                requested_user_id: None,
                chained_next_activity: None,
            };
            let id = self.insert(&mut tx, &cmd, deadline).await?;
            stage_bus_event(
                &mut tx,
                "ActivityScheduled",
                "MailActivity",
                id,
                record_channel(res_model, res_id),
                "ActivityScheduled",
                serde_json::json!({
                    "activity_id": id, "plan_id": plan_id,
                    "res_model": res_model, "res_id": res_id, "user_id": user_id,
                    "date_deadline": deadline.to_string(),
                }),
            )
            .await?;
            ids.push(id);
        }
        tx.commit().await?;
        Ok(ids)
    }

    /// `_action_done` — the chaining pump (TR-MAIL-5), ONE transaction:
    ///
    /// 1. post the completion note (a `mail_messages` row on the document),
    /// 2. archive the activity (`active=false` → the stored state projection goes
    ///    `'done'` — the ONLY path to done),
    /// 3. IF the activity type's `chaining_type == 'trigger'`: create the next
    ///    activity from the row's `chained_next_activity` spec. `'suggest'` never
    ///    auto-creates (the user decides).
    ///
    /// Returns `(done, next_activity_id)`.
    pub async fn action_done(
        &self,
        activity_id: Uuid,
        feedback: Option<&str>,
        today: NaiveDate,
    ) -> Result<(bool, Option<Uuid>), ActivityError> {
        let mut tx = self.pool.begin().await?;

        let activity = ActivityRepository::find_activity(
            &mut tx, activity_id)
            .await?
            .ok_or_else(|| ActivityError::NotFound(format!("activity {activity_id}")))?;

        // 1. The completion note rides the document's chatter.
        let note_body = feedback.unwrap_or("Activity done");
        let message_id = Uuid::new_v4();
        MessagePipelineRepository::insert_mail_message(
            &mut tx,
            &crate::infrastructure::persistence::message_pipeline_repository::NewMailMessageRow {
                id: message_id,
                subject: None,
                body: note_body,
                message_type: "notification",
                subtype_id: None,
                is_internal: true,
                author_id: Some(activity.user_id),
                author_guest_id: None,
                email_from: None,
                message_id: None,
                reply_to: None,
                model: Some(&activity.res_model),
                res_id: Some(activity.res_id),
                record_name: None,
            },
        )
        .await?;
        let channel_key = record_channel(&activity.res_model, activity.res_id);
        stage_bus_event(
            &mut tx,
            "MessagePosted",
            "MailMessage",
            message_id,
            channel_key.clone(),
            "MessagePosted",
            serde_json::json!({
                "message_id": message_id, "message_type": "notification",
                "model": activity.res_model, "res_id": activity.res_id,
                "activity_done": activity_id,
            }),
        )
        .await?;

        // 2. Archive → done. Idempotent: an already-archived row reports Ok(false)
        //    and we still surface the (already-created) chain decision as absent.
        let newly_done = ActivityRepository::archive_done(&mut tx, activity_id).await?;

        // 3. The chaining pump — next activity ONLY on 'trigger'.
        let mut next_id = None;
        if newly_done {
            if let Some(type_id) = activity.activity_type_id {
                if let Some(atype) = ActivityRepository::find_activity_type(&mut tx, type_id).await? {
                    if atype.chaining_type == "trigger" {
                        next_id = self
                            .create_chained_next(&mut tx, &activity, &channel_key, today)
                            .await?;
                    }
                }
            }
            stage_bus_event(
                &mut tx,
                "ActivityDone",
                "MailActivity",
                activity_id,
                channel_key,
                "ActivityDone",
                serde_json::json!({
                    "activity_id": activity_id, "chained_next_activity_id": next_id,
                }),
            )
            .await?;
        }

        tx.commit().await?;
        Ok((newly_done, next_id))
    }

    async fn create_chained_next(
        &self,
        tx: &mut sqlx::PgConnection,
        activity: &ActivityRow,
        channel_key: &str,
        today: NaiveDate,
    ) -> Result<Option<Uuid>, ActivityError> {
        // No spec → no chain (the type said trigger but the row carries no next
        // spec; a legal, visible no-op).
        let Some(spec) = &activity.chained_next_activity else {
            return Ok(None);
        };
        let deadline = spec["date_deadline"]
            .as_str()
            .and_then(|d| NaiveDate::parse_from_str(d, "%Y-%m-%d").ok())
            .unwrap_or(today);
        let cmd = ScheduleActivity {
            res_model: activity.res_model.clone(),
            res_id: activity.res_id,
            activity_type_id: spec["activity_type_id"].as_str().and_then(|s| Uuid::parse_str(s).ok()),
            summary: spec["summary"].as_str().map(|s| s.to_string()),
            note: spec["note"].as_str().map(|s| s.to_string()),
            date_deadline: deadline,
            user_id: spec["user_id"]
                .as_str()
                .and_then(|s| Uuid::parse_str(s).ok())
                .unwrap_or(activity.user_id),
            requested_user_id: None,
            chained_next_activity: None,
        };
        let id = self.insert(tx, &cmd, today).await?;
        stage_bus_event(
            tx,
            "ActivityScheduled",
            "MailActivity",
            id,
            channel_key.to_string(),
            "ActivityScheduled",
            serde_json::json!({
                "activity_id": id, "chained_from": activity.id,
                "res_model": cmd.res_model, "res_id": cmd.res_id,
                "user_id": cmd.user_id, "date_deadline": cmd.date_deadline.to_string(),
            }),
        )
        .await?;
        Ok(Some(id))
    }

    /// G-MAIL-3/4 are `enforcement: both` — the service check runs ahead of the
    /// DB CHECKs so the error is a domain error, not a constraint violation.
    async fn validate(&self, cmd: &ScheduleActivity) -> Result<(), ActivityError> {
        // res_model is NOT NULL on the table; the CHECK guards are the pair rules.
        if cmd.res_model.trim().is_empty() {
            return Err(ActivityError::Invalid("res_model is required".into()));
        }
        Ok(())
    }

    async fn insert(
        &self,
        tx: &mut sqlx::PgConnection,
        cmd: &ScheduleActivity,
        _today: NaiveDate,
    ) -> Result<Uuid, ActivityError> {
        let id = Uuid::new_v4();
        // The stored state projection off the deadline (overdue/today/planned).
        let state = if cmd.date_deadline < _today {
            "overdue"
        } else if cmd.date_deadline == _today {
            "today"
        } else {
            "planned"
        };
        ActivityRepository::insert_activity(
                tx,
                &NewActivityRow {
                    id,
                    res_model: &cmd.res_model,
                    res_id: cmd.res_id,
                    activity_type_id: cmd.activity_type_id,
                    summary: cmd.summary.as_deref(),
                    note: cmd.note.as_deref(),
                    date_deadline: cmd.date_deadline,
                    user_id: cmd.user_id,
                    requested_user_id: cmd.requested_user_id,
                    state,
                    chained_next_activity: cmd.chained_next_activity.as_ref(),
                },
            )
            .await?;
        Ok(id)
    }
}

/// Shift a date by (count, unit) — the plan-template delay (days/weeks/months).
fn shift_date(base: NaiveDate, count: i32, unit: Option<&str>) -> NaiveDate {
    match unit {
        Some("weeks") => base + Duration::weeks(count as i64),
        Some("months") => shift_months(base, count),
        // 'days' and anything unrecognised (defensive) both shift days.
        _ => base + Duration::days(count as i64),
    }
}

/// chrono has no month arithmetic on NaiveDate; implement clamped month shift
/// (Jan 31 + 1 month → Feb 28/29).
fn shift_months(base: NaiveDate, count: i32) -> NaiveDate {
    let months = base.year() * 12 + base.month() as i32 - 1 + count;
    let year = months.div_euclid(12);
    let month = (months.rem_euclid(12) + 1) as u32;
    let day = base.day().min(days_in_month(year, month));
    NaiveDate::from_ymd_opt(year, month, day).unwrap_or(base)
}

fn days_in_month(year: i32, month: u32) -> u32 {
    match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 => {
            if (year % 4 == 0 && year % 100 != 0) || year % 400 == 0 {
                29
            } else {
                28
            }
        }
        _ => 31,
    }
}
