//! Discuss channel routes (hand-written; user-owned).
//!
//! The `/discuss` wire surface: channel/group creation, the 1:1 chat dedup
//! verb, member state verbs (join/leave/mute/notifications/pin/fold/seen/
//! fetched/separator), typing notifications, search, reactions, message
//! edits (author-or-admin), stars, and channel-level message pinning.
//! Identity comes from the guest middleware's [`WireIdentity`] extension;
//! membership gates live in the services (`NotMember` → 403). Search is the
//! one throttled verb here (30/60s — an enumeration-shaped lookup).

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::Deserialize;
use uuid::Uuid;

use crate::application::service::channel_write_service::CreateChannelCommand;
use crate::application::service::{
    ChannelError, ChannelQueryError, MemberError, MessageEditError, ReactionError, TypingError,
};
use crate::presentation::http::thread_routes::{
    db_err, json_err, ok_json, require_identity, ApiState,
};
use crate::presentation::middleware::{forbidden, unauthorized, IsAdmin, WireIdentity};

fn member_err(e: MemberError) -> Response {
    match e {
        MemberError::NotMember(id) => (
            StatusCode::FORBIDDEN,
            Json(serde_json::json!({"error": format!("not a member of channel {id}")})),
        )
            .into_response(),
        MemberError::Invalid(m) => {
            (StatusCode::UNPROCESSABLE_ENTITY, Json(json_err(&m))).into_response()
        }
        MemberError::Db(err) => db_err(err),
    }
}

fn channel_err(e: ChannelError) -> Response {
    match e {
        ChannelError::NotFound(id) => (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({"error": format!("no such channel {id}")})),
        )
            .into_response(),
        ChannelError::Invalid(m) => {
            (StatusCode::UNPROCESSABLE_ENTITY, Json(json_err(&m))).into_response()
        }
        ChannelError::Db(err) => db_err(err),
    }
}

fn query_err(e: ChannelQueryError) -> Response {
    match e {
        ChannelQueryError::NotAMember => forbidden("not a channel member"),
        ChannelQueryError::Db(err) => db_err(err),
    }
}

// ---------------------------------------------------------------------------
// Channel creation / chat dedup
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
pub struct CreateChannelBody {
    pub name: Option<String>,
    /// chat | channel | group.
    pub channel_type: String,
    pub default_access_mode: Option<String>,
    pub email_send: Option<bool>,
}

async fn create_channel(
    State(app): State<ApiState>,
    axum::Extension(identity): axum::Extension<WireIdentity>,
    Json(body): Json<CreateChannelBody>,
) -> Response {
    let Ok(id) = require_identity(&identity) else { return unauthorized() };
    match app
        .channel_write_service
        .create(CreateChannelCommand {
            name: body.name,
            channel_type: body.channel_type,
            uuid: Some(Uuid::new_v4().to_string()),
            default_access_mode: body.default_access_mode,
            email_send: body.email_send.unwrap_or(false),
            creator: Some(id),
        })
        .await
    {
        Ok(channel_id) => ok_json(serde_json::json!({ "channel_id": channel_id })),
        Err(e) => channel_err(e),
    }
}

#[derive(Deserialize)]
pub struct ChatBody {
    /// The other partner (the caller's own partner id is the first leg).
    pub partner_id: Uuid,
}

async fn open_chat(
    State(app): State<ApiState>,
    axum::Extension(identity): axum::Extension<WireIdentity>,
    Json(body): Json<ChatBody>,
) -> Response {
    let Some(me) = identity.partner_id() else { return unauthorized() };
    // A caller may only mint a chat they are a party to — never a channel
    // between two third parties.
    match app.channel_write_service.get_or_create_chat(me, body.partner_id).await {
        Ok((channel_id, created)) => {
            ok_json(serde_json::json!({ "channel_id": channel_id, "created": created }))
        }
        Err(e) => channel_err(e),
    }
}

#[derive(Deserialize)]
pub struct ChannelUpdateBody {
    pub name: Option<String>,
    pub description: Option<String>,
    pub email_send: Option<bool>,
}

async fn update_channel(
    State(app): State<ApiState>,
    axum::Extension(identity): axum::Extension<WireIdentity>,
    Path(channel_id): Path<Uuid>,
    Json(body): Json<ChannelUpdateBody>,
) -> Response {
    let Ok(id) = require_identity(&identity) else { return unauthorized() };
    // Member gate first — only members may even propose an edit.
    if let Err(e) = app.channel_query_service.members(&id, channel_id).await {
        return query_err(e);
    }
    match app
        .channel_write_service
        .update_fields(
            channel_id,
            body.name.as_deref(),
            body.description.as_deref(),
            body.email_send,
        )
        .await
    {
        Ok(()) => ok_json(serde_json::json!({ "updated": true })),
        Err(e) => channel_err(e),
    }
}

#[derive(Deserialize)]
pub struct PinMessageBody {
    pub message_id: Uuid,
    pub pinned: bool,
}

async fn pin_message(
    State(app): State<ApiState>,
    axum::Extension(identity): axum::Extension<WireIdentity>,
    Path(channel_id): Path<Uuid>,
    Json(body): Json<PinMessageBody>,
) -> Response {
    let Ok(id) = require_identity(&identity) else { return unauthorized() };
    if let Err(e) = app.channel_query_service.members(&id, channel_id).await {
        return query_err(e);
    }
    match app
        .channel_write_service
        .set_message_pinned(channel_id, body.message_id, body.pinned)
        .await
    {
        Ok(()) => ok_json(serde_json::json!({ "pinned": body.pinned })),
        Err(e) => channel_err(e),
    }
}

// ---------------------------------------------------------------------------
// Member state verbs
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
pub struct MessageIdBody {
    pub message_id: Uuid,
}

async fn mark_seen(
    State(app): State<ApiState>,
    axum::Extension(identity): axum::Extension<WireIdentity>,
    Path(channel_id): Path<Uuid>,
    Json(body): Json<MessageIdBody>,
) -> Response {
    let Ok(id) = require_identity(&identity) else { return unauthorized() };
    match app
        .channel_member_write_service
        .mark_as_read(channel_id, &id, body.message_id)
        .await
    {
        Ok(advanced) => ok_json(serde_json::json!({ "advanced": advanced })),
        Err(e) => member_err(e),
    }
}

async fn mark_fetched(
    State(app): State<ApiState>,
    axum::Extension(identity): axum::Extension<WireIdentity>,
    Path(channel_id): Path<Uuid>,
    Json(body): Json<MessageIdBody>,
) -> Response {
    let Ok(id) = require_identity(&identity) else { return unauthorized() };
    match app
        .channel_member_write_service
        .mark_fetched(channel_id, &id, body.message_id)
        .await
    {
        Ok(advanced) => ok_json(serde_json::json!({ "advanced": advanced })),
        Err(e) => member_err(e),
    }
}

async fn set_separator(
    State(app): State<ApiState>,
    axum::Extension(identity): axum::Extension<WireIdentity>,
    Path(channel_id): Path<Uuid>,
    Json(body): Json<MessageIdBody>,
) -> Response {
    let Ok(id) = require_identity(&identity) else { return unauthorized() };
    match app
        .channel_member_write_service
        .set_new_message_separator(channel_id, &id, body.message_id)
        .await
    {
        Ok(()) => ok_json(serde_json::json!({ "set": true })),
        Err(e) => member_err(e),
    }
}

async fn join_channel(
    State(app): State<ApiState>,
    axum::Extension(identity): axum::Extension<WireIdentity>,
    Path(channel_id): Path<Uuid>,
) -> Response {
    let Ok(id) = require_identity(&identity) else { return unauthorized() };
    match app.channel_member_write_service.join(channel_id, &id).await {
        Ok(joined) => ok_json(serde_json::json!({ "joined": joined })),
        Err(e) => member_err(e),
    }
}

async fn leave_channel(
    State(app): State<ApiState>,
    axum::Extension(identity): axum::Extension<WireIdentity>,
    Path(channel_id): Path<Uuid>,
) -> Response {
    let Ok(id) = require_identity(&identity) else { return unauthorized() };
    match app.channel_member_write_service.leave(channel_id, &id).await {
        Ok(left) => ok_json(serde_json::json!({ "left": left })),
        Err(e) => member_err(e),
    }
}

#[derive(Deserialize)]
pub struct MuteBody {
    pub until: Option<chrono::DateTime<chrono::Utc>>,
    pub forever: Option<bool>,
}

async fn mute_channel(
    State(app): State<ApiState>,
    axum::Extension(identity): axum::Extension<WireIdentity>,
    Path(channel_id): Path<Uuid>,
    body: Option<Json<MuteBody>>,
) -> Response {
    let Ok(id) = require_identity(&identity) else { return unauthorized() };
    // No body at all = unmute (until=None, forever=false).
    let (until, forever) = match body {
        Some(Json(b)) => (b.until, b.forever.unwrap_or(false)),
        None => (None, false),
    };
    match app.channel_member_write_service.set_mute(channel_id, &id, until, forever).await {
        Ok(()) => ok_json(serde_json::json!({ "muted": until.is_some() || forever })),
        Err(e) => member_err(e),
    }
}

#[derive(Deserialize)]
pub struct NotificationsBody {
    /// all | mentions | no_notif; absent = inherit (None).
    pub custom_notifications: Option<String>,
}

async fn set_notifications(
    State(app): State<ApiState>,
    axum::Extension(identity): axum::Extension<WireIdentity>,
    Path(channel_id): Path<Uuid>,
    body: Option<Json<NotificationsBody>>,
) -> Response {
    let Ok(id) = require_identity(&identity) else { return unauthorized() };
    let value = body.and_then(|Json(b)| b.custom_notifications);
    match app
        .channel_member_write_service
        .set_custom_notifications(channel_id, &id, value.as_deref())
        .await
    {
        Ok(()) => ok_json(serde_json::json!({ "custom_notifications": value })),
        Err(e) => member_err(e),
    }
}

#[derive(Deserialize)]
pub struct PinnedBody {
    pub pinned: bool,
}

async fn set_member_pin(
    State(app): State<ApiState>,
    axum::Extension(identity): axum::Extension<WireIdentity>,
    Path(channel_id): Path<Uuid>,
    Json(body): Json<PinnedBody>,
) -> Response {
    let Ok(id) = require_identity(&identity) else { return unauthorized() };
    match app
        .channel_member_write_service
        .set_pinned(channel_id, &id, body.pinned)
        .await
    {
        Ok(()) => ok_json(serde_json::json!({ "pinned": body.pinned })),
        Err(e) => member_err(e),
    }
}

#[derive(Deserialize)]
pub struct FoldBody {
    /// open | closed; absent = undefined.
    pub fold_state: Option<String>,
}

async fn set_fold(
    State(app): State<ApiState>,
    axum::Extension(identity): axum::Extension<WireIdentity>,
    Path(channel_id): Path<Uuid>,
    body: Option<Json<FoldBody>>,
) -> Response {
    let Ok(id) = require_identity(&identity) else { return unauthorized() };
    let fold = body.and_then(|Json(b)| b.fold_state);
    match app
        .channel_member_write_service
        .set_fold_state(channel_id, &id, fold.as_deref())
        .await
    {
        Ok(()) => ok_json(serde_json::json!({ "fold_state": fold })),
        Err(e) => member_err(e),
    }
}

#[derive(Deserialize)]
pub struct TypingBody {
    pub typing: bool,
}

async fn notify_typing(
    State(app): State<ApiState>,
    axum::Extension(identity): axum::Extension<WireIdentity>,
    Path(channel_id): Path<Uuid>,
    Json(body): Json<TypingBody>,
) -> Response {
    let Ok(id) = require_identity(&identity) else { return unauthorized() };
    match app.typing_service.notify_typing(channel_id, &id, body.typing).await {
        Ok(()) => ok_json(serde_json::json!({ "ok": true })),
        Err(TypingError::Db(err)) => db_err(err),
    }
}

// ---------------------------------------------------------------------------
// Search + reads
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
pub struct SearchBody {
    pub term: Option<String>,
}

async fn search_channels(
    State(app): State<ApiState>,
    axum::Extension(identity): axum::Extension<WireIdentity>,
    Json(body): Json<SearchBody>,
) -> Response {
    let Ok(id) = require_identity(&identity) else { return unauthorized() };
    match app.channel_query_service.search(&id, body.term.as_deref()).await {
        Ok(hits) => ok_json(serde_json::json!({ "channels": hits })),
        Err(e) => query_err(e),
    }
}

async fn channel_members(
    State(app): State<ApiState>,
    axum::Extension(identity): axum::Extension<WireIdentity>,
    Path(channel_id): Path<Uuid>,
) -> Response {
    let Ok(id) = require_identity(&identity) else { return unauthorized() };
    match app.channel_query_service.members(&id, channel_id).await {
        Ok(rows) => ok_json(serde_json::json!({ "members": rows
            .into_iter()
            .map(|(member_id, partner_id, guest_id)| serde_json::json!({
                "member_id": member_id,
                "partner_id": partner_id,
                "guest_id": guest_id,
            }))
            .collect::<Vec<_>>() })),
        Err(e) => query_err(e),
    }
}

async fn pinned_messages(
    State(app): State<ApiState>,
    axum::Extension(identity): axum::Extension<WireIdentity>,
    Path(channel_id): Path<Uuid>,
) -> Response {
    let Ok(id) = require_identity(&identity) else { return unauthorized() };
    match app.channel_query_service.pinned_messages(&id, channel_id).await {
        Ok(ids) => ok_json(serde_json::json!({ "message_ids": ids })),
        Err(e) => query_err(e),
    }
}

// ---------------------------------------------------------------------------
// Message verbs: reactions, edits, stars
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
pub struct ReactionBody {
    pub content: String,
}

async fn toggle_reaction(
    State(app): State<ApiState>,
    axum::Extension(identity): axum::Extension<WireIdentity>,
    Path(message_id): Path<Uuid>,
    Json(body): Json<ReactionBody>,
) -> Response {
    let Ok(id) = require_identity(&identity) else { return unauthorized() };
    match app.reaction_write_service.toggle(message_id, &id, &body.content).await {
        Ok(reacted) => ok_json(serde_json::json!({ "reacted": reacted })),
        Err(e) => match e {
            ReactionError::NotFound(mid) => (
                StatusCode::NOT_FOUND,
                Json(serde_json::json!({"error": format!("no such message {mid}")})),
            )
                .into_response(),
            ReactionError::Invalid(m) => {
                (StatusCode::UNPROCESSABLE_ENTITY, Json(json_err(&m))).into_response()
            }
            ReactionError::NeedsIdentity => unauthorized(),
            ReactionError::Db(err) => db_err(err),
        },
    }
}

#[derive(Deserialize)]
pub struct EditBody {
    pub subject: Option<String>,
    pub body: Option<String>,
}

async fn edit_message(
    State(app): State<ApiState>,
    axum::Extension(identity): axum::Extension<WireIdentity>,
    axum::Extension(is_admin): axum::Extension<IsAdmin>,
    Path(message_id): Path<Uuid>,
    Json(body): Json<EditBody>,
) -> Response {
    let Ok(id) = require_identity(&identity) else { return unauthorized() };
    match app
        .message_edit_service
        .update_content(
            message_id,
            &id,
            is_admin.0,
            body.subject.as_deref(),
            body.body.as_deref(),
        )
        .await
    {
        Ok(changed) => ok_json(serde_json::json!({ "changed": changed })),
        Err(e) => match e {
            MessageEditError::NotFound(mid) => (
                StatusCode::NOT_FOUND,
                Json(serde_json::json!({"error": format!("no such message {mid}")})),
            )
                .into_response(),
            MessageEditError::Forbidden(_) => {
                forbidden("only the author (or an admin route) may edit")
            }
            MessageEditError::NeedsPartner => unauthorized(),
            MessageEditError::Invalid(m) => {
                (StatusCode::UNPROCESSABLE_ENTITY, Json(json_err(&m))).into_response()
            }
            MessageEditError::Db(err) => db_err(err),
        },
    }
}

async fn toggle_star(
    State(app): State<ApiState>,
    axum::Extension(identity): axum::Extension<WireIdentity>,
    Path(message_id): Path<Uuid>,
) -> Response {
    let Ok(id) = require_identity(&identity) else { return unauthorized() };
    match app.message_edit_service.toggle_star(message_id, &id).await {
        Ok(starred) => ok_json(serde_json::json!({ "starred": starred })),
        Err(e) => match e {
            MessageEditError::NotFound(mid) => (
                StatusCode::NOT_FOUND,
                Json(serde_json::json!({"error": format!("no such message {mid}")})),
            )
                .into_response(),
            MessageEditError::NeedsPartner => unauthorized(),
            MessageEditError::Invalid(m) => {
                (StatusCode::UNPROCESSABLE_ENTITY, Json(json_err(&m))).into_response()
            }
            MessageEditError::Forbidden(_) => forbidden("only the author may edit"),
            MessageEditError::Db(err) => db_err(err),
        },
    }
}

// ---------------------------------------------------------------------------
// Composer
// ---------------------------------------------------------------------------

/// The discuss route group (guarded surface — mount behind app user auth +
/// the guest middleware). Search is throttled 30/60s.
pub fn composer() -> Router<ApiState> {
    use axum::middleware as axum_mw;

    let throttled = Router::<ApiState>::new()
        .route("/discuss/search", post(search_channels))
        .route_layer(axum_mw::from_fn_with_state(
            backbone_rate_limit::middleware(30, 60),
            backbone_rate_limit::rate_limit_middleware,
        ));

    Router::new()
        .route("/discuss/channel", post(create_channel))
        .route("/discuss/chat", post(open_chat))
        .route("/discuss/channel/:id", post(update_channel))
        .route("/discuss/channel/:id/join", post(join_channel))
        .route("/discuss/channel/:id/leave", post(leave_channel))
        .route("/discuss/channel/:id/mute", post(mute_channel))
        .route("/discuss/channel/:id/notifications", post(set_notifications))
        .route("/discuss/channel/:id/pin", post(set_member_pin))
        .route("/discuss/channel/:id/fold", post(set_fold))
        .route("/discuss/channel/:id/seen", post(mark_seen))
        .route("/discuss/channel/:id/fetched", post(mark_fetched))
        .route("/discuss/channel/:id/separator", post(set_separator))
        .route("/discuss/channel/:id/typing", post(notify_typing))
        .route("/discuss/channel/:id/pin_message", post(pin_message))
        .route("/discuss/channel/:id/members", get(channel_members))
        .route("/discuss/channel/:id/pinned", get(pinned_messages))
        .route("/discuss/message/:id/reaction", post(toggle_reaction))
        .route("/discuss/message/:id/edit", post(edit_message))
        .route("/discuss/message/:id/star", post(toggle_star))
        .merge(throttled)
}
