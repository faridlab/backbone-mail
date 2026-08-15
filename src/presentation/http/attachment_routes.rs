//! Attachment routes (hand-written; user-owned).
//!
//! The slim MailAttachment wire surface: register (upload metadata + optional
//! inline datas), attach/detach to a message, and the public access-token
//! mint/clear (the consteq-gated sharing link credential). All verbs are
//! owner-gated in the service; the whole group is throttled 30/60s.

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::post;
use axum::{Json, Router};
use serde::Deserialize;
use uuid::Uuid;

use crate::application::service::AttachmentError;
use crate::presentation::http::thread_routes::{db_err, json_err, ok_json, require_identity, ApiState};
use crate::presentation::middleware::{unauthorized, IsAdmin, WireIdentity};

fn attachment_err(e: AttachmentError) -> Response {
    match e {
        AttachmentError::NotFound(id) => (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({"error": format!("no such attachment {id}")})),
        )
            .into_response(),
        AttachmentError::Forbidden(_) => {
            (StatusCode::FORBIDDEN, Json(json_err("attachment is not yours"))).into_response()
        }
        AttachmentError::NeedsOwner => unauthorized(),
        AttachmentError::Invalid(m) => {
            (StatusCode::UNPROCESSABLE_ENTITY, Json(json_err(&m))).into_response()
        }
        AttachmentError::Db(err) => db_err(err),
    }
}

#[derive(Deserialize)]
pub struct RegisterBody {
    pub name: String,
    pub mimetype: Option<String>,
    pub size: Option<i32>,
    /// Base64 content (the slim port stores inline; the object-storage
    /// handle arrives as `checksum`-keyed rows, not multipart, for now).
    pub datas: Option<String>,
    pub checksum: Option<String>,
}

async fn register(
    State(app): State<ApiState>,
    axum::Extension(identity): axum::Extension<WireIdentity>,
    Json(body): Json<RegisterBody>,
) -> Response {
    let Ok(id) = require_identity(&identity) else { return unauthorized() };
    match app
        .attachment_write_service
        .register(
            &id,
            &body.name,
            body.mimetype.as_deref(),
            body.size,
            body.datas.as_deref(),
            body.checksum.as_deref(),
        )
        .await
    {
        Ok(attachment_id) => ok_json(serde_json::json!({ "attachment_id": attachment_id })),
        Err(e) => attachment_err(e),
    }
}

#[derive(Deserialize)]
pub struct MessageRefBody {
    pub message_id: Uuid,
}

async fn attach(
    State(app): State<ApiState>,
    axum::Extension(identity): axum::Extension<WireIdentity>,
    axum::Extension(is_admin): axum::Extension<IsAdmin>,
    Path(attachment_id): Path<Uuid>,
    Json(body): Json<MessageRefBody>,
) -> Response {
    let Ok(id) = require_identity(&identity) else { return unauthorized() };
    match app
        .attachment_write_service
        .attach_to_message(&id, is_admin.0, body.message_id, attachment_id)
        .await
    {
        Ok(attached) => ok_json(serde_json::json!({ "attached": attached })),
        Err(e) => attachment_err(e),
    }
}

async fn detach(
    State(app): State<ApiState>,
    axum::Extension(identity): axum::Extension<WireIdentity>,
    axum::Extension(is_admin): axum::Extension<IsAdmin>,
    Path(attachment_id): Path<Uuid>,
    Json(body): Json<MessageRefBody>,
) -> Response {
    let Ok(id) = require_identity(&identity) else { return unauthorized() };
    match app
        .attachment_write_service
        .detach_from_message(&id, is_admin.0, body.message_id, attachment_id)
        .await
    {
        Ok(detached) => ok_json(serde_json::json!({ "detached": detached })),
        Err(e) => attachment_err(e),
    }
}

async fn mint_token(
    State(app): State<ApiState>,
    axum::Extension(identity): axum::Extension<WireIdentity>,
    axum::Extension(is_admin): axum::Extension<IsAdmin>,
    Path(attachment_id): Path<Uuid>,
) -> Response {
    let Ok(id) = require_identity(&identity) else { return unauthorized() };
    match app
        .attachment_write_service
        .mint_access_token(&id, is_admin.0, attachment_id)
        .await
    {
        Ok(token) => ok_json(serde_json::json!({ "access_token": token })),
        Err(e) => attachment_err(e),
    }
}

async fn clear_token(
    State(app): State<ApiState>,
    axum::Extension(identity): axum::Extension<WireIdentity>,
    axum::Extension(is_admin): axum::Extension<IsAdmin>,
    Path(attachment_id): Path<Uuid>,
) -> Response {
    let Ok(id) = require_identity(&identity) else { return unauthorized() };
    match app
        .attachment_write_service
        .clear_access_token(&id, is_admin.0, attachment_id)
        .await
    {
        Ok(()) => ok_json(serde_json::json!({ "cleared": true })),
        Err(e) => attachment_err(e),
    }
}

/// The attachment route group (owner-gated, throttled 30/60s as a class —
/// uploads and token mints are the expensive, enumeration-shaped verbs).
pub fn composer() -> Router<ApiState> {
    use axum::middleware as axum_mw;

    Router::<ApiState>::new()
        .route("/mail/attachment", post(register))
        .route("/mail/attachment/:id/attach", post(attach))
        .route("/mail/attachment/:id/detach", post(detach))
        .route("/mail/attachment/:id/token", post(mint_token))
        .route("/mail/attachment/:id/token/clear", post(clear_token))
        .route_layer(axum_mw::from_fn_with_state(
            backbone_rate_limit::middleware(30, 60),
            backbone_rate_limit::rate_limit_middleware,
        ))
}
