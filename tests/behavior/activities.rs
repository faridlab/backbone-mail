//! Activities (MAIL-M17): schedule + plan expansion, and the action_done
//! chaining pump (archive→done computed, next created only for chaining_type
//  == 'trigger').

use backbone_mail::application::service::{ActivityWriteService, ScheduleActivity};
use chrono::NaiveDate;
use sqlx::Row;
use uuid::Uuid;

use super::common;

const TODAY: NaiveDate = NaiveDate::from_ymd_opt(2026, 8, 15).unwrap();

async fn seed_activity_type(pool: &sqlx::PgPool, chaining: &str) -> Uuid {
    let id = Uuid::new_v4();
    sqlx::query(
        r#"INSERT INTO messaging.mail_activity_types (id, name, chaining_type)
           VALUES ($1, $2, $3::mail_activity_chaining_type)"#,
    )
    .bind(id)
    .bind(format!("t-{chaining}-{}", Uuid::new_v4().simple()))
    .bind(chaining)
    .execute(pool)
    .await
    .expect("seed activity type");
    id
}

#[tokio::test]
async fn schedule_projects_overdue_today_planned_state() {
    let Some(pool) = common::test_pool().await else {
        common::skipped("activity schedule projection");
        return;
    };
    let svc = ActivityWriteService::new(pool.clone());
    let user = Uuid::new_v4();
    let res_id = Uuid::new_v4();
    let model = format!("test.doc.{}", Uuid::new_v4().simple());

    let overdue = svc.schedule(ScheduleActivity {
        res_model: model.clone(), res_id, activity_type_id: None,
        summary: None, note: None, date_deadline: TODAY - chrono::Duration::days(1),
        user_id: user, requested_user_id: None, chained_next_activity: None,
    }, TODAY).await.unwrap();
    let today = svc.schedule(ScheduleActivity {
        res_model: model.clone(), res_id, activity_type_id: None,
        summary: None, note: None, date_deadline: TODAY,
        user_id: user, requested_user_id: None, chained_next_activity: None,
    }, TODAY).await.unwrap();
    let planned = svc.schedule(ScheduleActivity {
        res_model: model.clone(), res_id, activity_type_id: None,
        summary: None, note: None, date_deadline: TODAY + chrono::Duration::days(3),
        user_id: user, requested_user_id: None, chained_next_activity: None,
    }, TODAY).await.unwrap();

    for (id, want) in [(overdue, "overdue"), (today, "today"), (planned, "planned")] {
        let row = sqlx::query("SELECT state::text AS state, active FROM messaging.mail_activities WHERE id = $1")
            .bind(id).fetch_one(&pool).await.unwrap();
        assert_eq!(row.get::<String, _>("state"), want);
        assert!(row.get::<bool, _>("active"));
    }

    common::cleanup(&pool, &[("mail_activities", &[overdue, today, planned])]).await;
    cleanup_outbox(&pool, &model).await;
}

async fn cleanup_outbox(pool: &sqlx::PgPool, model: &str) {
    sqlx::query("DELETE FROM messaging.outbox_events WHERE payload->>'res_model' = $1")
        .bind(model).execute(pool).await.ok();
}

#[tokio::test]
async fn action_done_chains_only_for_trigger_types() {
    let Some(pool) = common::test_pool().await else {
        common::skipped("action_done chaining pump");
        return;
    };
    let svc = ActivityWriteService::new(pool.clone());
    let user = Uuid::new_v4();
    let next_user = Uuid::new_v4();
    let res_id = Uuid::new_v4();
    let model = format!("test.doc.{}", Uuid::new_v4().simple());
    let next_deadline = TODAY + chrono::Duration::days(2);

    // A 'suggest'-typed activity with a chain spec: done must NOT create a next.
    let suggest_type = seed_activity_type(&pool, "suggest").await;
    let suggest_id = svc.schedule(ScheduleActivity {
        res_model: model.clone(), res_id, activity_type_id: Some(suggest_type),
        summary: None, note: None, date_deadline: TODAY,
        user_id: user, requested_user_id: None,
        chained_next_activity: Some(serde_json::json!({
            "summary": "should not exist", "user_id": next_user,
            "date_deadline": next_deadline.to_string(),
        })),
    }, TODAY).await.unwrap();
    let (done, next) = svc.action_done(suggest_id, Some("feedback"), TODAY).await.unwrap();
    assert!(done, "the activity archived");
    assert!(next.is_none(), "suggest chaining does not pump: {next:?}");
    let row = sqlx::query("SELECT state::text AS state, active FROM messaging.mail_activities WHERE id = $1")
        .bind(suggest_id).fetch_one(&pool).await.unwrap();
    assert_eq!(row.get::<String, _>("state"), "done");
    assert!(!row.get::<bool, _>("active"), "done == archived+state, never a hand-set write");

    // A 'trigger'-typed one: done MUST create the next activity from the spec.
    let trigger_type = seed_activity_type(&pool, "trigger").await;
    let trigger_id = svc.schedule(ScheduleActivity {
        res_model: model.clone(), res_id, activity_type_id: Some(trigger_type),
        summary: None, note: None, date_deadline: TODAY,
        user_id: user, requested_user_id: None,
        chained_next_activity: Some(serde_json::json!({
            "summary": "call again", "user_id": next_user,
            "date_deadline": next_deadline.to_string(),
        })),
    }, TODAY).await.unwrap();
    let (done, next) = svc.action_done(trigger_id, None, TODAY).await.unwrap();
    assert!(done);
    let next = next.expect("trigger chaining pumps the next activity");
    let nrow = sqlx::query(
        "SELECT summary, user_id, date_deadline, state::text AS state, active FROM messaging.mail_activities WHERE id = $1",
    )
    .bind(next).fetch_one(&pool).await.unwrap();
    assert_eq!(nrow.get::<String, _>("summary"), "call again");
    assert_eq!(nrow.get::<Uuid, _>("user_id"), next_user);
    assert_eq!(nrow.get::<chrono::NaiveDate, _>("date_deadline"), next_deadline);
    assert!(nrow.get::<bool, _>("active"), "the chained next is live");
    assert_eq!(nrow.get::<String, _>("state"), "planned");

    // The completion note: action_done posts a chatter message on the same doc.
    let note: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM messaging.mail_messages WHERE model = $1 AND res_id = $2",
    )
    .bind(&model).bind(res_id).fetch_one(&pool).await.unwrap();
    assert!(note >= 2, "each action_done posted a note: {note}");

    // Idempotent replay: done is terminal.
    let (again, none) = svc.action_done(suggest_id, None, TODAY).await.unwrap();
    assert!(!again, "re-done is a no-op");
    assert!(none.is_none());

    sqlx::query("DELETE FROM messaging.mail_notifications WHERE mail_message_id IN (SELECT id FROM messaging.mail_messages WHERE model = $1)")
        .bind(&model).execute(&pool).await.ok();
    sqlx::query("DELETE FROM messaging.mail_messages WHERE model = $1")
        .bind(&model).execute(&pool).await.ok();
    sqlx::query("DELETE FROM messaging.mail_activities WHERE res_model = $1")
        .bind(&model).execute(&pool).await.ok();
    cleanup_outbox(&pool, &model).await;
}
