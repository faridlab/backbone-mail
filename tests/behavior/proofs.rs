//! The two port proofs:
//!
//! 1. **SM-B6 (fixed at port)** — the monotonic status lattice is enforced AT
//!    THE DB (trigger, ADR-0015): an out-of-order write fails even through raw
//!    SQL, which is exactly the raw-SQL-reachable hole Odoo's Python-side
//!    `notifications_statuses_to_ignore` guard leaves open.
//! 2. **MMB-4 / ADR-0020 pickup-lock** — two concurrent drainers over one
//!    queue claim DISJOINT sets (FOR UPDATE SKIP LOCKED): every row is sent
//!    exactly once, never twice, never zero times.

use std::collections::HashSet;

use backbone_mail::application::service::{NoopSmsApi, SmsWriteService};
use uuid::Uuid;

use super::common;

#[tokio::test]
async fn proof_monotonic_guard_rejects_regression_at_the_db() {
    let Some(pool) = common::test_pool().await else {
        common::skipped("SM-B6 monotonic-guard proof");
        return;
    };

    // --- sms.state: sent → pending must fail ---------------------------------
    let id = Uuid::new_v4();
    sqlx::query("INSERT INTO messaging.sms (id, uuid, number, body) VALUES ($1, $2, $3, $4)")
        .bind(id)
        .bind(id.simple().to_string())
        .bind("+6281200000201")
        .bind("guard proof")
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("UPDATE messaging.sms SET state = 'sent' WHERE id = $1")
        .bind(id)
        .execute(&pool)
        .await
        .unwrap();

    let regression = sqlx::query("UPDATE messaging.sms SET state = 'pending' WHERE id = $1")
        .bind(id)
        .execute(&pool)
        .await;
    assert!(
        regression.is_err(),
        "SM-B6: sent → pending MUST fail at the DB, got {regression:?}"
    );
    if let Err(e) = regression {
        assert!(e.to_string().contains("monotonic"), "the trigger's message: {e}");
    }

    // Idempotent same-value rewrite stays legal (a replayed webhook is a no-op).
    sqlx::query("UPDATE messaging.sms SET state = 'sent' WHERE id = $1")
        .bind(id)
        .execute(&pool)
        .await
        .expect("same-rank rewrite allowed");

    // --- mail_notifications.notification_status: terminal-once-set ------------
    let msg = Uuid::new_v4();
    sqlx::query("INSERT INTO messaging.mail_messages (id, body) VALUES ($1, $2)")
        .bind(msg)
        .bind("guard proof body")
        .execute(&pool)
        .await
        .unwrap();
    let nid = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO messaging.mail_notifications (id, mail_message_id, notification_status)
         VALUES ($1, $2, 'sent')",
    )
    .bind(nid)
    .bind(msg)
    .execute(&pool)
    .await
    .unwrap();

    let regression = sqlx::query(
        "UPDATE messaging.mail_notifications SET notification_status = 'ready' WHERE id = $1",
    )
    .bind(nid)
    .execute(&pool)
    .await;
    assert!(
        regression.is_err(),
        "SM-B6: sent → ready MUST fail at the DB (MAIL-M3 origin of the inversion)"
    );

    let terminal = sqlx::query(
        "UPDATE messaging.mail_notifications SET notification_status = 'exception' WHERE id = $1",
    )
    .bind(nid)
    .execute(&pool)
    .await
    .unwrap();
    let _ = terminal;
    // exception is terminal-once-set: exception → sent refuses.
    let after_terminal = sqlx::query(
        "UPDATE messaging.mail_notifications SET notification_status = 'sent' WHERE id = $1",
    )
    .bind(nid)
    .execute(&pool)
    .await;
    assert!(
        after_terminal.is_err(),
        "SM-B6: terminal exception stays terminal (rank 100 floor)"
    );

    sqlx::query("DELETE FROM messaging.sms WHERE id = $1").bind(id).execute(&pool).await.ok();
    sqlx::query("DELETE FROM messaging.mail_notifications WHERE id = $1").bind(nid).execute(&pool).await.ok();
    sqlx::query("DELETE FROM messaging.mail_messages WHERE id = $1").bind(msg).execute(&pool).await.ok();
}

#[tokio::test]
async fn proof_concurrent_drainers_claim_disjoint_sets() {
    let Some(pool) = common::test_pool().await else {
        common::skipped("MMB-4 concurrent-drainer proof");
        return;
    };
    let _drain_guard = common::DRAIN_LOCK.lock().await;
    common::sweep_queues(&pool).await;
    const N: usize = 12;

    let svc = SmsWriteService::new(pool.clone());
    let mut ids = Vec::with_capacity(N);
    for i in 0..N {
        let (id, _uuid) = svc
            .enqueue(&format!("+62812000003{:02}", i), "concurrent drain proof", None, None, None, None)
            .await
            .expect("enqueue");
        ids.push(id);
    }

    // TWO drainers racing over one queue, sharing ONE provider double that
    // records every request it sees.
    let provider = std::sync::Arc::new(NoopSmsApi::accepting());
    let a = svc.process_queue(provider.as_ref(), 5, 0);
    let b = svc.process_queue(provider.as_ref(), 5, 0);
    let (ra, rb) = tokio::join!(a, b);
    let (ra, rb) = (ra.expect("drainer a"), rb.expect("drainer b"));

    // Together they drained everything…
    assert_eq!(ra.claimed + rb.claimed, N, "every row drained: {ra:?} + {rb:?}");
    // …and the provider saw each row EXACTLY once (disjoint claims, no double-send).
    let seen = provider.requests();
    assert_eq!(seen.len(), N, "no double-send: {seen:?}");
    let unique: HashSet<&str> = seen.iter().map(|r| r.uuid.as_str()).collect();
    assert_eq!(unique.len(), N, "every claimed uuid distinct");

    // Every row left 'outgoing'.
    let stuck: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM messaging.sms WHERE id = ANY($1) AND state = 'outgoing'",
    )
    .bind(&ids)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(stuck, 0, "no row left unclaimed (no zero-send)");

    let uuids: Vec<String> = ids.iter().map(|i| i.simple().to_string()).collect();
    sqlx::query("DELETE FROM messaging.sms_trackers WHERE sms_uuid = ANY($1)")
        .bind(&uuids).execute(&pool).await.ok();
    sqlx::query("DELETE FROM messaging.sms WHERE id = ANY($1)")
        .bind(&ids).execute(&pool).await.ok();
    sqlx::query("DELETE FROM messaging.outbox_events WHERE aggregate_id = ANY($1)")
        .bind(&ids.iter().map(|u| u.to_string()).collect::<Vec<_>>())
        .execute(&pool).await.ok();
}
