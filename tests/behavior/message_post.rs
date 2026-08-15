//! message_post (MAIL-M16/B): one tx mints the message + per-channel
//! notification fan-out + the queue rows, and stages the bus events.

use backbone_mail::application::service::{
    MessagePostCommand, MessageWriteService, NotificationChannel, PostRecipient,
};
use sqlx::Row;
use uuid::Uuid;

use super::common;

#[tokio::test]
async fn message_post_fans_out_per_channel_and_stages_events() {
    let Some(pool) = common::test_pool().await else {
        common::skipped("message_post fan-out");
        return;
    };
    // message_post mints a claimable sms row — hold the drainer lock so a
    // concurrent drain can't steal it before this test's own assertions.
    let _drain_guard = common::DRAIN_LOCK.lock().await;
    let subtype_id = common::seed_subtype(&pool, "test-note").await;
    let model = format!("test.doc.{}", Uuid::new_v4().simple());
    let res_id = Uuid::new_v4();
    let p_inbox = Uuid::new_v4();
    let p_email = Uuid::new_v4();
    let p_sms = Uuid::new_v4();

    let posted = MessageWriteService::new(pool.clone())
        .message_post(MessagePostCommand {
            body: "hello chatter".into(),
            subject: Some("subject lives on mail_message".into()),
            message_type: "comment".into(),
            subtype_id: Some(subtype_id),
            model: Some(model.clone()),
            res_id: Some(res_id),
            recipients: vec![
                PostRecipient { res_partner_id: Some(p_inbox), channel: NotificationChannel::Inbox, email: None, number: None },
                PostRecipient { res_partner_id: Some(p_email), channel: NotificationChannel::Email, email: Some("a@example.com".into()), number: None },
                PostRecipient { res_partner_id: Some(p_sms), channel: NotificationChannel::Sms, email: None, number: Some("+6281200000001".into()) },
            ],
            ..Default::default()
        })
        .await
        .expect("message_post");

    // Fan-out shapes.
    assert_eq!(posted.notifications.len(), 3, "one notification per recipient");
    assert!(posted.mail_id.is_some(), "email group mints ONE mail row");
    let (sms_id, sms_uuid) = posted.sms[0].clone();
    assert!(!sms_uuid.is_empty());

    // MAIL-M3/SM-F19 channel-decided initial statuses.
    let st: Vec<(String, String)> = posted
        .notifications
        .iter()
        .map(|m| (m.notification_type.as_str().to_string(), m.notification_status.clone()))
        .collect();
    assert!(st.contains(&("inbox".into(), "sent".into())), "inbox is instant-sent: {st:?}");
    assert!(st.contains(&("email".into(), "ready".into())), "email starts ready: {st:?}");
    assert!(st.contains(&("sms".into(), "ready".into())), "sms starts ready: {st:?}");

    // Queue rows on disk.
    let mail_row = sqlx::query(
        "SELECT email_to, state::text AS state FROM messaging.mails WHERE id = $1",
    )
    .bind(posted.mail_id.unwrap())
    .fetch_one(&pool)
    .await
    .expect("mail row");
    assert_eq!(mail_row.get::<String, _>("email_to"), "a@example.com");
    assert_eq!(mail_row.get::<String, _>("state"), "outgoing");

    let sms_row = sqlx::query("SELECT state::text AS state, number FROM messaging.sms WHERE id = $1")
        .bind(sms_id)
        .fetch_one(&pool)
        .await
        .expect("sms row");
    assert_eq!(sms_row.get::<String, _>("state"), "outgoing");
    assert_eq!(sms_row.get::<String, _>("number"), "+6281200000001");

    // SM-M21: the uuid-correlated tracker row (no FK, ON CONFLICT survives GC).
    let tracker = sqlx::query(
        "SELECT state::text AS state FROM messaging.sms_trackers WHERE sms_uuid = $1::text",
    )
    .bind(&sms_uuid)
    .fetch_one(&pool)
    .await
    .expect("tracker row");
    assert_eq!(tracker.get::<String, _>("state"), "process");

    // The bus envelope (bus.bus shape) staged in-tx: MessagePosted (aggregate =
    // the message) + SmsCreated (aggregate = the sms row — TR-SM-1).
    let ev = sqlx::query(
        r#"SELECT payload FROM messaging.outbox_events
           WHERE event_type IN ('MessagePosted', 'SmsCreated') AND aggregate_id = ANY($1)
           ORDER BY event_type"#,
    )
    .bind(vec![posted.message_id.to_string(), sms_id.to_string()])
    .fetch_all(&pool)
    .await
    .expect("outbox rows");
    assert_eq!(ev.len(), 2, "MessagePosted + one SmsCreated per sms row");
    for row in ev {
        let payload: serde_json::Value = row.get("payload");
        assert!(payload.get("channel").is_some(), "envelope carries a channel key");
        assert!(payload.get("message").is_some(), "bus.bus envelope shape");
    }

    // Cleanup.
    let ids = vec![posted.message_id];
    sqlx::query("DELETE FROM messaging.sms_trackers WHERE sms_uuid = ANY($1)")
        .bind(&[sms_uuid.as_str()])
        .execute(&pool)
        .await
        .ok();
    common::cleanup(&pool, &[("mail_messages", &ids)]).await;
    // Notifications / mail / sms / followers cascade by hand (no FKs by design).
    sqlx::query("DELETE FROM messaging.mail_notifications WHERE mail_message_id = ANY($1)")
        .bind(&ids).execute(&pool).await.ok();
    sqlx::query("DELETE FROM messaging.sms WHERE id = $1").bind(sms_id).execute(&pool).await.ok();
    sqlx::query("DELETE FROM messaging.outbox_events WHERE aggregate_id = ANY($1)")
        .bind(&ids.iter().map(|u| u.to_string()).collect::<Vec<_>>())
        .execute(&pool).await.ok();
    sqlx::query("DELETE FROM messaging.mail_message_subtypes WHERE id = $1")
        .bind(subtype_id).execute(&pool).await.ok();
}
