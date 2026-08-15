//! Increment-2 behavior proofs (live DB): chat set-equality dedup, monotonic
//! mark_as_read, single-dispatch on due schedules, webhook fail-closed +
//! replay, chatter ACL deny-then-install, MAIL-B8 GC, and SM-B13's bounded
//! requeue.

use std::sync::Arc;

use backbone_mail::application::service::{
    ChatterError, ChannelWriteService, GcService, MessageWriteService, ScheduleWriteService,
    SmsStatusWebhookService, WebhookError, WebhookOutcome,
};
use backbone_mail::application::service::chatter_acl::{
    MessagingIdentity, StaticThreadAccess, ThreadAclSlot,
};
use sqlx::Row;
use uuid::Uuid;

use super::common;

/// Seed one mail_message on a thread with an explicit `date` (ordering is the
/// point of these tests — NOW() defaults collapse inside one statement).
async fn seed_message(
    pool: &sqlx::PgPool,
    model: &str,
    res_id: Uuid,
    date: chrono::DateTime<chrono::Utc>,
    author: Option<Uuid>,
) -> Uuid {
    let id = Uuid::new_v4();
    sqlx::query(
        r#"INSERT INTO messaging.mail_messages (id, body, model, res_id, author_id, date)
           VALUES ($1, $2, $3, $4, $5, $6)"#,
    )
    .bind(id)
    .bind("seed body")
    .bind(model)
    .bind(res_id)
    .bind(author)
    .bind(date)
    .execute(pool)
    .await
    .expect("seed message");
    id
}

// ---------------------------------------------------------------------------
// get_or_create_chat: set-equality dedup (MAIL-M37)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn get_or_create_chat_is_idempotent_and_set_exact() {
    let Some(pool) = common::test_pool().await else {
        common::skipped("get_or_create_chat dedup");
        return;
    };
    let a = Uuid::new_v4();
    let b = Uuid::new_v4();
    let c = Uuid::new_v4();
    let svc = ChannelWriteService::new(pool.clone());

    let (ch1, created1) = svc.get_or_create_chat(a, b).await.expect("first chat");
    assert!(created1);
    // Same pair, reversed order → SAME channel, no new row.
    let (ch2, created2) = svc.get_or_create_chat(b, a).await.expect("reversed pair");
    assert_eq!(ch1, ch2, "member set is unordered");
    assert!(!created2);
    // A pair sharing ONE member is a DIFFERENT chat (set exactness).
    let (ch3, created3) = svc.get_or_create_chat(a, c).await.expect("overlapping pair");
    assert!(created3);
    assert_ne!(ch1, ch3);

    // Concurrent mints of the same pair converge on ONE channel. The G-MAIL-8
    // member uniques make the loser's mint FAIL (its tx rolls back) — the DB
    // is the dedup arbiter, so losers erroring is the CORRECT outcome; what
    // must hold is: no second channel ever exists for {a, b}.
    let handles: Vec<_> = (0..4)
        .map(|_| {
            let pool = pool.clone();
            tokio::spawn(async move {
                ChannelWriteService::new(pool).get_or_create_chat(a, b).await
            })
        })
        .collect();
    for h in handles {
        // Ok = the find path or the mint winner; Err = unique-violation on a
        // raced mint, rolled back (no fork). Both are correct dedup outcomes.
        if let Ok((id, _)) = h.await.expect("join") {
            assert_eq!(id, ch1, "a winner returns the existing chat");
        }
    }
    let chats: i64 = sqlx::query_scalar(
        r#"SELECT COUNT(*) FROM messaging.discuss_channels c
           WHERE c.channel_type = 'chat'
             AND (SELECT COUNT(*) FROM messaging.discuss_channel_members m
                  WHERE m.channel_id = c.id
                    AND (m.metadata->>'deleted_at') IS NULL
                    AND m.partner_id IN ($1, $2)) = 2"#,
    )
    .bind(a)
    .bind(b)
    .fetch_one(&pool)
    .await
    .expect("chat count");
    assert_eq!(chats, 1, "exactly one 1:1 chat for the pair after the race");

    common::cleanup(&pool, &[("discuss_channels", &[ch1, ch3])]).await;
}


// ---------------------------------------------------------------------------
// mark_as_read: monotonic advance + counter reset + concurrent single-advance
// ---------------------------------------------------------------------------

#[tokio::test]
async fn mark_as_read_is_monotonic_and_race_safe() {
    let Some(pool) = common::test_pool().await else {
        common::skipped("mark_as_read monotonic + race");
        return;
    };
    let a = Uuid::new_v4();
    let b = Uuid::new_v4();
    let (channel, _) = ChannelWriteService::new(pool.clone())
        .get_or_create_chat(a, b)
        .await
        .expect("chat");
    let now = chrono::Utc::now();
    let older = seed_message(&pool, "discuss.channel", channel, now - chrono::Duration::minutes(2), Some(b)).await;
    let newer = seed_message(&pool, "discuss.channel", channel, now, Some(b)).await;

    // Bump the unread counters as if b never read the two messages.
    sqlx::query(
        r#"UPDATE messaging.discuss_channel_members
           SET message_unread_counter = 2, unread_counter = 2
           WHERE channel_id = $1 AND partner_id = $2"#,
    )
    .bind(channel)
    .bind(a)
    .execute(&pool)
    .await
    .expect("bump counters");

    let member = backbone_mail::application::service::ChannelMemberWriteService::new(pool.clone());
    let identity = MessagingIdentity::User { partner_id: a };

    // Advance to the NEWER message: lands, counters reset.
    assert!(
        member
            .mark_as_read(channel, &identity, newer)
            .await
            .expect("advance"),
        "first advance to newest lands"
    );
    let row = sqlx::query(
        r#"SELECT seen_message_id, message_unread_counter, unread_counter
           FROM messaging.discuss_channel_members
           WHERE channel_id = $1 AND partner_id = $2"#,
    )
    .bind(channel)
    .bind(a)
    .fetch_one(&pool)
    .await
    .expect("member row");
    assert_eq!(row.get::<Uuid, _>("seen_message_id"), newer);
    assert_eq!(row.get::<i32, _>("message_unread_counter"), 0);
    assert_eq!(row.get::<i32, _>("unread_counter"), 0);

    // Rewind to the OLDER message: refused, pointer stays on newer.
    assert!(
        !member
            .mark_as_read(channel, &identity, older)
            .await
            .expect("rewind attempt"),
        "rewind is a no-op"
    );
    let seen: Uuid = sqlx::query_scalar(
        r#"SELECT seen_message_id FROM messaging.discuss_channel_members
           WHERE channel_id = $1 AND partner_id = $2"#,
    )
    .bind(channel)
    .bind(a)
    .fetch_one(&pool)
    .await
    .expect("pointer");
    assert_eq!(seen, newer, "monotonic — no rewind");

    // Concurrent advances to a NEW message: exactly ONE true + ONE event.
    let race_target =
        seed_message(&pool, "discuss.channel", channel, now + chrono::Duration::minutes(1), Some(b)).await;
    let mut trues = 0usize;
    let mut errors = 0usize;
    let mut handles = Vec::new();
    for _ in 0..8 {
        handles.push(tokio::spawn({
            let pool = pool.clone();
            async move {
                backbone_mail::application::service::ChannelMemberWriteService::new(pool)
                    .mark_as_read(channel, &identity, race_target)
                    .await
            }
        }));
    }
    for h in handles {
        match h.await.expect("join") {
            Ok(true) => trues += 1,
            Ok(false) => {}
            // SKIP LOCKED loser inside another tx's window — the race loser.
            Err(_) => errors += 1,
        }
    }
    assert_eq!(trues, 1, "one advance wins; errors={errors} are lock-skips");
    let events = sqlx::query_scalar::<_, i64>(
        r#"SELECT COUNT(*) FROM messaging.outbox_events
           WHERE event_type = 'MemberSeenAdvanced'
             AND payload->'message'->'payload'->>'channel_id' = $1"#,
    )
    .bind(channel.to_string())
    .fetch_one(&pool)
    .await
    .expect("event count");
    assert_eq!(events, 2, "one seen-advance event per advanced message");

    common::cleanup(&pool, &[("discuss_channels", &[channel])]).await;
    common::cleanup(&pool, &[("mail_messages", &[older, newer, race_target])]).await;
}

// ---------------------------------------------------------------------------
// dispatch_due_scheduled: SKIP LOCKED single-dispatch under concurrency
// ---------------------------------------------------------------------------

#[tokio::test]
async fn dispatch_due_scheduled_posts_exactly_once_under_concurrency() {
    let Some(pool) = common::test_pool().await else {
        common::skipped("schedule single-dispatch");
        return;
    };
    let model = format!("test.doc.{}", Uuid::new_v4().simple());
    let res_id = Uuid::new_v4();
    let author = Uuid::new_v4();
    let acl: Arc<dyn backbone_mail::application::service::chatter_acl::ThreadAccessResolver> =
        Arc::new(StaticThreadAccess { open_models: vec![model.clone()] });

    // Seed past-due rows directly (arm_scheduled_message rejects past dates
    // by design — the dispatch path only ever sees genuinely-late rows).
    let due = now_past_due_rows(&pool, &model, res_id, author, 3).await;

    let svc = Arc::new(ScheduleWriteService::new(pool.clone(), Arc::clone(&acl)));
    let poster = Arc::new(MessageWriteService::new(pool.clone()));
    let mut handles = Vec::new();
    for _ in 0..3 {
        handles.push(tokio::spawn({
            let svc = Arc::clone(&svc);
            let poster = Arc::clone(&poster);
            async move { svc.dispatch_due_scheduled(&poster).await }
        }));
    }
    let mut total_posted = 0usize;
    for h in handles {
        let (posted, _skipped) = h.await.expect("join").expect("dispatch");
        total_posted += posted;
    }
    assert_eq!(total_posted, 3, "each due row dispatched EXACTLY once");

    // The rows are gone (delete-on-dispatch) and each body landed once.
    let left: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM messaging.mail_scheduled_messages WHERE id = ANY($1)",
    )
    .bind(&due)
    .fetch_one(&pool)
    .await
    .expect("leftover count");
    assert_eq!(left, 0, "dispatched rows are deleted on the claiming tx");
    let posted_bodies: i64 = sqlx::query_scalar(
        r#"SELECT COUNT(*) FROM messaging.mail_messages
           WHERE model = $1 AND res_id = $2 AND body LIKE 'sched-%'"#,
    )
    .bind(&model)
    .bind(res_id)
    .fetch_one(&pool)
    .await
    .expect("posted count");
    assert_eq!(posted_bodies, 3, "no double-post past the SKIP LOCKED claim");

    // Cleanup: the posted messages + their notifications.
    let posted_ids: Vec<Uuid> = sqlx::query_scalar(
        r#"SELECT id FROM messaging.mail_messages WHERE model = $1 AND res_id = $2"#,
    )
    .bind(&model)
    .bind(res_id)
    .fetch_all(&pool)
    .await
    .expect("posted ids");
    sqlx::query("DELETE FROM messaging.mail_notifications WHERE mail_message_id = ANY($1)")
        .bind(&posted_ids)
        .execute(&pool)
        .await
        .ok();
    common::cleanup(&pool, &[("mail_messages", &posted_ids)]).await;
}

async fn now_past_due_rows(
    pool: &sqlx::PgPool,
    model: &str,
    res_id: Uuid,
    author: Uuid,
    n: usize,
) -> Vec<Uuid> {
    let past = chrono::Utc::now() - chrono::Duration::hours(1);
    let mut ids = Vec::new();
    for i in 0..n {
        let id = Uuid::new_v4();
        sqlx::query(
            r#"INSERT INTO messaging.mail_scheduled_messages
               (id, body, scheduled_date, model, res_id, author_party_id)
               VALUES ($1, $2, $3, $4, $5, $6)"#,
        )
        .bind(id)
        .bind(format!("sched-{i}"))
        .bind(past)
        .bind(model)
        .bind(res_id)
        .bind(author)
        .execute(pool)
        .await
        .expect("seed due row");
        ids.push(id);
    }
    ids
}

// ---------------------------------------------------------------------------
// SMS webhook: fail-closed no-write, valid advance, replay
// ---------------------------------------------------------------------------

/// Per-row no-write snapshot for OUR sms: (state, error_message, metadata).
/// Global table counts are useless on the shared test DB (concurrent tests
/// write between snapshots) — these invariants are race-free AND stronger:
/// they prove OUR row was not touched, not merely that some table didn't move.
async fn snapshot_row(pool: &sqlx::PgPool, sms_id: Uuid) -> (String, Option<String>, serde_json::Value) {
    sqlx::query(
        "SELECT state::text AS state, error_message, metadata FROM messaging.sms WHERE id = $1",
    )
    .bind(sms_id)
    .fetch_one(pool)
    .await
    .map(|r| (r.get("state"), r.get("error_message"), r.get("metadata")))
    .expect("snapshot row")
}

#[tokio::test]
async fn webhook_fail_closed_and_replay_semantics() {
    let Some(pool) = common::test_pool().await else {
        common::skipped("webhook fail-closed + replay");
        return;
    };
    let _drain_guard = common::DRAIN_LOCK.lock().await;
    common::sweep_queues(&pool).await;
    let secret = "test-webhook-secret";
    let svc = SmsStatusWebhookService::new(pool.clone(), secret);
    let sms_uuid = format!("whk-{}", Uuid::new_v4().simple());
    let sms_id = seed_sms(&pool, &sms_uuid, "process").await;

    // ---- Fail-closed: bad signature touches NOTHING. ----
    let before = snapshot_row(&pool, sms_id).await;
    let raw = format!(
        r#"{{"timestamp":"{}","sms_uuid":"{sms_uuid}","status":"sent"}}"#,
        chrono::Utc::now().to_rfc3339()
    );
    let wrong_sig = hex_sig(b"not-the-secret", raw.as_bytes());
    let err = svc.handle(raw.as_bytes(), Some(&wrong_sig)).await.unwrap_err();
    assert!(matches!(err, WebhookError::Verify(m) if m.contains("signature")));
    assert_eq!(before, snapshot_row(&pool, sms_id).await, "ZERO writes on bad signature");
    let staged: i64 = sqlx::query_scalar(
        r#"SELECT COUNT(*) FROM messaging.outbox_events
           WHERE aggregate_id = $1"#,
    )
    .bind(sms_id.to_string())
    .fetch_one(&pool)
    .await
    .expect("staged count");
    assert_eq!(staged, 0, "no bus event staged for an unverified callback");

    // ---- Valid signature advances through the public seam. ----
    let good_sig = hex_sig(secret.as_bytes(), raw.as_bytes());
    assert_eq!(
        svc.handle(raw.as_bytes(), Some(&good_sig)).await.unwrap(),
        WebhookOutcome::Advanced
    );
    let state: String =
        sqlx::query_scalar("SELECT state::text FROM messaging.sms WHERE id = $1")
            .bind(sms_id)
            .fetch_one(&pool)
            .await
            .expect("state");
    assert_eq!(state, "sent");

    // ---- Replay: provider retries are idempotent no-ops. ----
    assert_eq!(
        svc.handle(raw.as_bytes(), Some(&good_sig)).await.unwrap(),
        WebhookOutcome::Replay
    );

    sqlx::query("DELETE FROM messaging.sms WHERE id = $1")
        .bind(sms_id)
        .execute(&pool)
        .await
        .ok();
}

fn hex_sig(secret: &[u8], body: &[u8]) -> String {
    use hmac::Mac;
    let mut mac = hmac::Hmac::<sha2::Sha256>::new_from_slice(secret).unwrap();
    mac.update(body);
    mac.finalize().into_bytes().iter().map(|b| format!("{b:02x}")).collect()
}

async fn seed_sms(pool: &sqlx::PgPool, sms_uuid: &str, state: &str) -> Uuid {
    let id = Uuid::new_v4();
    sqlx::query(
        r#"INSERT INTO messaging.sms (id, uuid, number, body, state)
           VALUES ($1, $2, '+628110000000', 'webhook proof', $3::sms_state)"#,
    )
    .bind(id)
    .bind(sms_uuid)
    .bind(state)
    .execute(pool)
    .await
    .expect("seed sms");
    id
}

// ---------------------------------------------------------------------------
// Chatter ACL: deny-by-default, install-opens (MAIL-B1/B16, ThreadAclSlot)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn chatter_acl_denies_until_resolver_installed() {
    let Some(pool) = common::test_pool().await else {
        common::skipped("chatter ACL deny/install");
        return;
    };
    let model = format!("crm.lead.{}", Uuid::new_v4().simple());
    let res_id = Uuid::new_v4();
    let partner = Uuid::new_v4();
    let identity = MessagingIdentity::User { partner_id: partner };

    // The slot (default DenyHostDocs) is the resolver the services are built
    // over — installing must open the gate WITHOUT rebuilding anything.
    let slot = ThreadAclSlot::default();
    let chatter = backbone_mail::application::service::ThreadChatterService::new(
        pool.clone(),
        Arc::new(slot.clone()),
    );

    // Default: host-document chatter denied.
    let denied = chatter
        .post(&identity, &model, res_id, "hello?", None, false)
        .await
        .unwrap_err();
    assert!(matches!(denied, ChatterError::CannotPost(_, _)), "deny-by-default");

    // The host registers — the next call goes through.
    slot.install(Arc::new(StaticThreadAccess { open_models: vec![model.clone()] }));
    let posted = chatter
        .post(&identity, &model, res_id, "hello after install", None, false)
        .await
        .expect("post after install");
    let thread = chatter
        .read_thread(&identity, &model, res_id, None)
        .await
        .expect("read after install");
    assert!(thread.iter().any(|m| m.id == posted.message_id));

    // A DIFFERENT model stays denied (allowlist is per-model).
    let other = format!("other.doc.{}", Uuid::new_v4().simple());
    assert!(matches!(
        chatter.post(&identity, &other, res_id, "x", None, false).await.unwrap_err(),
        ChatterError::CannotPost(_, _)
    ));

    sqlx::query("DELETE FROM messaging.mail_notifications WHERE mail_message_id = $1")
        .bind(posted.message_id)
        .execute(&pool)
        .await
        .ok();
    common::cleanup(&pool, &[("mail_messages", &[posted.message_id])]).await;
}

// ---------------------------------------------------------------------------
// GC: MAIL-B8 notification reap-by-age; SM-B13 bounded requeue
// ---------------------------------------------------------------------------

#[tokio::test]
async fn gc_reaps_old_notifications_and_bounds_stuck_requeue() {
    let Some(pool) = common::test_pool().await else {
        common::skipped("gc notification + stuck sweep");
        return;
    };
    let gc = GcService::new(pool.clone(), 30, 12, 60, 30);

    // MAIL-B8: an old notification dies, a fresh one survives. The audit
    // triggers stamp NOW() on every insert/update, so backdating happens with
    // them disabled inside a tx (the ALTER's ACCESS EXCLUSIVE lock makes the
    // window invisible to concurrent inserts).
    let fresh = seed_notification(&pool).await;
    let old = seed_notification(&pool).await;
    let mut tx = pool.begin().await.expect("backdate tx");
    sqlx::query("ALTER TABLE messaging.mail_notifications DISABLE TRIGGER mail_notifications_insert_audit")
        .execute(&mut *tx).await.expect("disable insert trigger");
    sqlx::query("ALTER TABLE messaging.mail_notifications DISABLE TRIGGER mail_notifications_update_audit")
        .execute(&mut *tx).await.expect("disable update trigger");
    sqlx::query(
        r#"UPDATE messaging.mail_notifications
           SET metadata = jsonb_set('{}'::jsonb, '{created_at}',
                                    to_jsonb($2::timestamptz))
           WHERE id = $1"#,
    )
    .bind(old)
    .bind(chrono::Utc::now() - chrono::Duration::days(40))
    .execute(&mut *tx)
    .await
    .expect("backdate");
    tx.commit().await.expect("commit backdate");

    let reaped = gc.notification_gc().await.expect("notification gc");
    assert!(reaped >= 1, "the old row is reaped");
    let survivors: Vec<Uuid> = sqlx::query_scalar(
        "SELECT id FROM messaging.mail_notifications WHERE id = ANY($1)",
    )
    .bind(vec![fresh, old])
    .fetch_all(&pool)
    .await
    .expect("survivors");
    assert_eq!(survivors, vec![fresh], "fresh survives, old reaped (reaped={reaped})");

    // SM-B13: a stuck 'process' row is re-queued once — and never again.
    // The sweep acts on ALL stuck rows (ops semantics) — hold the drain lock
    // so it can't requeue a row a concurrent drain test is mid-flight on.
    let _drain_guard = common::DRAIN_LOCK.lock().await;
    // Backdating needs the audit triggers off (they stamp updated_at=NOW on
    // every update); inside a tx the ALTER's lock serializes against other
    // writers for the window.
    let sms_uuid = format!("stuck-{}", Uuid::new_v4().simple());
    let sms_id = seed_sms(&pool, &sms_uuid, "process").await;
    let mut tx = pool.begin().await.expect("sms backdate tx");
    sqlx::query("ALTER TABLE messaging.sms DISABLE TRIGGER sms_insert_audit")
        .execute(&mut *tx).await.expect("disable insert trigger");
    sqlx::query("ALTER TABLE messaging.sms DISABLE TRIGGER sms_update_audit")
        .execute(&mut *tx).await.expect("disable update trigger");
    sqlx::query(
        r#"UPDATE messaging.sms
           SET metadata = jsonb_set(metadata, '{updated_at}', to_jsonb($2::timestamptz))
           WHERE id = $1"#,
    )
    .bind(sms_id)
    .bind(chrono::Utc::now() - chrono::Duration::hours(2))
    .execute(&mut *tx)
    .await
    .expect("backdate sms");
    tx.commit().await.expect("commit sms backdate");

    let (stuck, requeued) = gc.sweep_stuck_process().await.expect("first sweep");
    assert!(stuck.contains(&sms_id), "our row is among the stuck: {stuck:?}");
    assert!(requeued >= 1);
    let post_sweep = sqlx::query(
        "SELECT state::text AS state, (metadata->>'swept_at') IS NOT NULL AS swept FROM messaging.sms WHERE id = $1",
    )
    .bind(sms_id)
    .fetch_one(&pool)
    .await
    .expect("post-sweep row");
    let (state, swept): (String, bool) = (post_sweep.get("state"), post_sweep.get("swept"));
    assert_eq!((state.as_str(), swept), ("outgoing", true), "re-queued + marked");

    // Simulate the re-queued row being claimed and stuck AGAIN — the marker
    // is the bound: detected, but never re-queued a second time.
    sqlx::query(
        r#"UPDATE messaging.sms SET state = 'process',
             metadata = jsonb_set(metadata, '{updated_at}', to_jsonb($2::timestamptz))
           WHERE id = $1"#,
    )
    .bind(sms_id)
    .bind(chrono::Utc::now() - chrono::Duration::hours(2))
    .execute(&pool)
    .await
    .expect("re-stick");
    let (stuck2, requeued2) = gc.sweep_stuck_process().await.expect("second sweep");
    assert!(stuck2.contains(&sms_id), "still detected for alerting");
    assert_eq!(requeued2, 0, "swept_at marker bounds the retry (SM-B13)");
    // The alert event is staged on every detecting sweep.
    let alerts: i64 = sqlx::query_scalar(
        r#"SELECT COUNT(*) FROM messaging.outbox_events
           WHERE event_type = 'SmsStuckProcessDetected'
             AND payload->'message'->'payload'->>'sms_ids' LIKE $1"#,
    )
    .bind(format!("%{sms_id}%"))
    .fetch_one(&pool)
    .await
    .expect("alert events");
    assert!(alerts >= 2, "alert fires on every sweep: {alerts}");

    sqlx::query("DELETE FROM messaging.sms WHERE id = $1")
        .bind(sms_id)
        .execute(&pool)
        .await
        .ok();
    sqlx::query("DELETE FROM messaging.mail_notifications WHERE id = $1")
        .bind(fresh)
        .execute(&pool)
        .await
        .ok();
}

async fn seed_notification(pool: &sqlx::PgPool) -> Uuid {
    let id = Uuid::new_v4();
    let msg = seed_message(
        pool,
        "test.gc.doc",
        Uuid::new_v4(),
        chrono::Utc::now(),
        None,
    )
    .await;
    sqlx::query(
        r#"INSERT INTO messaging.mail_notifications
               (id, mail_message_id, res_partner_id, notification_type, notification_status)
           VALUES ($1, $2, $3, 'inbox', 'sent')"#,
    )
    .bind(id)
    .bind(msg)
    .bind(Uuid::new_v4())
    .execute(pool)
    .await
    .expect("seed notification");
    id
}
