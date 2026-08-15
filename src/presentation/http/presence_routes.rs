//! Presence routes (hand-written; user-owned).
//!
//! Two verbs: the manual im_status override, and the bus liveness ping. The
//! liveness ping is the port of Odoo's `is_websocket_session` gate — it is
//! accepted ONLY with a valid SSE session proof in `X-Messaging-Session`
//! (minted at stream connect; ADR-0018 Tier-A shape). Presence must reflect
//! REAL connections, so a proof-less ping is a 403, not a silent no-op.

use axum::extract::State;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::post;
use axum::{Json, Router};
use serde::Deserialize;

use crate::application::service::PresenceError;
use crate::presentation::http::thread_routes::{db_err, json_err, ok_json, require_identity, ApiState};
use crate::presentation::middleware::{unauthorized, WireIdentity};
use crate::realtime::session::verify_session_token;
use crate::realtime::SessionSecret;

fn presence_err(e: PresenceError) -> Response {
    match e {
        PresenceError::ProofRequired => (
            StatusCode::FORBIDDEN,
            Json(json_err("presence update requires a live SSE session proof")),
        )
            .into_response(),
        PresenceError::Invalid(m) => {
            (StatusCode::UNPROCESSABLE_ENTITY, Json(json_err(&m))).into_response()
        }
        PresenceError::Db(err) => db_err(err),
    }
}

#[derive(Deserialize)]
pub struct StatusBody {
    /// online | away | offline.
    pub status: String,
}

async fn set_status(
    State(app): State<ApiState>,
    axum::Extension(identity): axum::Extension<WireIdentity>,
    Json(body): Json<StatusBody>,
) -> Response {
    let Ok(id) = require_identity(&identity) else { return unauthorized() };
    match app.presence_write_service.set_manual_im_status(&id, &body.status).await {
        Ok(()) => ok_json(serde_json::json!({ "status": body.status })),
        Err(e) => presence_err(e),
    }
}

async fn bus_presence(
    State(app): State<ApiState>,
    axum::Extension(identity): axum::Extension<WireIdentity>,
    axum::Extension(secret): axum::Extension<SessionSecret>,
    headers: axum::http::HeaderMap,
) -> Response {
    let Ok(id) = require_identity(&identity) else { return unauthorized() };
    let proof = headers
        .get("x-messaging-session")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    let valid = verify_session_token(&secret.0, proof, &id);
    match app.presence_write_service.update_bus_presence(&id, valid).await {
        Ok(()) => ok_json(serde_json::json!({ "ok": true })),
        Err(e) => presence_err(e),
    }
}

/// The presence route group. The host app MUST layer
/// `Extension(SessionSecret(...))` onto this group (no default secret —
/// absence is a startup bug, not a fallback).
pub fn composer() -> Router<ApiState> {
    Router::new()
        .route("/mail/presence/status", post(set_status))
        .route("/mail/presence/bus", post(bus_presence))
}
