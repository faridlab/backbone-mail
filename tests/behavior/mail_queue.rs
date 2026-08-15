//! The outgoing-mail queue (MAIL-M2): messaging owns the persisted row + state
//! machine, the drainer stages `MailDispatchRequested` (NO SMTP here), and the
//! crash-safety pre-write puts `exception` on the row BEFORE dispatch.

use backbone_mail::application::service::MailQueueWriteService;
use sqlx::Row;
use uuid::Uuid;

use super::common;

async fn seed_message(pool: &sqlx::PgPool) -> Uuid {
    let id = Uuid::new_v4();
    sqlx::query("INSERT INTO messaging.mail_messages (id, body) VALUES ($1, $2)")
        .bind(id)
        .bind("mail-queue test body")
        .execute(pool)
        .await
        .expect("seed mail_messages");
    id
}

async fn mail_row_state(pool: &sqlx::PgPool, id: Uuid) -> String {
    sqlx::query("SELECT state::text FROM messaging.mails WHERE id = $1")
        .bind(id)
        .fetch_one(pool)
        .await
        .expect("mails row")
        .get::<String, _>("state")
}

#[tokio::test]
async fn drain_pre_writes_exception_and_stages_dispatch_request() {
    let Some(pool) = common::test_pool().await else {
        common::skipped("mail queue drain + MAIL-M2 pre-write");
        return;
    };
    let _drain_guard = common::DRAIN_LOCK.lock().await;
    common::sweep_queues(&pool).await;
    let svc = MailQueueWriteService::new(pool.clone());
    let msg_id = seed_message(&pool).await;

    let mail_id = svc
        .enqueue(msg_id, "dest@example.com", None, None, None, None, None)
        .await
        .expect("enqueue");
    // A previous crashed run may have left its dispatch event behind — this test
    // asserts on exactly-what-this-run staged.
    sqlx::query("DELETE FROM messaging.outbox_events WHERE aggregate_id = $1")
        .bind(mail_id.to_string())
        .execute(&pool)
        .await
        .ok();

    let out = svc.process_queue(10, 1).await.expect("drain");
    assert_eq!(out.claimed, 1);

    // MAIL-M2 crash-safety: the claim pre-writes 'exception' BEFORE any dispatch.
    assert_eq!(mail_row_state(&pool, mail_id).await, "exception");

    // The dispatch request is staged in the SAME tx (durable exactly when the
    // row leaves the claimable set), on the bus envelope.
    let ev = sqlx::query(
        r#"SELECT payload FROM messaging.outbox_events
           WHERE event_type = 'MailDispatchRequested' AND aggregate_id = $1"#,
    )
    .bind(mail_id.to_string())
    .fetch_all(&pool)
    .await
    .expect("outbox rows");
    assert_eq!(ev.len(), 1);
    let payload: serde_json::Value = ev[0].get("payload");
    assert_eq!(payload["message"]["payload"]["email_to"], "dest@example.com");
    assert!(payload.get("channel").is_some(), "bus.bus envelope shape");

    // The SMTP consumer's callback completes the cycle (exception → sent legal:
    // mails.state is deliberately NOT monotonic-guarded).
    assert!(svc.mark_sent(mail_id).await.unwrap());
    assert_eq!(mail_row_state(&pool, mail_id).await, "sent");

    sqlx::query("DELETE FROM messaging.mails WHERE id = $1").bind(mail_id).execute(&pool).await.ok();
    sqlx::query("DELETE FROM messaging.outbox_events WHERE aggregate_id = ANY($1)")
        .bind(&[mail_id.to_string(), msg_id.to_string()])
        .execute(&pool).await.ok();
    sqlx::query("DELETE FROM messaging.mail_messages WHERE id = $1").bind(msg_id).execute(&pool).await.ok();
}

#[tokio::test]
async fn mark_failed_then_requeue_reenters_queue() {
    let Some(pool) = common::test_pool().await else {
        common::skipped("mail mark_failed + requeue");
        return;
    };
    let _drain_guard = common::DRAIN_LOCK.lock().await;
    common::sweep_queues(&pool).await;
    let svc = MailQueueWriteService::new(pool.clone());
    let msg_id = seed_message(&pool).await;
    let mail_id = svc
        .enqueue(msg_id, "dest2@example.com", None, None, None, None, None)
        .await
        .unwrap();

    // Claim → exception, then the consumer reports an SMTP failure (no-op on
    // an already-exception row is fine; the failure reason is what matters).
    svc.process_queue(10, 1).await.unwrap();
    assert!(svc
        .mark_failed(mail_id, "mail_smtp", Some("550 no such user"))
        .await
        .unwrap());

    let row = sqlx::query(
        "SELECT state::text AS state, failure_type::text AS ft FROM messaging.mails WHERE id = $1",
    )
    .bind(mail_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    let state: String = row.get("state");
    let ft: Option<String> = row.get("ft");
    assert_eq!(state, "exception");
    assert_eq!(ft.as_deref(), Some("mail_smtp"));

    // Manual resend: exception → outgoing (no automatic retry).
    assert!(svc.requeue(mail_id).await.unwrap());
    assert_eq!(mail_row_state(&pool, mail_id).await, "outgoing");

    // And it is claimable again.
    let out = svc.process_queue(10, 1).await.unwrap();
    assert_eq!(out.claimed, 1);

    sqlx::query("DELETE FROM messaging.mails WHERE id = $1").bind(mail_id).execute(&pool).await.ok();
    sqlx::query("DELETE FROM messaging.outbox_events WHERE aggregate_id = ANY($1)")
        .bind(&[mail_id.to_string(), msg_id.to_string()])
        .execute(&pool).await.ok();
    sqlx::query("DELETE FROM messaging.mail_messages WHERE id = $1").bind(msg_id).execute(&pool).await.ok();
}
