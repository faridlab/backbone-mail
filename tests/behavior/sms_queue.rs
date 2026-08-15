//! The sms queue (SM-M1/SM-M2, TR-SM-1): enqueue arms the drainer via the
//! staged `SmsCreated` event; the drainer claims, sends through the port, and
//! applies outcomes through the pump map.

use backbone_mail::application::service::{NoopSmsApi, SmsWriteService};
use sqlx::Row;
use uuid::Uuid;

use super::common;

async fn sms_state(pool: &sqlx::PgPool, id: Uuid) -> String {
    sqlx::query("SELECT state::text FROM messaging.sms WHERE id = $1")
        .bind(id)
        .fetch_one(pool)
        .await
        .expect("sms row")
        .get::<String, _>("state")
}

#[tokio::test]
async fn enqueue_stages_arm_event_and_drain_accepts() {
    let Some(pool) = common::test_pool().await else {
        common::skipped("sms enqueue + drain");
        return;
    };
    let _drain_guard = common::DRAIN_LOCK.lock().await;
    common::sweep_queues(&pool).await;
    let svc = SmsWriteService::new(pool.clone());

    let (id, sms_uuid) = svc
        .enqueue("+6281200000101", "your code is 1234", None, None, None, None)
        .await
        .expect("enqueue");

    // TR-SM-1: the arming event is staged IN the enqueue tx.
    let armed: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM messaging.outbox_events WHERE event_type = 'SmsCreated' AND aggregate_id = $1",
    )
    .bind(id.to_string())
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(armed, 1, "SmsCreated staged with the row it arms for");

    // Drain through the accepting double: outgoing → process → pending ('Sent').
    let out = svc
        .process_queue(&NoopSmsApi::accepting(), 10, 1)
        .await
        .expect("drain");
    assert_eq!(out.claimed, 1);
    assert_eq!(out.accepted, 1, "IAP accept → pending: {out:?}");
    assert_eq!(sms_state(&pool, id).await, "pending");

    // The tracker mirror follows the same lattice.
    let tracker: String =
        sqlx::query_scalar("SELECT state::text FROM messaging.sms_trackers WHERE sms_uuid = $1")
            .bind(&sms_uuid)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(tracker, "pending");

    // Second drain: nothing left to claim.
    let out2 = svc.process_queue(&NoopSmsApi::accepting(), 10, 1).await.unwrap();
    assert_eq!(out2.claimed, 0, "queue drained");

    sqlx::query("DELETE FROM messaging.sms_trackers WHERE sms_uuid = $1")
        .bind(&sms_uuid).execute(&pool).await.ok();
    sqlx::query("DELETE FROM messaging.sms WHERE id = $1")
        .bind(id).execute(&pool).await.ok();
    sqlx::query("DELETE FROM messaging.outbox_events WHERE aggregate_id = $1")
        .bind(id.to_string()).execute(&pool).await.ok();
}

#[tokio::test]
async fn drain_failure_lands_error_with_failure_type() {
    let Some(pool) = common::test_pool().await else {
        common::skipped("sms drain failure");
        return;
    };
    let _drain_guard = common::DRAIN_LOCK.lock().await;
    common::sweep_queues(&pool).await;
    let svc = SmsWriteService::new(pool.clone());
    let (id, sms_uuid) = svc
        .enqueue("+6281200000102", "will fail", None, None, None, None)
        .await
        .expect("enqueue");

    let out = svc
        .process_queue(&NoopSmsApi::failing("sms_server", "provider exploded"), 10, 1)
        .await
        .expect("drain");
    assert_eq!(out.failed, 1, "error outcome: {out:?}");
    assert_eq!(sms_state(&pool, id).await, "error");

    let row = sqlx::query(
        "SELECT failure_type::text AS ft, error_message AS em FROM messaging.sms WHERE id = $1",
    )
    .bind(id)
    .fetch_one(&pool)
    .await
    .unwrap();
    let ft: Option<String> = row.get("ft");
    let em: Option<String> = row.get("em");
    assert_eq!(ft.as_deref(), Some("sms_server"));
    assert_eq!(em.as_deref(), Some("provider exploded"));

    sqlx::query("DELETE FROM messaging.sms_trackers WHERE sms_uuid = $1")
        .bind(&sms_uuid).execute(&pool).await.ok();
    sqlx::query("DELETE FROM messaging.sms WHERE id = $1")
        .bind(id).execute(&pool).await.ok();
    sqlx::query("DELETE FROM messaging.outbox_events WHERE aggregate_id = $1")
        .bind(id.to_string()).execute(&pool).await.ok();
}

#[tokio::test]
async fn enqueue_validates_inputs() {
    let Some(pool) = common::test_pool().await else {
        common::skipped("sms enqueue validation");
        return;
    };
    let svc = SmsWriteService::new(pool);
    assert!(svc.enqueue("", "body", None, None, None, None).await.is_err());
    assert!(svc.enqueue("+62...", "", None, None, None, None).await.is_err());
}
