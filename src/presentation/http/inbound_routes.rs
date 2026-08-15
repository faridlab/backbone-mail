//! Inbound email webhook route (hand-written; user-owned) — MAIL-M27's
//! webhook mode (owner-locked decision, port-notes §8).
//!
//! `POST /mail/inbound/:server_id` — the upstream relay (SES/SendGrid-style
//! inbound parse) POSTs one parsed message per call with a per-server bearer
//! token: `Authorization: Bearer <token>`. The token IS the auth — this group
//! mounts BARE (no user/guest middleware), mirroring the proven `/sms/status`
//! ADR-0021 posture, but with a per-row SHA-256 hash instead of a shared HMAC
//! (per-row tokens don't fit env refs; documented ADR-0024 deviation,
//! port-notes §8).
//!
//! Verdict shapes:
//!   200 `{outcome: routed|duplicate|rejected|dropped}` — every pipeline
//!       verdict is a SUCCESS at the HTTP layer (the relay retried, was
//!       rejected by policy, or hit an unroutable message — all expected).
//!   400 unparseable JSON (the relay sent garbage — fixable only relay-side).
//!   401 missing/bad token, unknown server id, inactive server — ALL
//!       IDENTICAL by design: no server-existence oracle, no token oracle.
//!   500 nothing else should leak.

use axum::body::Bytes;
use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::post;
use axum::{Json, Router};
use uuid::Uuid;

use crate::application::service::{
    InboundMessage, InboundOutcome, MailInboundError, MailInboundService,
};
use crate::presentation::http::thread_routes::{db_err, json_err, ApiState};

/// Pull the bearer token out of `Authorization: Bearer <token>` (absent or a
/// non-Bearer scheme → None → 401; never an error before auth).
fn bearer_token(headers: &HeaderMap) -> Option<String> {
    let value = headers.get(axum::http::header::AUTHORIZATION)?.to_str().ok()?;
    let token = value.strip_prefix("Bearer ").or_else(|| value.strip_prefix("bearer "))?;
    if token.is_empty() {
        return None;
    }
    Some(token.to_string())
}

async fn mail_inbound(
    State(app): State<ApiState>,
    Path(server_id): Path<Uuid>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    // Unparseable JSON is the ONE pre-auth error — nothing has been written and
    // the relay needs a non-401 signal to stop retrying a broken template.
    let msg: InboundMessage = match serde_json::from_slice(&body) {
        Ok(m) => m,
        Err(e) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(json_err(&format!("unparseable body: {e}"))),
            )
                .into_response()
        }
    };
    let Some(token) = bearer_token(&headers) else {
        return (StatusCode::UNAUTHORIZED, Json(json_err("unauthorized"))).into_response();
    };

    let service = MailInboundService::new(app.db_pool());
    match service.process_inbound(server_id, &token, &msg).await {
        Ok(outcome) => {
            let label = match &outcome {
                InboundOutcome::Routed { .. } => "routed",
                InboundOutcome::Duplicate { .. } => "duplicate",
                InboundOutcome::Rejected => "rejected",
                InboundOutcome::Dropped => "dropped",
            };
            (StatusCode::OK, Json(serde_json::json!({ "outcome": label }))).into_response()
        }
        Err(e) => match e {
            // Auth and unknown-server are IDENTICAL — the no-oracle rule.
            MailInboundError::Auth => {
                (StatusCode::UNAUTHORIZED, Json(json_err("unauthorized"))).into_response()
            }
            MailInboundError::Invalid(m) => {
                (StatusCode::UNPROCESSABLE_ENTITY, Json(json_err(&m))).into_response()
            }
            MailInboundError::Db(err) => db_err(err),
        },
    }
}

/// The inbound webhook route group — BARE mount (the token IS the auth),
/// throttled 120/60s so a relay retry storm cannot wedge the pipeline.
pub fn composer() -> Router<ApiState> {
    use axum::middleware as axum_mw;

    Router::<ApiState>::new()
        .route("/mail/inbound/:server_id", post(mail_inbound))
        .route_layer(axum_mw::from_fn_with_state(
            backbone_rate_limit::middleware(120, 60),
            backbone_rate_limit::rate_limit_middleware,
        ))
}
