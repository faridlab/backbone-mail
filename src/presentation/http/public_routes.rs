//! Public bootstrap routes (hand-written; user-owned).
//!
//! The unauthenticated entry to a public discuss channel (Odoo's public
//! `/discuss` invitation pages, re-expressed as JSON — no HTML here). The
//! caller arrives anonymous or with a guest cookie; the bootstrap resolves
//! the invitation uuid to a PUBLIC channel (private/nonexistent are
//! indistinguishable — no existence oracle), mints a guest persona for
//! anonymous callers, and returns the `dgid` cookie alongside the channel
//! snapshot. Hard-throttled 10/60s: this is the one truly public surface.

use axum::body::Body;
use axum::extract::State;
use axum::http::{header, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::post;
use axum::{Json, Router};
use serde::Deserialize;

use crate::infrastructure::persistence::channel_repository::ChannelRepository;
use crate::presentation::http::thread_routes::{db_err, json_err, ApiState};
use crate::presentation::middleware::WireIdentity;

#[derive(Deserialize)]
pub struct BootstrapBody {
    /// The invitation uuid from the channel's public link.
    pub channel_uuid: String,
    /// chat | meet — display hint only (Odoo's mode param).
    pub mode: Option<String>,
}

async fn bootstrap(
    State(app): State<ApiState>,
    axum::Extension(identity): axum::Extension<WireIdentity>,
    Json(body): Json<BootstrapBody>,
) -> Response {
    if identity.identity().is_some() && !identity.is_guest() {
        // Signed-in partners use the guarded surface, not the bootstrap.
        return (
            StatusCode::UNAUTHORIZED,
            Json(json_err("authentication required")),
        )
            .into_response();
    }

    // Public lookup only — private/deleted uuids are "not found", identical.
    let channel = match ChannelRepository::find_public_by_uuid(&app.db_pool(), &body.channel_uuid).await {
        Ok(Some(hit)) => hit,
        Ok(None) => {
            return (StatusCode::NOT_FOUND, Json(json_err("no such channel"))).into_response()
        }
        Err(err) => return db_err(err),
    };

    // Anonymous callers get a guest persona; the cookie IS the credential.
    let (guest_id, cookie) = match identity.identity() {
        Some(crate::application::service::chatter_acl::MessagingIdentity::Guest { guest_id }) => {
            (*guest_id, None)
        }
        _ => match app.guest_write_service.mint(None).await {
            Ok(gid) => (
                gid,
                Some(format!("dgid={gid}; Path=/; HttpOnly; SameSite=Lax; Max-Age=2592000")),
            ),
            Err(_) => {
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(json_err("internal error")),
                )
                    .into_response()
            }
        },
    };

    let payload = serde_json::json!({
        "channel_id": channel.0,
        "channel_name": channel.1,
        "channel_type": channel.2,
        "mode": body.mode,
        "guest_id": guest_id,
    });
    let body = serde_json::to_string(&payload).unwrap_or_default();
    let mut headers = vec![(header::CONTENT_TYPE, axum::http::HeaderValue::from_static("application/json"))];
    if let Some(cookie) = cookie {
        if let Ok(v) = axum::http::HeaderValue::from_str(&cookie) {
            headers.push((header::SET_COOKIE, v));
        }
    }
    (StatusCode::OK, axum::response::AppendHeaders(headers), Body::from(body)).into_response()
}

/// The public bootstrap group — mount behind `guest_context` ONLY (no user
/// auth), throttled hard (10/60s).
pub fn composer() -> Router<ApiState> {
    use axum::middleware as axum_mw;

    Router::<ApiState>::new()
        .route("/discuss/public/bootstrap", post(bootstrap))
        .route_layer(axum_mw::from_fn_with_state(
            backbone_rate_limit::middleware(10, 60),
            backbone_rate_limit::rate_limit_middleware,
        ))
}
