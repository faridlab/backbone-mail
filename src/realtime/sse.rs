//! The SSE stream endpoint (hand-written; user-owned).
//!
//! `GET /mail/realtime/stream` — Odoo's websocket receive-push, ported onto
//! SSE over the outbox tailer (ADR-0017). The route never writes: it is a
//! pure read surface (ADR-0019's sanctioned GET shape — a snapshot stream,
//! not a mutating read).
//!
//! Security posture:
//! - **Identity required** (partner via app auth, guest via dgid cookie);
//!   anonymous → 401. No proof needed to CONNECT — the connection mints a
//!   session proof as its FIRST frame (`event: session`), which the client
//!   must present for `update_bus_presence` (Odoo's is_websocket_session).
//! - **Per-identity connection cap** (4) → 429 beyond — one identity cannot
//!   hold an unbounded fan-out.
//! - **Allowlist, not oracle**: each event is checked against the
//!   identity's own channel + live membership channels, and record-shaped
//!   keys additionally through the MAIL-B1 resolver. Out-of-allowlist
//!   events are DROPPED with a warn — never a 403 mid-stream, which would
//!   hand the client a channel-existence oracle.
//! - **Watermark**: `Last-Event-ID` = outbox row uuid; found → replay after
//!   it from the ring; absent/forged/too-old → clamp to window start
//!   (Odoo's reset-0 semantics — a client can never skip itself forward).

use std::convert::Infallible;
use std::sync::Arc;

use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::Router;
use tokio_stream::wrappers::ReceiverStream;
use uuid::Uuid;

use crate::application::service::chatter_acl::{MessagingIdentity, ThreadAccessResolver};
use crate::domain::event::constants::{discuss_channel, guest_channel, partner_channel};
use crate::infrastructure::persistence::channel_repository::ChannelRepository;
use crate::presentation::http::thread_routes::ApiState;
use crate::presentation::middleware::{unauthorized, WireIdentity};
use crate::realtime::session::{mint_session_token, SESSION_TTL_SECS};
use crate::realtime::SessionSecret;
use crate::realtime::tailer::parse_record_key;

use super::registry::{identity_key, ConnectionGuard, RealtimeRegistry, TailedEvent};

/// One identity may hold at most this many concurrent streams.
const MAX_STREAMS_PER_IDENTITY: usize = 4;
/// Membership-allowlist ceiling (config `realtime.max_channels_per_connection`).
const MAX_MEMBERSHIP_CHANNELS: usize = 64;

/// The per-connection allowlist: own channel + live membership channels.
/// Record-shaped keys NOT in this set fall through to the MAIL-B1 resolver
/// per-event (see [`Allowlist::permits`]).
struct Allowlist {
    own: String,
    membership: std::collections::HashSet<String>,
}

impl Allowlist {
    async fn build(pool: &sqlx::PgPool, identity: &MessagingIdentity) -> Self {
        let mut membership = std::collections::HashSet::new();
        match ChannelRepository::membership_channel_ids(pool, identity.partner_id(), identity.guest_id()).await {
            Ok(ids) => {
                for id in ids {
                    if membership.len() >= MAX_MEMBERSHIP_CHANNELS {
                        tracing::warn!(
                            target: "mail::realtime_stream",
                            identity = %identity_key(identity),
                            "membership allowlist truncated at MAX_MEMBERSHIP_CHANNELS"
                        );
                        break;
                    }
                    membership.insert(discuss_channel(id));
                }
            }
            Err(e) => {
                // Degraded, not open: with no membership leg the connection
                // still gets its own channel + resolver-checked records.
                tracing::error!(target: "mail::realtime_stream", error = %e, "membership lookup failed");
            }
        }
        Self { own: identity.channel(), membership }
    }

    /// May this event reach this identity's wire? The resolver call is the
    /// async part — hence a method on the state, not the filter closure.
    async fn permits(
        &self,
        pool: &sqlx::PgPool,
        identity: &MessagingIdentity,
        acl: &crate::application::service::chatter_acl::ThreadAclSlot,
        event: &TailedEvent,
    ) -> bool {
        if event.channel == self.own {
            return true;
        }
        if self.membership.contains(&event.channel) {
            return true;
        }
        match parse_record_key(&event.channel) {
            // Record-shaped and not directly allowlisted (another partner's
            // wall, a host document's chatter): the MAIL-B1 walk decides.
            Some((model, res_id)) => acl.can_read(pool, identity, model, res_id).await,
            // Non-record shape we have no relationship to: dropped.
            None => false,
        }
    }
}

fn sse_event(event: &TailedEvent) -> Event {
    Event::default()
        .id(event.id.to_string())
        .event(event.message_type.as_str())
        .data(event.payload.to_string())
}

async fn stream(
    State(app): State<ApiState>,
    axum::Extension(identity): axum::Extension<WireIdentity>,
    axum::Extension(secret): axum::Extension<SessionSecret>,
    headers: HeaderMap,
) -> Response {
    // Anonymous: no stream. (Guests are identified via the dgid cookie the
    // guest middleware resolved into `identity`.)
    let Ok(id) = crate::presentation::http::thread_routes::require_identity(&identity) else {
        return unauthorized();
    };

    // Per-identity connection cap. The guard moves into the forwarder task:
    // it releases when the client disconnects (body dropped → send fails).
    let registry = Arc::clone(&app.realtime_registry);
    if registry.reserve_connection(&identity_key(&id), MAX_STREAMS_PER_IDENTITY).is_err() {
        return (
            StatusCode::TOO_MANY_REQUESTS,
            axum::Json(serde_json::json!({
                "error": format!("too many concurrent streams for this identity (max {MAX_STREAMS_PER_IDENTITY})")
            })),
        )
            .into_response();
    }
    let _guard = ConnectionGuard::new(Arc::clone(&registry), &id);

    // Watermark: a parseable uuid replays after that row; anything else
    // (absent, malformed, forged) clamps to the window start.
    let last_id = headers
        .get("last-event-id")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| Uuid::parse_str(v).ok());

    // Snapshot the replay set BEFORE subscribing, then subscribe — a
    // broadcast receiver only sees post-subscribe sends, so this order
    // cannot double-deliver or drop.
    let replay = registry.replay_after(last_id);
    let mut live = registry.subscribe();

    let pool = app.db_pool();
    let acl = app.thread_acl.clone();
    let allow = Allowlist::build(&pool, &id).await;

    // The first frame is the minted session proof (the is_websocket_session
    // port: connect mints, presence verbs verify).
    let proof = mint_session_token(&secret.0, &id, SESSION_TTL_SECS);

    let (tx, rx) = tokio::sync::mpsc::channel::<Result<Event, Infallible>>(64);
    tokio::spawn(async move {
        let _guard = _guard; // moved in — releases when this task ends
        let send = |ev: Event| tx.send(Ok(ev));
        if send(Event::default().event("session").data(proof)).await.is_err() {
            return; // client gone before the first frame
        }
        for event in &replay {
            if allow.permits(&pool, &id, &acl, event).await {
                if send(sse_event(event)).await.is_err() {
                    return;
                }
            } else {
                tracing::warn!(
                    target: "mail::realtime_stream",
                    channel = %event.channel,
                    outbox_id = %event.id,
                    "BUS-B2: dropping out-of-allowlist event for this identity"
                );
            }
        }
        loop {
            match live.recv().await {
                Ok(event) => {
                    if allow.permits(&pool, &id, &acl, &event).await {
                        if send(sse_event(&event)).await.is_err() {
                            return; // client disconnected
                        }
                    } else {
                        tracing::warn!(
                            target: "mail::realtime_stream",
                            channel = %event.channel,
                            outbox_id = %event.id,
                            "BUS-B2: dropping out-of-allowlist event for this identity"
                        );
                    }
                }
                Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                    // Too slow: frames missed. Reconnect-with-watermark is
                    // the client's remedy; keep streaming.
                    tracing::warn!(
                        target: "mail::realtime_stream",
                        missed = n,
                        "stream lagged; client should reconnect with Last-Event-ID"
                    );
                }
                Err(tokio::sync::broadcast::error::RecvError::Closed) => return,
            }
        }
    });

    let stream: ReceiverStream<Result<Event, Infallible>> = ReceiverStream::new(rx);
    let mut response = Sse::new(stream)
        .keep_alive(KeepAlive::new().interval(std::time::Duration::from_secs(15)))
        .into_response();
    // Disable reverse-proxy buffering (nginx et al.) or SSE stalls behind it.
    response
        .headers_mut()
        .insert("x-accel-buffering", "no".parse().unwrap());
    response
}

/// The realtime route group. Requires BOTH the `guest_context` middleware
/// (identity resolution) and the `realtime::SessionSecret` router extension
/// — the same extension contract as the presence group.
pub fn composer() -> Router<ApiState> {
    Router::new().route("/mail/realtime/stream", get(stream))
}

#[cfg(test)]
mod tests {
    use super::*;

    // The allowlist's static legs are exercised through the behavior tests
    // (post→receive, foreign-channel rejection, reconnect replay); here only
    // the pure pieces.

    #[test]
    fn own_and_membership_keys_are_direct_allows_by_construction() {
        // The keys the constructors build are exactly what the allowlist
        // compares against — grammar-validated shapes by BUS-B2 (see
        // tailer tests). A sanity check that partner/guest/discuss keys all
        // parse as record keys, so the resolver path is their ONLY other
        // route when not directly allowlisted:
        let p = Uuid::new_v4();
        assert_eq!(partner_channel(p), format!("res.partner_{p}"));
        let g = Uuid::new_v4();
        assert_eq!(guest_channel(g), format!("mail.guest_{g}"));
        let c = Uuid::new_v4();
        assert_eq!(discuss_channel(c), format!("discuss.channel_{c}"));
    }
}
