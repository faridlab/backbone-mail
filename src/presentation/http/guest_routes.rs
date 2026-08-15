//! Guest routes (hand-written; user-owned).
//!
//! The public guest lifecycle: mint a guest identity (returns the `dgid`
//! cookie that IS the guest's bearer credential) and self-rename. Odoo's
//! guest has no password, no email, no reset — the cookie is everything,
//! which is exactly why losing it means losing the persona (guest GC then
//! reaps the row).

use axum::body::Body;
use axum::extract::State;
use axum::http::{header, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::post;
use axum::{Json, Router};
use serde::Deserialize;

use crate::application::service::GuestError;
use crate::presentation::http::thread_routes::{db_err, json_err, ok_json, ApiState};
use crate::presentation::middleware::{unauthorized, WireIdentity};

fn guest_cookie(guest_id: uuid::Uuid) -> String {
    // HttpOnly: the credential is server-issued and never read by page JS.
    // SameSite=Lax: rides same-site navigations, not cross-site posts.
    format!("dgid={guest_id}; Path=/; HttpOnly; SameSite=Lax; Max-Age=2592000")
}

#[derive(Deserialize)]
pub struct MintGuestBody {
    pub name: Option<String>,
}

async fn mint_guest(
    State(app): State<ApiState>,
    axum::Extension(identity): axum::Extension<WireIdentity>,
    body: Option<Json<MintGuestBody>>,
) -> Response {
    // An already-identified caller never needs a second persona.
    if identity.identity().is_some() {
        return unauthorized();
    }
    let name = body.and_then(|Json(b)| b.name);
    match app.guest_write_service.mint(name.as_deref()).await {
        Ok(guest_id) => {
            let mut res = Response::builder()
                .status(StatusCode::OK)
                .header(header::SET_COOKIE, guest_cookie(guest_id))
                .body(Body::from(
                    serde_json::to_string(&serde_json::json!({ "guest_id": guest_id }))
                        .unwrap_or_default(),
                ))
                .expect("static response parts");
            res.headers_mut()
                .insert(header::CONTENT_TYPE, axum::http::HeaderValue::from_static("application/json"));
            res
        }
        Err(e) => match e {
            crate::application::service::GuestError::Invalid(m) => {
                (StatusCode::UNPROCESSABLE_ENTITY, Json(json_err(&m))).into_response()
            }
            crate::application::service::GuestError::Db(err) => db_err(err),
            _ => (StatusCode::INTERNAL_SERVER_ERROR, Json(json_err("internal error"))).into_response(),
        },
    }
}

#[derive(Deserialize)]
pub struct UpdateNameBody {
    pub name: String,
}

async fn update_name(
    State(app): State<ApiState>,
    axum::Extension(identity): axum::Extension<WireIdentity>,
    Json(body): Json<UpdateNameBody>,
) -> Response {
    let Some(caller) = identity.identity() else { return unauthorized() };
    if !identity.is_guest() {
        // A signed-in partner is not a guest — no cross-persona renames here.
        return unauthorized();
    }
    match app.guest_write_service.update_name(caller.guest_id().expect("checked above"), &body.name, caller).await {
        Ok(()) => ok_json(serde_json::json!({ "renamed": true })),
        Err(e) => match e {
            GuestError::NotFound(id) => (
                StatusCode::NOT_FOUND,
                Json(serde_json::json!({"error": format!("no such guest {id}")})),
            )
                .into_response(),
            GuestError::Forbidden => unauthorized(),
            GuestError::Invalid(m) => {
                (StatusCode::UNPROCESSABLE_ENTITY, Json(json_err(&m))).into_response()
            }
            GuestError::Db(err) => db_err(err),
        },
    }
}

/// The guest route group — public surface behind `guest_context` only.
pub fn composer() -> Router<ApiState> {
    Router::new()
        .route("/mail/guest", post(mint_guest))
        .route("/mail/guest/update_name", post(update_name))
}
