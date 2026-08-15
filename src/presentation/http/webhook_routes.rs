//! Sms provider webhook route (hand-written; user-owned).
//!
//! `POST /sms/status` — the provider callback. Scheme: `hmac_raw_body`
//! (ADR-0021): HMAC-SHA256 over the EXACT raw request bytes, hex-encoded in
//! `X-Signature`. The HMAC IS the auth — this group mounts BARE (no user/guest
//! middleware). The service fails closed: nothing is written before
//! verification passes, and a replay is an idempotent no-op.
//!
//! The secret is `SMS_WEBHOOK_SECRET` (an ENV-VAR REFERENCE — ADR-0024
//! interim; the module config declares the reference, the process env
//! supplies the value). When the variable is unset the route answers 503:
//! fail closed, because an empty-string key would be a forgeable MAC.

use axum::body::Bytes;
use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::post;
use axum::{Json, Router};

use crate::application::service::{SmsStatusWebhookService, WebhookError, WebhookOutcome};
use crate::presentation::http::thread_routes::{db_err, json_err, ApiState};

/// The env-var name (greppable — the config reference and the reader must
/// agree; see ADR-0024).
pub const SMS_WEBHOOK_SECRET_ENV: &str = "SMS_WEBHOOK_SECRET";

async fn sms_status(
    State(app): State<ApiState>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    let Some(secret) = std::env::var(SMS_WEBHOOK_SECRET_ENV).ok().filter(|s| !s.is_empty())
    else {
        tracing::error!(
            env = SMS_WEBHOOK_SECRET_ENV,
            "sms webhook secret not configured — failing closed"
        );
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(json_err("webhook not configured")),
        )
            .into_response();
    };
    let signature = headers.get("x-signature").and_then(|v| v.to_str().ok());
    let service = SmsStatusWebhookService::new(app.db_pool(), &secret);
    match service.handle(&body, signature).await {
        Ok(WebhookOutcome::Advanced) => {
            (StatusCode::OK, Json(serde_json::json!({ "outcome": "advanced" }))).into_response()
        }
        Ok(WebhookOutcome::Replay) => {
            (StatusCode::OK, Json(serde_json::json!({ "outcome": "replay" }))).into_response()
        }
        Err(e) => match e {
            WebhookError::Verify(_) => {
                // Bad signature / stale timestamp: 401, and NEVER a hint about
                // which leg failed.
                (StatusCode::UNAUTHORIZED, Json(json_err("verification failed"))).into_response()
            }
            WebhookError::Invalid(m) => {
                (StatusCode::UNPROCESSABLE_ENTITY, Json(json_err(&m))).into_response()
            }
            WebhookError::Db(err) => db_err(err),
            WebhookError::Sms(_) => {
                (StatusCode::INTERNAL_SERVER_ERROR, Json(json_err("internal error"))).into_response()
            }
        },
    }
}

/// The webhook route group — BARE mount (the HMAC is the auth), throttled
/// 120/60s so a provider retry storm cannot wedge the queue.
pub fn composer() -> Router<ApiState> {
    use axum::middleware as axum_mw;

    Router::<ApiState>::new()
        .route("/sms/status", post(sms_status))
        .route_layer(axum_mw::from_fn_with_state(
            backbone_rate_limit::middleware(120, 60),
            backbone_rate_limit::rate_limit_middleware,
        ))
}
