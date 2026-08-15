//! Thread/chatter routes (hand-written; user-owned).
//!
//! The `message_fetch` + `message_post` wire surface over the MAIL-B1-gated
//! services: thread/channel/inbox/starred fetches, the unread counters,
//! recipient suggestions, and the host-chatter verbs (post/follow/unfollow/
//! schedule). Every handler reads its identity from the guest middleware's
//! [`WireIdentity`] extension and applies its procedural gate BEFORE any
//! query runs. All fetches are POST (they carry cursor bodies — ADR-0019:
//! no mutating GETs, and reads-with-body stay POST too); the pure counter
//! reads are GET (the audit surface's only GETs: these two, the discuss
//! member/pinned reads, the SSE stream, and the read-only bootstrap).

use std::sync::Arc;

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::Deserialize;
use uuid::Uuid;

use crate::application::service::chatter_acl::MessagingIdentity;
use crate::application::service::{ChatterError, MessageQueryError, RecipientQueryError};
use crate::MessagingModule;
use crate::presentation::middleware::{unauthorized, WireIdentity};

/// Shared route state for every hand-written messaging router.
pub type ApiState = Arc<MessagingModule>;

/// Extract any identified caller or reject (anonymous is nobody — M43).
pub fn require_identity(identity: &WireIdentity) -> Result<MessagingIdentity, Response> {
    identity.identity().copied().ok_or_else(unauthorized)
}

/// Map a chatter error to a JSON response without leaking internals.
pub fn chatter_err(e: ChatterError) -> Response {
    match e {
        ChatterError::CannotPost(m, id) => (
            StatusCode::FORBIDDEN,
            Json(serde_json::json!({"error": format!("cannot post on {m} {id}")})),
        )
            .into_response(),
        ChatterError::CannotRead(m, id) => (
            StatusCode::FORBIDDEN,
            Json(serde_json::json!({"error": format!("cannot read {m} {id}")})),
        )
            .into_response(),
        ChatterError::Invalid(m) => {
            (StatusCode::UNPROCESSABLE_ENTITY, Json(json_err(&m))).into_response()
        }
        ChatterError::Db(e) => db_err(e),
        ChatterError::Query(e) => query_err(e),
        _ => (StatusCode::INTERNAL_SERVER_ERROR, Json(json_err("internal error"))).into_response(),
    }
}

pub fn query_err(e: MessageQueryError) -> Response {
    match e {
        MessageQueryError::Forbidden(m, id) => (
            StatusCode::FORBIDDEN,
            Json(serde_json::json!({"error": format!("no read access to {m} {id}")})),
        )
            .into_response(),
        MessageQueryError::NotAMember => {
            (StatusCode::FORBIDDEN, Json(json_err("not a channel member"))).into_response()
        }
        MessageQueryError::Db(err) => db_err(err),
    }
}

pub fn recipient_err(e: RecipientQueryError) -> Response {
    match e {
        RecipientQueryError::Forbidden(m, id) => (
            StatusCode::FORBIDDEN,
            Json(serde_json::json!({"error": format!("no read access to {m} {id}")})),
        )
            .into_response(),
        RecipientQueryError::NeedsPartner => unauthorized(),
        RecipientQueryError::Db(err) => db_err(err),
    }
}

pub fn json_err(msg: &str) -> serde_json::Value {
    serde_json::json!({ "error": msg })
}

pub fn db_err(e: sqlx::Error) -> Response {
    tracing::error!(error = %e, "messaging route db error");
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(json_err("internal error")),
    )
        .into_response()
}

pub fn ok_json(v: impl serde::Serialize) -> Response {
    (StatusCode::OK, Json(v)).into_response()
}

// ---------------------------------------------------------------------------
// Handlers
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
pub struct FetchThreadBody {
    pub model: String,
    pub res_id: Uuid,
    pub after_id: Option<Uuid>,
}

async fn fetch_thread(
    State(app): State<ApiState>,
    axum::Extension(identity): axum::Extension<WireIdentity>,
    Json(body): Json<FetchThreadBody>,
) -> Response {
    let Ok(id) = require_identity(&identity) else { return unauthorized() };
    match app
        .message_query_service
        .fetch_thread(&id, &body.model, body.res_id, body.after_id)
        .await
    {
        Ok(msgs) => ok_json(&msgs),
        Err(e) => query_err(e),
    }
}

#[derive(Deserialize)]
pub struct FetchChannelBody {
    pub channel_id: Uuid,
    pub after_id: Option<Uuid>,
}

async fn fetch_channel(
    State(app): State<ApiState>,
    axum::Extension(identity): axum::Extension<WireIdentity>,
    Json(body): Json<FetchChannelBody>,
) -> Response {
    let Ok(id) = require_identity(&identity) else { return unauthorized() };
    match app
        .message_query_service
        .fetch_channel(&id, body.channel_id, body.after_id)
        .await
    {
        Ok(msgs) => ok_json(&msgs),
        Err(e) => query_err(e),
    }
}

async fn fetch_inbox(
    State(app): State<ApiState>,
    axum::Extension(identity): axum::Extension<WireIdentity>,
) -> Response {
    let Ok(id) = require_identity(&identity) else { return unauthorized() };
    match app.message_query_service.fetch_inbox(&id).await {
        Ok(msgs) => ok_json(&msgs),
        Err(e) => query_err(e),
    }
}

async fn fetch_starred(
    State(app): State<ApiState>,
    axum::Extension(identity): axum::Extension<WireIdentity>,
) -> Response {
    let Ok(id) = require_identity(&identity) else { return unauthorized() };
    match app.message_query_service.fetch_starred(&id).await {
        Ok(msgs) => ok_json(&msgs),
        Err(e) => query_err(e),
    }
}

async fn inbox_unread(
    State(app): State<ApiState>,
    axum::Extension(identity): axum::Extension<WireIdentity>,
) -> Response {
    let Ok(id) = require_identity(&identity) else { return unauthorized() };
    match app.message_query_service.inbox_unread_count(&id).await {
        Ok(n) => ok_json(serde_json::json!({ "count": n })),
        Err(e) => query_err(e),
    }
}

async fn channel_unread(
    State(app): State<ApiState>,
    axum::Extension(identity): axum::Extension<WireIdentity>,
) -> Response {
    let Ok(id) = require_identity(&identity) else { return unauthorized() };
    match app.message_query_service.channel_unread_counts(&id).await {
        Ok(rows) => ok_json(serde_json::json!({ "channels": rows
            .into_iter()
            .map(|(channel_id, unread)| serde_json::json!({
                "channel_id": channel_id,
                "unread": unread,
            }))
            .collect::<Vec<_>>() })),
        Err(e) => query_err(e),
    }
}

#[derive(Deserialize)]
pub struct RecipientsBody {
    pub model: String,
    pub res_id: Uuid,
}

async fn suggested_recipients(
    State(app): State<ApiState>,
    axum::Extension(identity): axum::Extension<WireIdentity>,
    Json(body): Json<RecipientsBody>,
) -> Response {
    let Ok(id) = require_identity(&identity) else { return unauthorized() };
    match app
        .recipient_query_service
        .suggested(&id, &body.model, body.res_id)
        .await
    {
        Ok(list) => ok_json(&list),
        Err(e) => recipient_err(e),
    }
}

#[derive(Deserialize)]
pub struct PostBody {
    pub model: String,
    pub res_id: Uuid,
    pub body: String,
    pub subject: Option<String>,
    pub is_note: Option<bool>,
}

async fn post_to_thread(
    State(app): State<ApiState>,
    axum::Extension(identity): axum::Extension<WireIdentity>,
    Json(body): Json<PostBody>,
) -> Response {
    let Ok(id) = require_identity(&identity) else { return unauthorized() };
    match app
        .thread_chatter_service
        .post(
            &id,
            &body.model,
            body.res_id,
            &body.body,
            body.subject.as_deref(),
            body.is_note.unwrap_or(false),
        )
        .await
    {
        Ok(posted) => ok_json(serde_json::json!({
            "message_id": posted.message_id,
            "notifications": posted.notifications.len(),
        })),
        Err(e) => chatter_err(e),
    }
}

#[derive(Deserialize)]
pub struct FollowBody {
    pub model: String,
    pub res_id: Uuid,
}

async fn follow_thread(
    State(app): State<ApiState>,
    axum::Extension(identity): axum::Extension<WireIdentity>,
    Json(body): Json<FollowBody>,
) -> Response {
    let Some(partner) = identity.partner_id() else { return unauthorized() };
    match app.thread_chatter_service.follow(partner, &body.model, body.res_id).await {
        Ok(_) => ok_json(serde_json::json!({ "followed": true })),
        Err(e) => chatter_err(e),
    }
}

async fn unfollow_thread(
    State(app): State<ApiState>,
    axum::Extension(identity): axum::Extension<WireIdentity>,
    Json(body): Json<FollowBody>,
) -> Response {
    let Some(partner) = identity.partner_id() else { return unauthorized() };
    match app
        .thread_chatter_service
        .unfollow(partner, &body.model, body.res_id)
        .await
    {
        Ok(n) => ok_json(serde_json::json!({ "removed": n })),
        Err(e) => chatter_err(e),
    }
}

#[derive(Deserialize)]
pub struct ScheduleBody {
    pub model: String,
    pub res_id: Uuid,
    pub body: String,
    pub subject: Option<String>,
    pub scheduled_date: chrono::DateTime<chrono::Utc>,
    pub is_note: Option<bool>,
}

async fn schedule_thread_post(
    State(app): State<ApiState>,
    axum::Extension(identity): axum::Extension<WireIdentity>,
    Json(body): Json<ScheduleBody>,
) -> Response {
    let Some(partner) = identity.partner_id() else { return unauthorized() };
    match app
        .thread_chatter_service
        .schedule_post(
            partner,
            &body.model,
            body.res_id,
            &body.body,
            body.subject.as_deref(),
            body.scheduled_date,
            body.is_note.unwrap_or(false),
        )
        .await
    {
        Ok(id) => ok_json(serde_json::json!({ "scheduled_message_id": id })),
        Err(e) => chatter_err(e),
    }
}

/// Cancel a scheduled post by id (author-side cancel; M9 delete-shaped).
async fn cancel_scheduled(
    State(app): State<ApiState>,
    axum::Extension(identity): axum::Extension<WireIdentity>,
    Path(id): Path<Uuid>,
) -> Response {
    if identity.partner_id().is_none() && !identity.is_guest() {
        return unauthorized();
    }
    match app.schedule_write_service.cancel_scheduled_message(id).await {
        Ok(()) => ok_json(serde_json::json!({ "canceled": true })),
        Err(e) => match e {
            crate::application::service::schedule_write_service::ScheduleError::NotFound(_) => {
                (StatusCode::NOT_FOUND, Json(json_err("no such scheduled message")))
                    .into_response()
            }
            crate::application::service::schedule_write_service::ScheduleError::Invalid(m) => {
                (StatusCode::UNPROCESSABLE_ENTITY, Json(json_err(&m))).into_response()
            }
            other => (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json_err(&format!("{other}"))),
            )
                .into_response(),
        },
    }
}

// ---------------------------------------------------------------------------
// Composer
// ---------------------------------------------------------------------------

/// The thread/chatter route group (guarded surface — mount behind app user
/// auth + the guest middleware). Recipient suggestions are throttled 30/60s
/// (the enumeration-shaped read of the whole follower/author graph).
pub fn composer() -> Router<ApiState> {
    use axum::middleware as axum_mw;

    let throttled = Router::<ApiState>::new()
        .route("/mail/thread/recipients", post(suggested_recipients))
        .route_layer(axum_mw::from_fn_with_state(
            backbone_rate_limit::middleware(30, 60),
            backbone_rate_limit::rate_limit_middleware,
        ));

    Router::new()
        .route("/mail/thread/messages", post(fetch_thread))
        .route("/mail/channel/messages", post(fetch_channel))
        .route("/mail/inbox/messages", post(fetch_inbox))
        .route("/mail/inbox/starred", post(fetch_starred))
        .route("/mail/inbox/unread_count", get(inbox_unread))
        .route("/mail/channels/unread_counts", get(channel_unread))
        .route("/mail/thread/post", post(post_to_thread))
        .route("/mail/thread/follow", post(follow_thread))
        .route("/mail/thread/unfollow", post(unfollow_thread))
        .route("/mail/thread/schedule", post(schedule_thread_post))
        .route("/mail/thread/schedule/:id/cancel", post(cancel_scheduled))
        .merge(throttled)
}
