//! Increment-2 SSE proofs (live DB, in-process router + a real tailer):
//! post→receive inside the latency budget, watermark replay + forged-watermark
//! clamp, the session proof as the first frame, foreign-channel events dropped
//! (BUS-B2 allowlist — no mid-stream oracle), presence observable on the
//! identity's channel, the per-identity connection cap, and anonymous 401.

use std::sync::Arc;
use std::time::Duration;

use backbone_mail::application::service::chatter_acl::MessagingIdentity;
use backbone_mail::presentation::middleware::{guest_context, AuthPartnerId};
use backbone_mail::realtime::session::verify_session_token;
use backbone_mail::realtime::tailer::TailerConfig;
use backbone_mail::realtime::SessionSecret;
use backbone_mail::MessagingModule;
use tower::ServiceExt;
use uuid::Uuid;

use super::common;

const PER_FRAME: Duration = Duration::from_secs(2);

fn sse_wire(module: Arc<MessagingModule>) -> axum::Router {
    use axum::middleware as axum_mw;
    axum::Router::new()
        .merge(backbone_mail::realtime::sse::composer())
        .layer(axum::Extension(SessionSecret(Arc::new(
            b"sse-test-secret".to_vec(),
        ))))
        .layer(axum_mw::from_fn(guest_context))
        .with_state(module)
}

async fn module_with_tailer() -> Option<(Arc<MessagingModule>, axum::Router, tokio::task::JoinHandle<()>)> {
    let pool = common::test_pool().await?;
    let module = Arc::new(MessagingModule::builder().with_database(pool).build().ok()?);
    // A real tailer at test cadence — the proof is the full
    // outbox→tailer→registry→stream path, not a registry.publish shortcut.
    let tailer = backbone_mail::realtime::tailer::spawn(
        module.db_pool(),
        Arc::clone(&module.realtime_registry),
        TailerConfig { window_seconds: 55, poll_ms: 100 },
    );
    let router = sse_wire(Arc::clone(&module));
    Some((module, router, tailer))
}

/// Accumulating reader over one SSE connection.
struct Frames {
    buf: String,
    inner: axum::body::BodyDataStream,
}

impl Frames {
    async fn open(router: &axum::Router, req: axum::http::Request<axum::body::Body>) -> Option<Self> {
        let res = router.clone().oneshot(req).await.expect("oneshot");
        if res.status() != axum::http::StatusCode::OK {
            return None;
        }
        Some(Self { buf: String::new(), inner: res.into_body().into_data_stream() })
    }

    /// Pull frames until `marker` appears in the accumulated text, or the
    /// per-call budget expires. Returns false on timeout.
    async fn wait_for(&mut self, marker: &str, budget: Duration) -> bool {
        let deadline = tokio::time::Instant::now() + budget;
        loop {
            if self.buf.contains(marker) {
                return true;
            }
            let remain = deadline.saturating_duration_since(tokio::time::Instant::now());
            if remain.is_zero() {
                return false;
            }
            match tokio::time::timeout(remain, tokio_stream::StreamExt::next(&mut self.inner)).await {
                Ok(Some(Ok(chunk))) => self.buf.push_str(&String::from_utf8_lossy(&chunk)),
                Ok(Some(Err(_))) | Ok(None) => return self.buf.contains(marker),
                Err(_) => return false,
            }
        }
    }

    /// Keep pulling for `quiet` and assert `marker` NEVER arrives.
    async fn assert_silent(&mut self, marker: &str, quiet: Duration) {
        let deadline = tokio::time::Instant::now() + quiet;
        while tokio::time::Instant::now() < deadline {
            let remain = deadline.saturating_duration_since(tokio::time::Instant::now());
            if let Ok(Some(Ok(chunk))) =
                tokio::time::timeout(remain, tokio_stream::StreamExt::next(&mut self.inner)).await
            {
                self.buf.push_str(&String::from_utf8_lossy(&chunk));
            }
            assert!(!self.buf.contains(marker), "foreign event leaked into the stream: {marker}");
        }
    }
}

fn stream_req(partner: Option<Uuid>, last_event_id: Option<Uuid>) -> axum::http::Request<axum::body::Body> {
    let mut b = axum::http::Request::builder().method("GET").uri("/mail/realtime/stream");
    if let Some(id) = last_event_id {
        b = b.header("last-event-id", id.to_string());
    }
    let req = b.body(axum::body::Body::empty()).unwrap();
    match partner {
        Some(p) => {
            let mut req = req;
            req.extensions_mut().insert(AuthPartnerId(p));
            req
        }
        None => req,
    }
}

async fn cleanup(pool: &sqlx::PgPool, message_ids: &[Uuid], partner: Uuid) {
    for id in message_ids {
        sqlx::query("DELETE FROM messaging.outbox_events WHERE aggregate_id = $1")
            .bind(id.to_string())
            .execute(pool)
            .await
            .ok();
        sqlx::query("DELETE FROM messaging.mail_notifications WHERE message_id = $1")
            .bind(id)
            .execute(pool)
            .await
            .ok();
    }
    common::cleanup(pool, &[("mail_messages", message_ids)]).await;
    sqlx::query("DELETE FROM messaging.mail_presences WHERE user_id = $1")
        .bind(partner)
        .execute(pool)
        .await
        .ok();
}

// ---------------------------------------------------------------------------
// Post → receive, watermark replay, forged-watermark clamp
// ---------------------------------------------------------------------------

#[tokio::test]
async fn post_reaches_stream_replays_and_clamps() {
    let Some((m, router, tailer)) = module_with_tailer().await else { return common::skipped("sse_replay") };
    let pool = m.db_pool();
    let partner = Uuid::new_v4();
    let identity = MessagingIdentity::User { partner_id: partner };
    let mut posted_ids = Vec::new();

    // Post ONE, then connect: the frame must arrive within the budget and
    // the FIRST frame on the wire is the session proof.
    let p1 = m
        .thread_chatter_service
        .post(&identity, "res.partner", partner, "sse-body-one", None, false)
        .await
        .expect("post one");
    posted_ids.push(p1.message_id);

    let mut stream = Frames::open(&router, stream_req(Some(partner), None))
        .await
        .expect("stream connects");
    // The bus payload addresses by message_id (not body text) — key on it.
    assert!(
        stream.wait_for(&p1.message_id.to_string(), PER_FRAME).await,
        "post must reach the stream"
    );
    assert!(
        stream.buf.starts_with("event: session"),
        "first frame is the session proof, got: {:?}",
        &stream.buf[..stream.buf.len().min(80)]
    );
    // The proof is a real, verifiable token for THIS identity.
    let session_at = stream.buf.find("event: session").expect("session frame");
    let token = stream.buf[session_at..]
        .lines()
        .find_map(|l| l.strip_prefix("data: "))
        .expect("session frame carries a proof token")
        .to_string();

    // The message frame carries the outbox watermark id.
    let id1 = stream
        .buf
        .lines()
        .find(|l| l.starts_with("id: "))
        .and_then(|l| Uuid::parse_str(l.trim_start_matches("id: ")).ok())
        .expect("message frame has a Last-Event-ID");
    drop(stream);

    // Post TWO while disconnected.
    let p2 = m
        .thread_chatter_service
        .post(&identity, "res.partner", partner, "sse-body-two", None, false)
        .await
        .expect("post two");
    posted_ids.push(p2.message_id);

    // Reconnect at the watermark: replay of two, NOT one again.
    let mut replay = Frames::open(&router, stream_req(Some(partner), Some(id1)))
        .await
        .expect("reconnect");
    assert!(
        replay.wait_for(&p2.message_id.to_string(), PER_FRAME).await,
        "events after the watermark replay"
    );
    assert!(!replay.buf.contains(&p1.message_id.to_string()), "watermarked event must not repeat");

    // Forged/too-old watermark: clamped to the window start — BOTH replay.
    let mut clamped = Frames::open(&router, stream_req(Some(partner), Some(Uuid::new_v4())))
        .await
        .expect("clamp reconnect");
    assert!(
        clamped.wait_for(&p1.message_id.to_string(), PER_FRAME).await,
        "clamp replays the window from the start"
    );
    assert!(
        clamped.wait_for(&p2.message_id.to_string(), PER_FRAME).await,
        "the whole window replays under a clamped watermark"
    );
    drop(clamped);
    drop(replay);

    // The token proves presence-worthy: same secret verifies it end-to-end.
    assert!(verify_session_token(b"sse-test-secret", &token, &identity));

    tailer.abort();
    cleanup(&pool, &posted_ids, partner).await;
}

// ---------------------------------------------------------------------------
// BUS-B2 allowlist: foreign channels dropped silently; presence observable
// ---------------------------------------------------------------------------

#[tokio::test]
async fn foreign_channels_dropped_presence_observable() {
    let Some((m, router, tailer)) = module_with_tailer().await else { return common::skipped("sse_allowlist") };
    let pool = m.db_pool();
    let partner = Uuid::new_v4();
    let identity = MessagingIdentity::User { partner_id: partner };
    let foreign = MessagingIdentity::User { partner_id: Uuid::new_v4() };
    let mut posted_ids = Vec::new();

    let mut stream = Frames::open(&router, stream_req(Some(partner), None))
        .await
        .expect("stream connects");
    assert!(stream.wait_for("event: session", PER_FRAME).await);

    // Presence on OUR channel is observable: manual status → im_status frame.
    m.presence_write_service.set_manual_im_status(&identity, "away").await.expect("presence");
    assert!(
        stream.wait_for("\"im_status\":\"away\"", PER_FRAME).await,
        "im_status_updated must reach the identity's channel"
    );

    // A message on a FOREIGN partner's wall: staged in the outbox (the row
    // exists, addressed by message_id), but it must never reach our wire —
    // dropped, no 403 oracle.
    let f1 = m
        .thread_chatter_service
        .post(&foreign, "res.partner", foreign.partner_id().unwrap(), "foreign body", None, false)
        .await
        .expect("foreign post");
    posted_ids.push(f1.message_id);
    let staged: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM messaging.outbox_events WHERE aggregate_id = $1",
    )
    .bind(f1.message_id.to_string())
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(staged, 1, "foreign event IS in the outbox (the drop is stream-side)");
    stream.assert_silent(&f1.message_id.to_string(), Duration::from_millis(700)).await;
    drop(stream);

    tailer.abort();
    cleanup(&pool, &posted_ids, partner).await;
    cleanup(&pool, &[f1.message_id], foreign.partner_id().unwrap()).await;
}

// ---------------------------------------------------------------------------
// Per-identity connection cap → 429; anonymous → 401
// ---------------------------------------------------------------------------

#[tokio::test]
async fn connection_cap_and_anonymous_gate() {
    let Some((_m, router, tailer)) = module_with_tailer().await else { return common::skipped("sse_cap") };
    let partner = Uuid::new_v4();

    // Anonymous: 401 before anything else.
    let res = router.clone().oneshot(stream_req(None, None)).await.expect("oneshot");    assert_eq!(res.status(), axum::http::StatusCode::UNAUTHORIZED);

    // Four live streams for one identity are fine — hold them open.
    let mut held = Vec::new();
    for _ in 0..4 {
        let res = router.clone().oneshot(stream_req(Some(partner), None)).await.expect("oneshot");
        assert_eq!(res.status(), axum::http::StatusCode::OK);
        held.push(res.into_body().into_data_stream());
    }

    // The fifth: 429, and the JSON says why.
    let res = router.clone().oneshot(stream_req(Some(partner), None)).await.expect("oneshot");
    assert_eq!(res.status(), axum::http::StatusCode::TOO_MANY_REQUESTS);
    let bytes = axum::body::to_bytes(res.into_body(), 16 * 1024).await.expect("body");
    let body: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert!(body["error"].as_str().unwrap().contains("concurrent streams"));

    drop(held); // releases the slots
    tokio::time::sleep(Duration::from_millis(200)).await;
    let res = router.clone().oneshot(stream_req(Some(partner), None)).await.expect("oneshot");
    assert_eq!(res.status(), axum::http::StatusCode::OK, "released slots admit a new stream");

    tailer.abort();
}
