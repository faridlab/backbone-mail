//! Increment-2 route-layer proofs (live DB, in-process router via
//! `tower::ServiceExt::oneshot`): the authz matrix (partner / guest /
//! anonymous × allowed / denied), the chatter deny-by-default surfaced over
//! HTTP, throttle 429, the SSE-session proof gate on bus presence, guest
//! mint/rename over the wire (Set-Cookie), the webhook's fail-closed
//! behavior END-TO-END (raw bytes + header), and the ADR-0019 GET-snapshot
//! (GETs leave rows unchanged).

use std::sync::Arc;

use backbone_mail::application::service::chatter_acl::MessagingIdentity;
use backbone_mail::presentation::http::{
    attachment_routes, channel_routes, guest_routes, presence_routes, public_routes,
    thread_routes, webhook_routes,
};
use backbone_mail::presentation::middleware::{guest_context, AuthPartnerId};
use backbone_mail::realtime::session::mint_session_token;
use backbone_mail::realtime::SessionSecret;
use backbone_mail::MessagingModule;
use tower::ServiceExt;
use uuid::Uuid;

use super::common;

/// The full guarded wire surface exactly as the app mounts it — minus app
/// auth (tests inject `AuthPartnerId` directly, the contract the app's
/// user-scope verifier fulfills) — plus the bare webhook group.
fn wire(module: Arc<MessagingModule>) -> axum::Router {
    use axum::middleware as axum_mw;
    axum::Router::new()
        .merge(thread_routes::composer())
        .merge(channel_routes::composer())
        .merge(guest_routes::composer())
        .merge(presence_routes::composer().layer(axum::Extension(SessionSecret(Arc::new(
            b"route-test-secret".to_vec(),
        )))))
        .merge(attachment_routes::composer())
        .merge(public_routes::composer())
        .merge(webhook_routes::composer())
        .layer(axum_mw::from_fn(guest_context))
        .with_state(module)
}

async fn module() -> Option<Arc<MessagingModule>> {
    let pool = common::test_pool().await?;
    let module = MessagingModule::builder().with_database(pool).build().ok()?;
    Some(Arc::new(module))
}

fn json_req(method: &str, uri: &str, body: Option<serde_json::Value>) -> axum::http::Request<axum::body::Body> {
    let mut b = axum::http::Request::builder().method(method).uri(uri);
    if body.is_some() {
        b = b.header(axum::http::header::CONTENT_TYPE, "application/json");
    }
    let req = if let Some(v) = body {
        b.body(axum::body::Body::from(v.to_string())).unwrap()
    } else {
        b.body(axum::body::Body::empty()).unwrap()
    };
    req
}

fn as_partner(mut req: axum::http::Request<axum::body::Body>, partner: Uuid) -> axum::http::Request<axum::body::Body> {
    req.extensions_mut().insert(AuthPartnerId(partner));
    req
}

/// Guests take the REAL path — the dgid cookie the middleware resolves.
fn as_guest(mut req: axum::http::Request<axum::body::Body>, guest: Uuid) -> axum::http::Request<axum::body::Body> {
    req.headers_mut()
        .insert(axum::http::header::COOKIE, format!("dgid={guest}").parse().unwrap());
    req
}

async fn status(router: &axum::Router, req: axum::http::Request<axum::body::Body>) -> (axum::http::StatusCode, Option<serde_json::Value>) {
    let res = router.clone().oneshot(req).await.expect("oneshot");
    let code = res.status();
    let bytes = axum::body::to_bytes(res.into_body(), 64 * 1024).await.expect("body");
    let json = if bytes.is_empty() { None } else { serde_json::from_slice(&bytes).ok() };
    (code, json)
}

// ---------------------------------------------------------------------------
// Authz matrix: partner / guest / anonymous × allowed / denied
// ---------------------------------------------------------------------------

#[tokio::test]
async fn authz_matrix_partner_guest_anonymous() {
    let Some(m) = module().await else { return common::skipped("authz_matrix") };
    let partner = Uuid::new_v4();
    let guest = Uuid::new_v4();
    let model = "crm.lead";
    let res_id = Uuid::new_v4();

    let router = wire(m);

    // Anonymous: guarded verbs are 401 — no identity, no surface.
    for (method, uri, body) in [
        ("POST", "/mail/thread/messages", Some(serde_json::json!({"model": model, "res_id": res_id}))),
        ("POST", "/mail/thread/post", Some(serde_json::json!({"model": model, "res_id": res_id, "body": "x"}))),
        ("POST", "/mail/thread/follow", Some(serde_json::json!({"model": model, "res_id": res_id}))),
        ("GET", "/mail/inbox/unread_count", None),
    ] {
        let (code, _) = status(&router, json_req(method, uri, body)).await;
        assert_eq!(code, axum::http::StatusCode::UNAUTHORIZED, "{uri}");
    }

    // Partner on an UNREGISTERED model: deny-by-default (MAIL-B1) is a 403,
    // not a silent empty set — proven through the whole HTTP stack.
    let (code, body) = status(
        &router,
        as_partner(
            json_req("POST", "/mail/thread/messages", Some(serde_json::json!({"model": model, "res_id": res_id}))),
            partner,
        ),
    )
    .await;
    assert_eq!(code, axum::http::StatusCode::FORBIDDEN);
    assert!(body.unwrap()["error"].as_str().unwrap().contains(model));

    // Guest: identified, but the partner-only inbox verb is still closed
    // (fetch_inbox's partner gate → 403 with a guest identity).
    let (code, _) = status(
        &router,
        as_guest(json_req("POST", "/mail/inbox/messages", None), guest),
    )
    .await;
    assert_eq!(code, axum::http::StatusCode::FORBIDDEN);

    // Partner: the inbox counter works (empty inbox, zero channels — the
    // partner-gated happy path).
    let (code, body) = status(
        &router,
        as_partner(json_req("GET", "/mail/inbox/unread_count", None), partner),
    )
    .await;
    assert_eq!(code, axum::http::StatusCode::OK);
    assert_eq!(body.unwrap()["count"].as_i64().unwrap(), 0);

    // Guest on an unregistered thread read: same deny-by-default 403.
    let (code, _) = status(
        &router,
        as_guest(
            json_req("POST", "/mail/thread/messages", Some(serde_json::json!({"model": model, "res_id": res_id}))),
            guest,
        ),
    )
    .await;
    assert_eq!(code, axum::http::StatusCode::FORBIDDEN);

}

// ---------------------------------------------------------------------------
// Throttle: the public bootstrap hard-limits (10/60s)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn throttle_returns_429_after_limit() {
    let Some(m) = module().await else { return common::skipped("throttle") };
    let router = wire(m);

    let body = serde_json::json!({"channel_uuid": Uuid::new_v4().to_string()});
    let mut last = axum::http::StatusCode::OK;
    for i in 0..11 {
        let mut req = json_req("POST", "/discuss/public/bootstrap", Some(body.clone()));
        // Same key = same caller bucket (the crate keys off this header).
        req.headers_mut().insert("x-rate-limit-key", "throttle-test".parse().unwrap());
        let (code, _) = status(&router, req).await;
        last = code;
        if i == 10 {
            assert_eq!(code, axum::http::StatusCode::TOO_MANY_REQUESTS, "11th hit must 429");
        } else {
            assert_eq!(code, axum::http::StatusCode::NOT_FOUND, "under limit: unknown uuid is a 404");
        }
    }
    assert_eq!(last, axum::http::StatusCode::TOO_MANY_REQUESTS);
}

// ---------------------------------------------------------------------------
// SSE-session proof gates bus presence (is_websocket_session port)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn bus_presence_requires_live_session_proof() {
    let Some(m) = module().await else { return common::skipped("bus_presence") };
    let pool = m.db_pool();
    let partner = Uuid::new_v4();
    let identity = MessagingIdentity::User { partner_id: partner };
    let secret = b"route-test-secret".to_vec();
    let router = wire(m);

    // No proof → 403 (ProofRequired).
    let (code, body) = status(
        &router,
        as_partner(json_req("POST", "/mail/presence/bus", None), partner),
    )
    .await;
    assert_eq!(code, axum::http::StatusCode::FORBIDDEN);
    assert!(body.unwrap()["error"].as_str().unwrap().contains("session proof"));

    // Someone ELSE's proof → still 403 (identity-bound verification).
    let other = mint_session_token(&secret, &MessagingIdentity::User { partner_id: Uuid::new_v4() }, 60);
    let mut req = as_partner(json_req("POST", "/mail/presence/bus", None), partner);
    req.headers_mut().insert("x-messaging-session", other.parse().unwrap());
    let (code, _) = status(&router, req).await;
    assert_eq!(code, axum::http::StatusCode::FORBIDDEN);

    // Own proof → 200.
    let proof = mint_session_token(&secret, &identity, 60);
    let mut req = as_partner(json_req("POST", "/mail/presence/bus", None), partner);
    req.headers_mut().insert("x-messaging-session", proof.parse().unwrap());
    let (code, _) = status(&router, req).await;
    assert_eq!(code, axum::http::StatusCode::OK);

    // The liveness row we refreshed is ours alone.
    sqlx::query("DELETE FROM messaging.mail_presences WHERE user_id = $1")
        .bind(partner)
        .execute(&pool)
        .await
        .ok();
}

// ---------------------------------------------------------------------------
// Guest mint + rename over the wire (Set-Cookie is the credential)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn guest_mint_sets_cookie_and_rename_is_self_only() {
    let Some(m) = module().await else { return common::skipped("guest_wire") };
    let pool = m.db_pool();
    let router = wire(m);

    // Anonymous mint: 200 + a dgid Set-Cookie.
    let res = router
        .clone()
        .oneshot(json_req("POST", "/mail/guest", Some(serde_json::json!({"name": "Visitor"}))))
        .await
        .expect("oneshot");
    assert_eq!(res.status(), axum::http::StatusCode::OK);
    let cookie = res
        .headers()
        .get(axum::http::header::SET_COOKIE)
        .and_then(|v| v.to_str().ok())
        .expect("dgid cookie")
        .to_string();
    assert!(cookie.starts_with("dgid=") && cookie.contains("HttpOnly"));
    let guest_id: Uuid = cookie.trim_start_matches("dgid=").split(';').next().unwrap().parse().unwrap();

    // A cookie-bearing rename works — send the cookie like a browser would.
    let mut req = json_req("POST", "/mail/guest/update_name", Some(serde_json::json!({"name": "Renamed"})));
    req.headers_mut().insert(axum::http::header::COOKIE, format!("dgid={guest_id}").parse().unwrap());
    let (code, _) = status(&router, req).await;
    assert_eq!(code, axum::http::StatusCode::OK);

    // A DIFFERENT guest's cookie cannot rename this guest.
    let mut req = json_req("POST", "/mail/guest/update_name", Some(serde_json::json!({"name": "Hijack"})));
    req.headers_mut().insert(axum::http::header::COOKIE, format!("dgid={}", Uuid::new_v4()).parse().unwrap());
    let (code, _) = status(&router, req).await;
    assert_eq!(code, axum::http::StatusCode::NOT_FOUND, "unknown guest cookie: not-found, no oracle");

    // Anonymous rename: 401.
    let (code, _) = status(
        &router,
        json_req("POST", "/mail/guest/update_name", Some(serde_json::json!({"name": "X"}))),
    )
    .await;
    assert_eq!(code, axum::http::StatusCode::UNAUTHORIZED);

    // An already-identified caller cannot mint a second persona.
    let partner = Uuid::new_v4();
    let (code, _) = status(
        &router,
        as_partner(json_req("POST", "/mail/guest", None), partner),
    )
    .await;
    assert_eq!(code, axum::http::StatusCode::UNAUTHORIZED);

    sqlx::query("DELETE FROM messaging.outbox_events WHERE aggregate_id = $1")
        .bind(guest_id.to_string())
        .execute(&pool)
        .await
        .ok();
    sqlx::query("DELETE FROM messaging.mail_guests WHERE id = $1")
        .bind(guest_id)
        .execute(&pool)
        .await
        .ok();
}

// ---------------------------------------------------------------------------
// Webhook over HTTP: raw bytes + X-Signature, fail-closed end-to-end
// ---------------------------------------------------------------------------

#[tokio::test]
async fn webhook_http_fail_closed_and_replay() {
    let Some(m) = module().await else { return common::skipped("webhook_http") };
    let pool = m.db_pool();
    // The advance pump and the concurrent-drainer proofs share this DB:
    // serialize the mutating window (same convention as increment2).
    let _drain_guard = common::DRAIN_LOCK.lock().await;
    common::sweep_queues(&pool).await;
    let router = wire(m);

    // Set the secret for this process (the route reads the env reference).
    std::env::set_var("SMS_WEBHOOK_SECRET", "route-test-webhook-secret");

    // Seed an outgoing sms the callback can advance.
    let sms_uuid = Uuid::new_v4().to_string();
    sqlx::query(
        r#"INSERT INTO messaging.sms (id, uuid, number, body, state)
           VALUES ($1, $2, '+15550001111', 'route test', 'process')"#,
    )
    .bind(Uuid::new_v4())
    .bind(&sms_uuid)
    .execute(&pool)
    .await
    .expect("seed sms");

    let payload = serde_json::json!({
        "timestamp": chrono::Utc::now(),
        "sms_uuid": sms_uuid,
        "status": "sent",
    })
    .to_string();

    // Missing header → 401, row untouched.
    let (code, _) = status(
        &router,
        json_req("POST", "/sms/status", Some(serde_json::from_str(&payload).unwrap())),
    )
    .await;
    assert_eq!(code, axum::http::StatusCode::UNAUTHORIZED);
    let state: String = sqlx::query_scalar("SELECT state::text FROM messaging.sms WHERE uuid = $1")
        .bind(&sms_uuid)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(state, "process");

    // Bad signature → 401, row still untouched (fail-closed over HTTP).
    let mut req = json_req("POST", "/sms/status", None);
    *req.body_mut() = axum::body::Body::from(payload.clone());
    req.headers_mut().insert("x-signature", "0".repeat(64).parse().unwrap());
    let (code, _) = status(&router, req).await;
    assert_eq!(code, axum::http::StatusCode::UNAUTHORIZED);
    let state: String = sqlx::query_scalar("SELECT state::text FROM messaging.sms WHERE uuid = $1")
        .bind(&sms_uuid)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(state, "process");

    // Valid signature (MAC over the EXACT bytes) → advanced.
    let sig = {
        use hmac::{Hmac, Mac};
        use sha2::Sha256;
        let mut mac = Hmac::<Sha256>::new_from_slice(b"route-test-webhook-secret").unwrap();
        mac.update(payload.as_bytes());
        let out: [u8; 32] = mac.finalize().into_bytes().into();
        out.iter().map(|b| format!("{b:02x}")).collect::<String>()
    };
    let mut req = json_req("POST", "/sms/status", None);
    *req.body_mut() = axum::body::Body::from(payload.clone());
    req.headers_mut().insert("x-signature", sig.parse().unwrap());
    let (code, body) = status(&router, req).await;
    assert_eq!(code, axum::http::StatusCode::OK);
    assert_eq!(body.unwrap()["outcome"].as_str().unwrap(), "advanced");

    // Replay (same signed body again) → 200 replay, no double-advance.
    let mut req = json_req("POST", "/sms/status", None);
    *req.body_mut() = axum::body::Body::from(payload);
    req.headers_mut().insert("x-signature", sig.parse().unwrap());
    let (code, body) = status(&router, req).await;
    assert_eq!(code, axum::http::StatusCode::OK);
    assert_eq!(body.unwrap()["outcome"].as_str().unwrap(), "replay");

    sqlx::query("DELETE FROM messaging.sms_trackers WHERE sms_uuid = $1")
        .bind(&sms_uuid)
        .execute(&pool)
        .await
        .ok();
    sqlx::query("DELETE FROM messaging.outbox_events WHERE aggregate_id = $1")
        .bind(&sms_uuid)
        .execute(&pool)
        .await
        .ok();
    sqlx::query("DELETE FROM messaging.sms WHERE uuid = $1")
        .bind(&sms_uuid)
        .execute(&pool)
        .await
        .ok();
}

// ---------------------------------------------------------------------------
// ADR-0019 GET-snapshot: the surface's GETs leave rows unchanged
// ---------------------------------------------------------------------------

#[tokio::test]
async fn get_endpoints_never_mutate() {
    let Some(m) = module().await else { return common::skipped("get_snapshot") };
    let pool = m.db_pool();
    let partner = Uuid::new_v4();
    let router = wire(m);

    async fn snapshot(pool: &sqlx::PgPool, partner: uuid::Uuid) -> (i64, i64) {
        let msgs: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM messaging.mail_messages WHERE author_id = $1",
        )
        .bind(partner)
        .fetch_one(pool)
        .await
        .unwrap();
        let events: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM messaging.outbox_events",
        )
        .fetch_one(pool)
        .await
        .unwrap();
        (msgs, events)
    }

    let before = snapshot(&pool, partner).await;

    // Every GET on the surface, twice each (idempotence + no mutation).
    for uri in ["/mail/inbox/unread_count", "/mail/channels/unread_counts"] {
        for _ in 0..2 {
            let (code, _) = status(
                &router,
                as_partner(json_req("GET", uri, None), partner),
            )
            .await;
            assert_eq!(code, axum::http::StatusCode::OK, "{uri}");
        }
    }
    // The discuss member/pinned GETs on a random channel: 403 (non-member),
    // still no mutation.
    let ch = Uuid::new_v4();
    for uri in [format!("/discuss/channel/{ch}/members"), format!("/discuss/channel/{ch}/pinned")] {
        let (code, _) = status(
            &router,
            as_partner(json_req("GET", &uri, None), partner),
        )
        .await;
        assert_eq!(code, axum::http::StatusCode::FORBIDDEN, "{uri}");
    }

    // Outbox events move globally (shared DB) — assert OUR partner's rows
    // and that no event carries our partner as aggregate.
    let after_msgs = snapshot(&pool, partner).await.0;
    assert_eq!(after_msgs, before.0, "GETs must not create messages");
    let own_events: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM messaging.outbox_events WHERE payload->'message'->'payload'->>'partner_id' = $1")
            .bind(partner.to_string())
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(own_events, 0, "GETs must not stage bus events for the caller");
}
