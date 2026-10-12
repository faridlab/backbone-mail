//! Who may change what on the messaging surface (live DB, in-process router): a caller joins only
//! a public channel, typing never adds a member, a guest opens no channel, only its author cancels
//! a scheduled message, and a file goes only onto a message its owner wrote.
//!
//! MA-1..MA-5, one per case below.

use std::sync::Arc;

use backbone_mail::presentation::http::{attachment_routes, channel_routes, thread_routes};
use backbone_mail::presentation::middleware::{guest_context, AuthPartnerId};
use backbone_mail::MessagingModule;
use tower::ServiceExt;
use uuid::Uuid;

use super::common;

fn wire(module: Arc<MessagingModule>) -> axum::Router {
    axum::Router::new()
        .merge(thread_routes::composer())
        .merge(channel_routes::composer())
        .merge(attachment_routes::composer())
        .layer(axum::middleware::from_fn(guest_context))
        .with_state(module)
}

async fn setup() -> Option<(sqlx::PgPool, axum::Router)> {
    let pool = common::test_pool().await?;
    let module = MessagingModule::builder().with_database(pool.clone()).build().ok()?;
    Some((pool, wire(Arc::new(module))))
}

fn req(method: &str, uri: &str, body: Option<serde_json::Value>) -> axum::http::Request<axum::body::Body> {
    let b = axum::http::Request::builder().method(method).uri(uri);
    match body {
        Some(v) => b
            .header(axum::http::header::CONTENT_TYPE, "application/json")
            .body(axum::body::Body::from(v.to_string()))
            .unwrap(),
        None => b.body(axum::body::Body::empty()).unwrap(),
    }
}

fn as_partner(mut r: axum::http::Request<axum::body::Body>, partner: Uuid) -> axum::http::Request<axum::body::Body> {
    r.extensions_mut().insert(AuthPartnerId(partner));
    r
}

fn as_guest(mut r: axum::http::Request<axum::body::Body>, guest: Uuid) -> axum::http::Request<axum::body::Body> {
    r.headers_mut()
        .insert(axum::http::header::COOKIE, format!("dgid={guest}").parse().unwrap());
    r
}

async fn send(router: &axum::Router, r: axum::http::Request<axum::body::Body>) -> (u16, serde_json::Value) {
    let res = router.clone().oneshot(r).await.expect("oneshot");
    let code = res.status().as_u16();
    let bytes = axum::body::to_bytes(res.into_body(), 64 * 1024).await.expect("body");
    (code, serde_json::from_slice(&bytes).unwrap_or(serde_json::Value::Null))
}

async fn channel(router: &axum::Router, owner: Uuid, access_mode: &str) -> Uuid {
    let (code, body) = send(router, as_partner(req("POST", "/discuss/channel", Some(serde_json::json!({
        "name": format!("ma-{}", &Uuid::new_v4().simple().to_string()[..6]),
        "channel_type": "channel",
        "default_access_mode": access_mode,
    }))), owner)).await;
    assert_eq!(code, 200, "create channel: {body}");
    body["channel_id"].as_str().unwrap().parse().unwrap()
}

async fn members_of(pool: &sqlx::PgPool, channel: Uuid, partner: Uuid) -> i64 {
    sqlx::query_scalar("SELECT count(*) FROM messaging.discuss_channel_members WHERE channel_id=$1 AND partner_id=$2")
        .bind(channel).bind(partner).fetch_one(pool).await.unwrap()
}

/// MA-1: a caller joins a public channel by themselves; a private one takes members by invitation.
#[tokio::test]
async fn only_a_public_channel_is_open_to_join() {
    let Some((pool, router)) = setup().await else { common::skipped("only_a_public_channel_is_open_to_join"); return; };
    let (owner, outsider) = (Uuid::new_v4(), Uuid::new_v4());
    let private = channel(&router, owner, "private").await;
    let public = channel(&router, owner, "public").await;

    let (code, _) = send(&router, as_partner(req("POST", &format!("/discuss/channel/{private}/join"), None), outsider)).await;
    assert_eq!(code, 403, "an outsider cannot join a private channel");
    assert_eq!(members_of(&pool, private, outsider).await, 0);
    let (code, _) = send(&router, as_guest(req("POST", &format!("/discuss/channel/{private}/join"), None), Uuid::new_v4())).await;
    assert_eq!(code, 403, "nor can any guest cookie");

    let (code, _) = send(&router, as_partner(req("POST", &format!("/discuss/channel/{public}/join"), None), outsider)).await;
    assert_eq!(code, 200, "anyone joins a public channel");
    assert_eq!(members_of(&pool, public, outsider).await, 1);

    let (code, _) = send(&router, as_partner(req("POST", &format!("/discuss/channel/{private}/join"), None), owner)).await;
    assert_eq!(code, 200, "a member re-joining is a no-op");
}

/// MA-2: typing in a channel needs membership and never grants it.
#[tokio::test]
async fn typing_does_not_make_a_member() {
    let Some((pool, router)) = setup().await else { common::skipped("typing_does_not_make_a_member"); return; };
    let (owner, outsider) = (Uuid::new_v4(), Uuid::new_v4());
    let private = channel(&router, owner, "private").await;
    let (code, _) = send(&router, as_partner(req("POST", &format!("/discuss/channel/{private}/typing"),
        Some(serde_json::json!({"typing": true}))), outsider)).await;
    assert_eq!(code, 403);
    assert_eq!(members_of(&pool, private, outsider).await, 0, "the indicator added no member");
    let (code, _) = send(&router, as_partner(req("POST", &format!("/discuss/channel/{private}/typing"),
        Some(serde_json::json!({"typing": true}))), owner)).await;
    assert_eq!(code, 200, "a member types");
}

/// MA-3: a guest opens no channel, and a channel's access mode is one the module knows.
#[tokio::test]
async fn a_guest_opens_no_channel() {
    let Some((_pool, router)) = setup().await else { common::skipped("a_guest_opens_no_channel"); return; };
    let (code, _) = send(&router, as_guest(req("POST", "/discuss/channel", Some(serde_json::json!({
        "name": "guest-room", "channel_type": "channel", "default_access_mode": "public",
    }))), Uuid::new_v4())).await;
    assert_eq!(code, 403);
    let (code, _) = send(&router, as_partner(req("POST", "/discuss/channel", Some(serde_json::json!({
        "name": "odd-room", "channel_type": "channel", "default_access_mode": "everyone",
    }))), Uuid::new_v4())).await;
    assert_eq!(code, 422);
}

/// MA-4: only its author cancels a scheduled message.
#[tokio::test]
async fn only_the_author_cancels_a_scheduled_message() {
    let Some((pool, router)) = setup().await else { common::skipped("only_the_author_cancels_a_scheduled_message"); return; };
    let (author, other) = (Uuid::new_v4(), Uuid::new_v4());
    let id: Uuid = sqlx::query_scalar(
        "INSERT INTO messaging.mail_scheduled_messages (body, scheduled_date, model, res_id, author_party_id) \
         VALUES ('later', now() + interval '1 day', 'res.partner', $1, $1) RETURNING id",
    )
    .bind(author).fetch_one(&pool).await.unwrap();
    let path = format!("/mail/thread/schedule/{id}/cancel");

    let (code, _) = send(&router, as_partner(req("POST", &path, None), other)).await;
    assert_eq!(code, 403, "another partner cannot cancel it");
    let (code, _) = send(&router, as_guest(req("POST", &path, None), Uuid::new_v4())).await;
    assert_eq!(code, 401, "a guest authors no scheduled message");
    let left: i64 = sqlx::query_scalar("SELECT count(*) FROM messaging.mail_scheduled_messages WHERE id=$1")
        .bind(id).fetch_one(&pool).await.unwrap();
    assert_eq!(left, 1, "still pending");

    let (code, _) = send(&router, as_partner(req("POST", &path, None), author)).await;
    assert_eq!(code, 200, "its author cancels it");
}

/// MA-5: owning a file is not a licence to add it to someone else's message.
#[tokio::test]
async fn a_file_goes_only_onto_ones_own_message() {
    let Some((pool, router)) = setup().await else { common::skipped("a_file_goes_only_onto_ones_own_message"); return; };
    let (me, someone) = (Uuid::new_v4(), Uuid::new_v4());
    let theirs: Uuid = sqlx::query_scalar("INSERT INTO messaging.mail_messages (body, author_id) VALUES ('theirs', $1) RETURNING id")
        .bind(someone).fetch_one(&pool).await.unwrap();
    let mine: Uuid = sqlx::query_scalar("INSERT INTO messaging.mail_messages (body, author_id) VALUES ('mine', $1) RETURNING id")
        .bind(me).fetch_one(&pool).await.unwrap();
    let (code, body) = send(&router, as_partner(req("POST", "/mail/attachment",
        Some(serde_json::json!({"name": "note.txt", "datas": "aGk="}))), me)).await;
    assert_eq!(code, 200, "register: {body}");
    let attachment = body["attachment_id"].as_str().unwrap().to_string();

    let (code, _) = send(&router, as_partner(req("POST", &format!("/mail/attachment/{attachment}/attach"),
        Some(serde_json::json!({"message_id": theirs}))), me)).await;
    assert_eq!(code, 403, "not onto someone else's message");
    let (code, _) = send(&router, as_partner(req("POST", &format!("/mail/attachment/{attachment}/attach"),
        Some(serde_json::json!({"message_id": mine}))), me)).await;
    assert_eq!(code, 200, "onto one's own");
}
