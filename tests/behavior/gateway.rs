//! Increment-3 gateway behavior + route proofs (live DB): the
//! `_find_mail_server` selection ladder (MAIL-M26), the queue send path
//! through the MailApiPort, the inbound pipeline's fail-closed ordering
//! (token consteq with ZERO writes before auth), replay dedup (MAIL-B6),
//! the allowlist (MAIL-M28), routing (reply collation), sms_gc's terminal-only
//! reap, and the inbound route's verdict shapes (no server-existence oracle).

use std::sync::Arc;

use backbone_mail::application::service::{
    InboundMessage, InboundOutcome, MailInboundError, MailInboundService, MailQueueWriteService,
    MailServerQueryService, NoopMailApi,
};
use backbone_mail::application::service::GcService;
use backbone_mail::infrastructure::persistence::smtp_selection_repository::SmtpSelectionRepository;
use backbone_mail::MessagingModule;
use sha2::{Digest, Sha256};
use sqlx::Row;
use tower::ServiceExt;
use uuid::Uuid;

use super::common;

fn sha256_hex(input: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(input.as_bytes());
    let digest = hasher.finalize();
    let mut out = String::with_capacity(64);
    for byte in digest {
        use std::fmt::Write as _;
        let _ = write!(out, "{byte:02x}");
    }
    out
}

// ---------------------------------------------------------------------------
// MAIL-M26 — the selection ladder
// ---------------------------------------------------------------------------

/// The ladder scans the whole mail_servers table, so the two ladder tests
/// serialize against each other AND sweep leftovers from prior panicked runs.
static MAIL_SERVER_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

async fn sweep_mail_servers(pool: &sqlx::PgPool) {
    sqlx::query("DELETE FROM messaging.mail_servers WHERE name = ANY($1)")
        .bind(&[
            "exact".to_string(),
            "domain".to_string(),
            "wild-hi".to_string(),
            "wild-lo".to_string(),
            "mixed-case".to_string(),
        ])
        .execute(pool)
        .await
        .ok();
}


async fn seed_mail_server(
    pool: &sqlx::PgPool,
    name: &str,
    from_filter: Option<&str>,
    sequence: i32,
) -> Uuid {
    let id = Uuid::new_v4();
    sqlx::query(
        r#"INSERT INTO messaging.mail_servers (id, name, from_filter, smtp_host, sequence)
           VALUES ($1, $2, $3, 'smtp.test', $4)"#,
    )
    .bind(id)
    .bind(name)
    .bind(from_filter)
    .bind(sequence)
    .execute(pool)
    .await
    .expect("seed mail_server");
    id
}

#[tokio::test]
async fn selection_ladder_exact_then_domain_then_wildcard_then_sequence() {
    let Some(pool) = common::test_pool().await else {
        common::skipped("MAIL-M26 selection ladder");
        return;
    };
    let _guard = MAIL_SERVER_LOCK.lock().await;
    sweep_mail_servers(&pool).await;
    let svc = MailServerQueryService::new(pool.clone());

    let exact = seed_mail_server(&pool, "exact", Some("ceo@corp.example"), 90).await;
    let domain = seed_mail_server(&pool, "domain", Some("@corp.example"), 80).await;
    let wildcard_hi = seed_mail_server(&pool, "wild-hi", None, 50).await;
    let wildcard_lo = seed_mail_server(&pool, "wild-lo", None, 5).await;

    // Rung 0: exact beats domain beats both wildcards, regardless of sequence.
    let pick = svc.resolve_endpoint("ceo@corp.example").await.unwrap().expect("a server");
    assert_eq!(pick.server_id, exact);

    // Rung 1: without the exact server, the domain entry wins.
    sqlx::query("UPDATE messaging.mail_servers SET active = false WHERE id = $1")
        .bind(exact)
        .execute(&pool)
        .await
        .unwrap();
    let pick = svc.resolve_endpoint("anyone@corp.example").await.unwrap().expect("a server");
    assert_eq!(pick.server_id, domain);

    // Rung 2 + tie-break: domain gone → the LOWEST-sequence wildcard.
    sqlx::query("UPDATE messaging.mail_servers SET active = false WHERE id = $1")
        .bind(domain)
        .execute(&pool)
        .await
        .unwrap();
    let pick = svc.resolve_endpoint("anyone@corp.example").await.unwrap().expect("a server");
    assert_eq!(pick.server_id, wildcard_lo);
    let _ = wildcard_hi;

    // An address no from_filter covers but a wildcard does → still routable;
    // an EMPTY pool (all archived) → None (the caller's mail_server failure).
    sqlx::query("UPDATE messaging.mail_servers SET active = false WHERE id = ANY($1)")
        .bind([wildcard_lo, wildcard_hi])
        .execute(&pool)
        .await
        .unwrap();
    assert!(svc.resolve_endpoint("x@y.z").await.unwrap().is_none());

    // A domainless address never reaches the ladder at all.
    assert!(svc.resolve_endpoint("no-at-sign").await.is_err());

    sqlx::query("DELETE FROM messaging.mail_servers WHERE id = ANY($1)")
        .bind([exact, domain, wildcard_lo, wildcard_hi])
        .execute(&pool)
        .await
        .ok();
}

/// The repository-level rung match is also provable without the query service
/// (case-insensitivity on both sides — the documented normalization).
#[tokio::test]
async fn selection_ladder_matches_case_insensitively() {
    let Some(pool) = common::test_pool().await else {
        common::skipped("MAIL-M26 case-insensitive from_filter");
        return;
    };
    let _guard = MAIL_SERVER_LOCK.lock().await;
    sweep_mail_servers(&pool).await;
    let id = seed_mail_server(&pool, "mixed-case", Some(" CEO@Corp.Example , @Other.Domain "), 10).await;
    let mut conn = pool.acquire().await.unwrap();
    let pick = SmtpSelectionRepository::resolve_endpoint(&mut conn, "ceo", "corp.example")
        .await
        .unwrap()
        .expect("rung 0 hit despite case + padding");
    assert_eq!(pick.server_id, id);
    drop(conn);
    sqlx::query("DELETE FROM messaging.mail_servers WHERE id = $1")
        .bind(id)
        .execute(&pool)
        .await
        .ok();
}

// ---------------------------------------------------------------------------
// The queue send path through the port (success leg — the failing leg lives
// in mail_queue.rs)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn queue_send_via_port_marks_sent() {
    let Some(pool) = common::test_pool().await else {
        common::skipped("mail queue send via MailApiPort");
        return;
    };
    let _drain_guard = common::DRAIN_LOCK.lock().await;
    common::sweep_queues(&pool).await;
    let svc = MailQueueWriteService::new(pool.clone());

    let msg_id = Uuid::new_v4();
    sqlx::query("INSERT INTO messaging.mail_messages (id, body, email_from) VALUES ($1, $2, $3)")
        .bind(msg_id)
        .bind("send-path body")
        .bind("from@corp.example")
        .execute(&pool)
        .await
        .unwrap();
    let mail_id = svc
        .enqueue(msg_id, "dest@example.com", Some("cc@example.com"), None, None, None, None)
        .await
        .unwrap();

    let out = svc.process_queue(&NoopMailApi::accepting(), 10, 1).await.unwrap();
    assert_eq!(out.claimed, 1);
    assert_eq!(out.sent, 1);
    assert_eq!(out.failed, 0);

    // 250 → 'sent' on the row, failure cleared.
    let row = sqlx::query("SELECT state::text AS s, failure_type::text AS ft FROM messaging.mails WHERE id = $1")
        .bind(mail_id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(row.get::<String, _>("s"), "sent");
    assert!(row.get::<Option<String>, _>("ft").is_none());

    sqlx::query("DELETE FROM messaging.mails WHERE id = $1").bind(mail_id).execute(&pool).await.ok();
    sqlx::query("DELETE FROM messaging.outbox_events WHERE aggregate_id = ANY($1)")
        .bind(&[mail_id.to_string(), msg_id.to_string()])
        .execute(&pool).await.ok();
    sqlx::query("DELETE FROM messaging.mail_messages WHERE id = $1").bind(msg_id).execute(&pool).await.ok();
}

// ---------------------------------------------------------------------------
// MAIL-M27/M28 — the inbound pipeline
// ---------------------------------------------------------------------------

struct InboundFixture {
    server_id: Uuid,
    /// Per-fixture token (token_hash is UNIQUE — parallel tests cannot share one).
    token: String,
    /// Per-fixture RFC message-id of the seeded parent (same uniqueness reason).
    parent_rfc_id: String,
    allowlist_id: Uuid,
    parent_msg: Uuid,
    parent_thread: Uuid,
}

async fn seed_inbound(pool: &sqlx::PgPool) -> InboundFixture {
    let server_id = Uuid::new_v4();
    let token = format!("inbound-test-token-{server_id}");
    sqlx::query(
        r#"INSERT INTO messaging.fetchmail_servers (id, name, token_hash, state)
           VALUES ($1, 'gw-test', $2, 'done')"#,
    )
    .bind(server_id)
    .bind(sha256_hex(&token))
    .execute(pool)
    .await
    .expect("seed fetchmail_server");

    let allowlist_id = Uuid::new_v4();
    sqlx::query(
        r#"INSERT INTO messaging.mail_gateway_allowed (id, fetchmail_server_id, pattern)
           VALUES ($1, $2, '@partner.example')"#,
    )
    .bind(allowlist_id)
    .bind(server_id)
    .execute(pool)
    .await
    .expect("seed allowlist");

    // A parent message on a real thread (reply collation's target). The
    // message_id is per-fixture — the MAIL-B6 unique index would collide
    // under parallel runs with a static literal.
    let parent_thread = Uuid::new_v4();
    let parent_msg = Uuid::new_v4();
    let parent_rfc_id = format!("<parent-{parent_msg}@msg.id>");
    sqlx::query(
        r#"INSERT INTO messaging.mail_messages
             (id, body, message_type, model, res_id, message_id)
           VALUES ($1, 'parent', 'email', 'crm.lead', $2, $3)"#,
    )
    .bind(parent_msg)
    .bind(parent_thread)
    .bind(&parent_rfc_id)
    .execute(pool)
    .await
    .expect("seed parent message");

    InboundFixture { server_id, token, parent_rfc_id, allowlist_id, parent_msg, parent_thread }
}

async fn cleanup_inbound(pool: &sqlx::PgPool, f: &InboundFixture) {
    sqlx::query("DELETE FROM messaging.mail_gateway_allowed WHERE id = $1")
        .bind(f.allowlist_id)
        .execute(pool)
        .await
        .ok();
    sqlx::query("DELETE FROM messaging.mail_messages WHERE res_id = $1 OR id = $2")
        .bind(f.parent_thread)
        .bind(f.parent_msg)
        .execute(pool)
        .await
        .ok();
    sqlx::query(
        r#"DELETE FROM messaging.outbox_events
           WHERE aggregate_id = ANY($1) OR payload->'message'->'payload'->>'server_id' = $2"#,
    )
    .bind(&[f.server_id.to_string(), f.parent_msg.to_string()])
    .bind(f.server_id.to_string())
    .execute(pool)
    .await
    .ok();
    sqlx::query("DELETE FROM messaging.fetchmail_servers WHERE id = $1")
        .bind(f.server_id)
        .execute(pool)
        .await
        .ok();
}

fn inbound_msg(message_id: &str, from: &str, in_reply_to: Option<&str>) -> InboundMessage {
    InboundMessage {
        message_id: Some(message_id.to_string()),
        from: from.to_string(),
        to: vec!["catchall@our.domain".to_string()],
        subject: Some("inbound test".into()),
        body_html: "<p>hi</p>".into(),
        in_reply_to: in_reply_to.map(str::to_string),
    }
}

/// Row snapshot of everything THIS fixture's pipeline could touch — the
/// fail-closed proof's before/after comparison basis. Scoped to the fixture
/// (not global counts) so parallel tests can't confound the proof.
async fn snapshot_inbound_tables(pool: &sqlx::PgPool, f: &InboundFixture) -> (i64, i64, i64, i64) {
    async fn count(pool: &sqlx::PgPool, sql: &'static str, f: &InboundFixture) -> i64 {
        sqlx::query_scalar::<_, i64>(sql)
            .bind(f.parent_thread)
            .bind(f.parent_msg)
            .bind(f.server_id.to_string())
            .bind(f.server_id)
            .bind(f.allowlist_id)
            .fetch_one(pool)
            .await
            .unwrap_or(-1)
    }
    (
        count(pool, "SELECT COUNT(*) FROM messaging.mail_messages WHERE res_id = $1 OR id = $2", f).await,
        count(pool, "SELECT COUNT(*) FROM messaging.outbox_events WHERE payload->'message'->'payload'->>'server_id' = $3", f).await,
        count(pool, "SELECT COUNT(*) FROM messaging.fetchmail_servers WHERE id = $4", f).await,
        count(pool, "SELECT COUNT(*) FROM messaging.mail_gateway_allowed WHERE id = $5", f).await,
    )
}

#[tokio::test]
async fn inbound_reply_routes_to_parent_thread_and_replay_is_duplicate() {
    let Some(pool) = common::test_pool().await else {
        common::skipped("inbound reply collation + MAIL-B6 replay");
        return;
    };
    let f = seed_inbound(&pool).await;
    let svc = MailInboundService::new(pool.clone());

    // Routed: reply collation lands the message on the parent's thread.
    let child_rfc = format!("<child-{}@msg.id>", f.server_id);
    let out = svc
        .process_inbound(f.server_id, &f.token, &inbound_msg(&child_rfc, "alice@partner.example", Some(&f.parent_rfc_id)))
        .await
        .expect("inbound ok");
    let InboundOutcome::Routed { message_row, model, res_id } = out else {
        panic!("expected Routed, got {out:?}");
    };
    assert_eq!(model.as_deref(), Some("crm.lead"));
    assert_eq!(res_id, Some(f.parent_thread));
    let stored: Option<String> = sqlx::query_scalar(
        "SELECT message_id FROM messaging.mail_messages WHERE id = $1")
        .bind(message_row)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(stored.as_deref(), Some(child_rfc.as_str()));

    // MAIL-B6: the replay of the same message_id is an idempotent duplicate.
    let out = svc
        .process_inbound(f.server_id, &f.token, &inbound_msg(&child_rfc, "alice@partner.example", Some(&f.parent_rfc_id)))
        .await
        .expect("inbound ok");
    assert_eq!(out, InboundOutcome::Duplicate { existing: message_row });

    // And exactly ONE row carries that message_id (the partial index's work).
    let n: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM messaging.mail_messages WHERE message_id = $1")
        .bind(&child_rfc)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(n, 1);

    cleanup_inbound(&pool, &f).await;
}

#[tokio::test]
async fn inbound_bad_token_writes_nothing() {
    let Some(pool) = common::test_pool().await else {
        common::skipped("inbound fail-closed zero-writes");
        return;
    };
    let f = seed_inbound(&pool).await;
    let svc = MailInboundService::new(pool.clone());
    let before = snapshot_inbound_tables(&pool, &f).await;

    let err = svc
        .process_inbound(f.server_id, "wrong-token", &inbound_msg("<x@y>", "alice@partner.example", None))
        .await
        .expect_err("auth must fail");
    assert!(matches!(err, MailInboundError::Auth));

    // Unknown server id: the SAME auth error (no oracle), also zero writes.
    let err = svc
        .process_inbound(Uuid::new_v4(), &f.token, &inbound_msg("<x@y>", "alice@partner.example", None))
        .await
        .expect_err("auth must fail");
    assert!(matches!(err, MailInboundError::Auth));

    // Inactive server: still the same error, still zero writes.
    sqlx::query("UPDATE messaging.fetchmail_servers SET active = false WHERE id = $1")
        .bind(f.server_id)
        .execute(&pool)
        .await
        .unwrap();
    let err = svc
        .process_inbound(f.server_id, &f.token, &inbound_msg("<x@y>", "alice@partner.example", None))
        .await
        .expect_err("auth must fail");
    assert!(matches!(err, MailInboundError::Auth));

    assert_eq!(before, snapshot_inbound_tables(&pool, &f).await, "ZERO rows moved before auth");
    cleanup_inbound(&pool, &f).await;
}

#[tokio::test]
async fn inbound_disallowed_sender_rejected_with_event_and_unroutable_dropped_with_event() {
    let Some(pool) = common::test_pool().await else {
        common::skipped("inbound allowlist + drop");
        return;
    };
    let f = seed_inbound(&pool).await;
    let svc = MailInboundService::new(pool.clone());

    // MAIL-M28: the sender is not on the allowlist → Rejected, NO message row,
    // and the rejection event IS committed.
    let out = svc
        .process_inbound(f.server_id, &f.token, &inbound_msg("<r@j>", "stranger@evil.example", None))
        .await
        .unwrap();
    assert_eq!(out, InboundOutcome::Rejected);
    let n: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM messaging.mail_messages WHERE message_id = '<r@j>'")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(n, 0);
    let rej: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM messaging.outbox_events WHERE event_type = 'InboundEmailRejected'")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert!(rej >= 1, "the rejection event is durable");

    // Allowed but unroutable (no parent, no matching alias, no default model)
    // → Dropped WITH an event.
    let out = svc
        .process_inbound(f.server_id, &f.token, &inbound_msg("<d@j>", "alice@partner.example", None))
        .await
        .unwrap();
    assert_eq!(out, InboundOutcome::Dropped);
    let drop: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM messaging.outbox_events WHERE event_type = 'InboundEmailDropped'")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert!(drop >= 1, "the drop event is durable");

    // The default-model rung rescues the same message once configured.
    sqlx::query("UPDATE messaging.fetchmail_servers SET default_thread_model = 'crm.lead' WHERE id = $1")
        .bind(f.server_id)
        .execute(&pool)
        .await
        .unwrap();
    let rescue_rfc = format!("<dm-{}@j>", f.server_id);
    let out = svc
        .process_inbound(f.server_id, &f.token, &inbound_msg(&rescue_rfc, "alice@partner.example", None))
        .await
        .unwrap();
    assert!(matches!(out, InboundOutcome::Routed { .. }), "default model rung: {out:?}");

    // The rescued row lives on a FRESH thread — clean it by message_id.
    sqlx::query("DELETE FROM messaging.mail_messages WHERE message_id = $1")
        .bind(&rescue_rfc)
        .execute(&pool)
        .await
        .ok();
    cleanup_inbound(&pool, &f).await;
}

// ---------------------------------------------------------------------------
// sms::gc — terminal-only reap
// ---------------------------------------------------------------------------

#[tokio::test]
async fn sms_gc_reaps_only_terminal_rows_past_retention() {
    let Some(pool) = common::test_pool().await else {
        common::skipped("sms::gc terminal-only reap");
        return;
    };
    let gc = GcService::new(pool.clone(), 30, 12, 60, 30);

    for (i, (state, expect_reaped)) in [("sent", true), ("error", true), ("canceled", true), ("pending", false)].into_iter().enumerate() {
        let id = Uuid::new_v4();
        sqlx::query(
            r#"INSERT INTO messaging.sms (id, uuid, number, body, state, metadata)
               VALUES ($1, $1, $2, 'gc test', $3::sms_state,
                       jsonb_build_object('created_at', $4::timestamptz, 'updated_at', $4::timestamptz))"#,
        )
        .bind(id)
        .bind(format!("+1555000{i:02}"))
        .bind(state)
        .bind("2020-01-01T00:00:00Z")
        .execute(&pool)
        .await
        .expect("seed sms");
        let reaped = gc.sms_gc(30).await.unwrap() > 0;
        let n: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM messaging.sms WHERE id = $1")
            .bind(id)
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(n == 0, expect_reaped, "state {state}: reaped={reaped}");
        sqlx::query("DELETE FROM messaging.sms WHERE id = $1")
            .bind(id)
            .execute(&pool)
            .await
            .ok();
    }

    // A FRESH terminal row survives (age bound, not state bound).
    let fresh = Uuid::new_v4();
    sqlx::query(
        r#"INSERT INTO messaging.sms (id, uuid, number, body, state)
           VALUES ($1, $2, '+10000000009', 'gc test', 'sent'::sms_state)"#,
    )
    .bind(fresh)
    .bind(fresh.to_string())
    .execute(&pool)
    .await
    .unwrap();
    gc.sms_gc(30).await.unwrap();
    let n: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM messaging.sms WHERE id = $1")
        .bind(fresh)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(n, 1, "fresh terminal row survives the retention bound");
    sqlx::query("DELETE FROM messaging.sms WHERE id = $1").bind(fresh).execute(&pool).await.ok();
}

// ---------------------------------------------------------------------------
// Route layer — verdict shapes + no server oracle
// ---------------------------------------------------------------------------

#[tokio::test]
async fn inbound_route_verdict_shapes() {
    let Some(pool) = common::test_pool().await else {
        common::skipped("inbound route verdict shapes");
        return;
    };
    let module = Arc::new(MessagingModule::builder().with_database(pool.clone()).build().unwrap());
    let router = module.inbound_routes();
    let f = seed_inbound(&pool).await;

    let post = |uri: &str, token: Option<&str>, body: &str| {
        let mut b = axum::http::Request::builder().method("POST").uri(uri);
        if let Some(t) = token {
            b = b.header(axum::http::header::AUTHORIZATION, format!("Bearer {t}"));
        }
        b.body(axum::body::Body::from(body.to_string())).unwrap()
    };

    // Bad token → 401.
    let res = router
        .clone()
        .oneshot(post(&format!("/mail/inbound/{}", f.server_id), Some("nope"), r#"{"from":"a@partner.example","to":["c@d.e"],"body_html":"x"}"#))
        .await
        .unwrap();
    assert_eq!(res.status(), axum::http::StatusCode::UNAUTHORIZED);

    // Unknown server → the SAME 401 (no existence oracle).
    let res = router
        .clone()
        .oneshot(post("/mail/inbound/00000000-0000-0000-0000-000000000000", Some(f.token.as_str()), r#"{"from":"a@partner.example","to":["c@d.e"],"body_html":"x"}"#))
        .await
        .unwrap();
    assert_eq!(res.status(), axum::http::StatusCode::UNAUTHORIZED);

    // Unparseable body → 400 (the one pre-auth error, relay-fixable).
    let res = router
        .clone()
        .oneshot(post(&format!("/mail/inbound/{}", f.server_id), Some(f.token.as_str()), "{not json"))
        .await
        .unwrap();
    assert_eq!(res.status(), axum::http::StatusCode::BAD_REQUEST);

    // Valid → 200 with a labeled outcome.
    let body = format!(
        r#"{{"message_id":"<route-{}@proof>","from":"alice@partner.example","to":["c@our.domain"],"body_html":"x","in_reply_to":"{}"}}"#,
        f.server_id, f.parent_rfc_id
    );
    let res = router
        .clone()
        .oneshot(post(
            &format!("/mail/inbound/{}", f.server_id),
            Some(f.token.as_str()),
            &body,
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), axum::http::StatusCode::OK);
    let bytes = axum::body::to_bytes(res.into_body(), 64 * 1024).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(json["outcome"], "routed");

    cleanup_inbound(&pool, &f).await;
}
