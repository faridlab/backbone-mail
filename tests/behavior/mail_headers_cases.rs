//! Per-mail custom headers + email-blacklist verb cases (hand-written;
//! user-owned).
//!
//! DB-backed probes for the two write surfaces this increment adds, each on
//! its own scratch database (`sqlx::test`) with the module migrations
//! applied:
//!
//! - the headers column round-trip: enqueue with a headers object → the
//!   drain hands the port a request carrying them; enqueue without headers
//!   stores the empty-object default; a CR/LF smuggle in a name or value is
//!   refused at enqueue with the typed error (nothing persisted); and a row
//!   whose headers column holds a non-object (written outside the sanctioned
//!   enqueue) fails LOUDLY on the drain — visible failure reason, never a
//!   silent partial send;
//! - the email-blacklist verbs: the opt-out reason persists through add, the
//!   lowercase fold converges mixed-case writes onto one row, an unreasoned
//!   re-add never erases a recorded reason, and remove archives without
//!   touching the reason.

use std::collections::HashMap;

use backbone_mail::application::service::mail_blacklist_write_service::{
    AddOutcome, MailBlacklistWriteService, RemoveOutcome,
};
use backbone_mail::application::service::mail_ports::MailHeaderError;
use backbone_mail::application::service::{MailQueueError, MailQueueWriteService, NoopMailApi};
use sqlx::{PgPool, Row};
use uuid::Uuid;

async fn seed_message(pool: &PgPool) -> Uuid {
    let id = Uuid::new_v4();
    sqlx::query("INSERT INTO messaging.mail_messages (id, body) VALUES ($1, $2)")
        .bind(id)
        .bind("headers-case body")
        .execute(pool)
        .await
        .expect("seed mail_messages");
    id
}

/// The module migrations do not create `messaging.outbox_events` (the outbox
/// crate owns that table — same bootstrap the shared harness runs), and the
/// queue verbs stage bus events in-tx: provision it on the scratch database
/// before touching the queue.
async fn migrate_outbox(pool: &PgPool) {
    backbone_outbox::outbox::migrate(pool, "messaging")
        .await
        .expect("outbox migrate");
}

// ─── per-mail headers: round-trip + refusal ──────────────────────────────────

#[sqlx::test(migrations = "./migrations")]
async fn enqueued_headers_flow_onto_the_port_request(pool: PgPool) {
    migrate_outbox(&pool).await;
    let svc = MailQueueWriteService::new(pool.clone());
    let msg_id = seed_message(&pool).await;
    let headers = serde_json::json!({
        "X-Campaign-Id": "digest-launch",
        "List-Unsubscribe": "<https://example.com/unsub>",
    });

    let mail_id = svc
        .enqueue(msg_id, "dest@example.com", None, None, Some(&headers), None, None, None)
        .await
        .expect("enqueue with headers");

    // The row persisted the object verbatim.
    let stored: serde_json::Value = sqlx::query("SELECT headers FROM messaging.mails WHERE id = $1")
        .bind(mail_id)
        .fetch_one(&pool)
        .await
        .expect("mails row")
        .get("headers");
    assert_eq!(stored, headers);

    // The drain hands the port a request carrying them, verbatim.
    let port = NoopMailApi::accepting();
    let out = svc.process_queue(&port, 10, 1).await.expect("drain");
    assert_eq!(out.claimed, 1);
    assert_eq!(out.sent, 1);
    let requests = port.requests();
    let request = requests.iter().find(|r| r.mail_id == mail_id).expect("the drained row");
    let expected: HashMap<String, String> = [
        ("X-Campaign-Id".to_string(), "digest-launch".to_string()),
        ("List-Unsubscribe".to_string(), "<https://example.com/unsub>".to_string()),
    ]
    .into_iter()
    .collect();
    assert_eq!(request.headers, expected);
    assert_eq!(request.to, vec!["dest@example.com".to_string()]);
}

#[sqlx::test(migrations = "./migrations")]
async fn enqueue_without_headers_stores_the_empty_object_default(pool: PgPool) {
    migrate_outbox(&pool).await;
    let svc = MailQueueWriteService::new(pool.clone());
    let msg_id = seed_message(&pool).await;
    let mail_id = svc
        .enqueue(msg_id, "dest@example.com", None, None, None, None, None, None)
        .await
        .expect("enqueue without headers");

    let stored: serde_json::Value = sqlx::query("SELECT headers FROM messaging.mails WHERE id = $1")
        .bind(mail_id)
        .fetch_one(&pool)
        .await
        .expect("mails row")
        .get("headers");
    assert_eq!(stored, serde_json::json!({}));

    // And the port request carries an empty map (not None — the field is
    // always present on the request).
    let port = NoopMailApi::accepting();
    svc.process_queue(&port, 10, 1).await.expect("drain");
    let requests = port.requests();
    let request = requests.iter().find(|r| r.mail_id == mail_id).expect("drained");
    assert!(request.headers.is_empty());
}

#[sqlx::test(migrations = "./migrations")]
async fn crlf_smuggle_in_a_header_is_refused_at_enqueue_nothing_persisted(pool: PgPool) {
    migrate_outbox(&pool).await;
    let svc = MailQueueWriteService::new(pool.clone());
    let msg_id = seed_message(&pool).await;

    // Value-side smuggle: a second header line riding one value.
    let smuggled = serde_json::json!({
        "X-Campaign-Id": "digest\r\nBcc: victim@example.com",
    });
    match svc
        .enqueue(msg_id, "dest@example.com", None, None, Some(&smuggled), None, None, None)
        .await
    {
        Err(MailQueueError::Header(MailHeaderError::LineBreakInValue { name })) => {
            assert_eq!(name, "X-Campaign-Id");
        }
        other => panic!("CRLF in value must refuse with the typed error, got {other:?}"),
    }

    // Name-side smuggle too.
    let smuggled_name = serde_json::json!({ "X-Fine: 1\r\nBcc: a@b.c": "v" });
    assert!(matches!(
        svc.enqueue(msg_id, "dest2@example.com", None, None, Some(&smuggled_name), None, None, None).await,
        Err(MailQueueError::Header(MailHeaderError::LineBreakInName { .. }))
    ));

    // The refusal is total: no row, no staged event, nothing to clean up.
    let rows: i64 = sqlx::query_scalar("SELECT count(*) FROM messaging.mails")
        .fetch_one(&pool)
        .await
        .expect("count mails");
    assert_eq!(rows, 0, "a refused enqueue must not write a queue row");
    let events: i64 =
        sqlx::query_scalar("SELECT count(*) FROM messaging.outbox_events WHERE event_type = 'MailQueued'")
            .fetch_one(&pool)
            .await
            .expect("count events");
    assert_eq!(events, 0, "a refused enqueue must not stage MailQueued");
}

#[sqlx::test(migrations = "./migrations")]
async fn a_rogue_non_object_headers_column_fails_the_row_loudly_on_drain(pool: PgPool) {
    // A row written OUTSIDE the sanctioned enqueue (raw SQL / generic CRUD)
    // whose headers column is not an object of strings: the drain refuses it
    // visibly — the row lands 'exception' with the refusal as its failure
    // reason, the port is never asked to send it.
    migrate_outbox(&pool).await;
    let svc = MailQueueWriteService::new(pool.clone());
    let msg_id = seed_message(&pool).await;
    let mail_id = Uuid::new_v4();
    sqlx::query(
        r#"INSERT INTO messaging.mails (id, mail_message_id, email_to, headers)
           VALUES ($1, $2, 'dest@example.com', '["not","an","object"]'::jsonb)"#,
    )
    .bind(mail_id)
    .bind(msg_id)
    .execute(&pool)
    .await
    .expect("seed rogue row");

    let port = NoopMailApi::accepting();
    let out = svc.process_queue(&port, 10, 1).await.expect("drain");
    assert_eq!(out.claimed, 1);
    assert_eq!(out.failed, 1);
    assert_eq!(out.sent, 0, "the port must not be asked to send a malformed row");

    let row = sqlx::query(
        "SELECT state::text AS s, failure_type::text AS ft, failure_reason FROM messaging.mails WHERE id = $1",
    )
    .bind(mail_id)
    .fetch_one(&pool)
    .await
    .expect("row");
    assert_eq!(row.get::<String, _>("s"), "exception");
    assert_eq!(row.get::<Option<String>, _>("ft").as_deref(), Some("unknown"));
    let reason: Option<String> = row.get("failure_reason");
    assert!(
        reason.as_deref().unwrap_or_default().contains("malformed per-mail headers"),
        "the failure reason must name the defect: {reason:?}"
    );

    // And the accepting port saw nothing for this row.
    assert!(port.requests().iter().all(|r| r.mail_id != mail_id));
}

// ─── email-blacklist verbs: the opt-out reason writer ─────────────────────────

fn verbs(pool: &PgPool) -> MailBlacklistWriteService {
    MailBlacklistWriteService::new(pool.clone())
}

async fn blacklist_row(pool: &PgPool, email: &str) -> Option<(Uuid, Option<Uuid>, bool)> {
    sqlx::query("SELECT id, opt_out_reason_id, active FROM messaging.mail_blacklists WHERE email = $1")
        .bind(email)
        .fetch_optional(pool)
        .await
        .expect("fetch mail_blacklists row")
        .map(|r| (r.get::<Uuid, _>("id"), r.get("opt_out_reason_id"), r.get("active")))
}

#[sqlx::test(migrations = "./migrations")]
async fn add_persists_the_opt_out_reason_and_folds_case(pool: PgPool) {
    let reason = Uuid::new_v4();
    let out = verbs(&pool).add("  User@Example.COM  ", Some(reason)).await.expect("add");
    assert!(matches!(out, AddOutcome::NewlyListed { .. }));

    // Stored lowercased (the documented app-layer fold), reason persisted.
    let Some((_, stored_reason, active)) = blacklist_row(&pool, "user@example.com").await else {
        panic!("row must be stored under the lowercased address");
    };
    assert_eq!(stored_reason, Some(reason));
    assert!(active);

    // A mixed-case write converges on the SAME row, never a near-duplicate.
    let again = verbs(&pool).add("user@example.com", None).await.expect("re-add");
    assert!(matches!(again, AddOutcome::AlreadyListed { .. }));
    assert_eq!(again.row_id(), out.row_id());
    let rows: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM messaging.mail_blacklists WHERE email LIKE '%example.com'",
    )
    .fetch_one(&pool)
    .await
    .expect("count");
    assert_eq!(rows, 1);

    // Membership probes fold the same way (point + batch).
    assert!(verbs(&pool).is_listed("USER@EXAMPLE.COM").await.unwrap());
    let listed = verbs(&pool)
        .listed_among(&["USER@example.com".into(), "absent@example.com".into()])
        .await
        .unwrap();
    assert_eq!(listed, ["user@example.com".to_string()].into_iter().collect());
}

#[sqlx::test(migrations = "./migrations")]
async fn an_unreasoned_re_add_never_erases_a_recorded_reason(pool: PgPool) {
    let reason = Uuid::new_v4();
    let svc = verbs(&pool);
    svc.add("no-reason-erase@example.com", Some(reason)).await.unwrap();
    // A later add WITHOUT a reason keeps the recorded one (COALESCE).
    svc.add("no-reason-erase@example.com", None).await.unwrap();
    let (_, stored, _) = blacklist_row(&pool, "no-reason-erase@example.com")
        .await
        .expect("row");
    assert_eq!(stored, Some(reason));
    // A re-add WITH a different reason records the newer one.
    let newer = Uuid::new_v4();
    svc.add("no-reason-erase@example.com", Some(newer)).await.unwrap();
    let (_, stored, _) = blacklist_row(&pool, "no-reason-erase@example.com")
        .await
        .expect("row");
    assert_eq!(stored, Some(newer));
}

#[sqlx::test(migrations = "./migrations")]
async fn remove_archives_the_row_and_keeps_the_recorded_reason(pool: PgPool) {
    let reason = Uuid::new_v4();
    let svc = verbs(&pool);
    svc.add("archived@example.com", Some(reason)).await.unwrap();

    let removed = svc.remove("ARCHIVED@example.com").await.expect("remove");
    assert!(matches!(removed, RemoveOutcome::Removed { .. }));

    // Archived, not deleted — and the reason survives the archive (the
    // listing's history stays inspectable while suppressed).
    let Some((row_id, stored_reason, active)) = blacklist_row(&pool, "archived@example.com").await
    else {
        panic!("remove must retain the row (archive, not delete)");
    };
    assert!(!active);
    assert_eq!(stored_reason, Some(reason));
    assert_eq!(Some(row_id), removed.row_id());

    // Not listed while archived; a second remove is AlreadyInactive; removing
    // an address that was never listed is NotListed — idempotent everywhere.
    assert!(!svc.is_listed("archived@example.com").await.unwrap());
    assert!(matches!(
        svc.remove("archived@example.com").await.unwrap(),
        RemoveOutcome::AlreadyInactive { .. }
    ));
    assert!(matches!(
        svc.remove("never-existed@example.com").await.unwrap(),
        RemoveOutcome::NotListed
    ));

    // Re-add reactivates the SAME row (with its reason intact).
    let back = svc.add("archived@example.com", None).await.expect("re-add");
    assert!(matches!(back, AddOutcome::Reactivated { .. }));
    assert_eq!(back.row_id(), row_id);
    let (_, stored_again, active_again) =
        blacklist_row(&pool, "archived@example.com").await.expect("row");
    assert!(active_again);
    assert_eq!(stored_again, Some(reason), "the unreasoned re-add keeps the reason");
}

#[sqlx::test(migrations = "./migrations")]
async fn parallel_adds_of_one_address_converge_on_a_single_row(pool: PgPool) {
    let reason = Uuid::new_v4();
    let svc = std::sync::Arc::new(verbs(&pool));
    let (s1, s2, s3) = (svc.clone(), svc.clone(), svc.clone());
    let (a, b, c) = tokio::join!(
        s1.add("Race@Example.com", Some(reason)),
        s2.add("race@example.com", None),
        s3.add("RACE@example.COM", Some(reason)),
    );
    let outcomes = [a.expect("add a"), b.expect("add b"), c.expect("add c")];
    let row_ids: std::collections::HashSet<Uuid> = outcomes.iter().map(|o| o.row_id()).collect();
    assert_eq!(row_ids.len(), 1, "parallel adds converge on one row");
    let (_, stored, active) = blacklist_row(&pool, "race@example.com").await.expect("row");
    assert!(active);
    assert_eq!(stored, Some(reason));
}
