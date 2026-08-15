//! The guest/identity middleware (hand-written; user-owned).
//!
//! Port of Odoo's discuss public-boundary resolution: one request is either a
//! known partner (user), a guest (the `dgid` cookie — the cookie IS the
//! guest's bearer credential, exactly like Odoo's session-scoped guest id),
//! or anonymous. The resolved identity rides the request as the
//! [`WireIdentity`] extension; every guarded route reads it and applies its
//! own procedural gate (token consteq / authorship / membership /
//! thread-access — MAIL-B1). The middleware itself authorizes NOTHING.
//!
//! Partner identity comes from an upstream app-auth layer (the user-scope
//! verifier, ADR-0014 posture 4) inserting [`AuthPartnerId`] into the
//! extensions — the module never parses user tokens itself.

use axum::extract::Request;
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use uuid::Uuid;

use crate::application::service::chatter_acl::MessagingIdentity;

/// Partner identity injected by the app's user-scope auth layer.
#[derive(Debug, Clone, Copy)]
pub struct AuthPartnerId(pub Uuid);

/// Admin flag injected by the app's auth layer (drives the author-or-admin
/// gates on message edits and attachment ownership).
#[derive(Debug, Clone, Copy, Default)]
pub struct IsAdmin(pub bool);

/// The resolved caller identity. `Anonymous` means NO identity at all —
/// routes decide whether that is a 401 (guarded surface) or grounds to mint
/// a fresh guest (public bootstrap).
#[derive(Debug, Clone)]
pub enum WireIdentity {
    Identified(MessagingIdentity),
    Anonymous,
}

impl WireIdentity {
    /// The inner identity, or None when anonymous.
    pub fn identity(&self) -> Option<&MessagingIdentity> {
        match self {
            WireIdentity::Identified(id) => Some(id),
            WireIdentity::Anonymous => None,
        }
    }

    /// The partner id when this is a signed-in partner, else None.
    pub fn partner_id(&self) -> Option<Uuid> {
        self.identity().and_then(|i| i.partner_id())
    }

    /// True when a guest cookie identified the caller.
    pub fn is_guest(&self) -> bool {
        matches!(self.identity(), Some(MessagingIdentity::Guest { .. }))
    }
}

/// The axum middleware fn: resolve identity, insert the extension, continue.
/// Resolution order: app-auth partner extension → `dgid` cookie guest →
/// anonymous. A malformed dgid value degrades to anonymous (never a 500 —
/// hostile cookies are noise, not events).
pub async fn guest_context(mut req: Request, next: Next) -> Response {
    // The admin flag defaults to false when the app auth layer didn't set
    // one — routes extracting `Extension<IsAdmin>` must never 500 just
    // because a guest/anonymous caller reached them without app auth.
    if req.extensions().get::<IsAdmin>().is_none() {
        req.extensions_mut().insert(IsAdmin(false));
    }
    if let Some(AuthPartnerId(partner_id)) = req.extensions().get::<AuthPartnerId>().copied() {
        req.extensions_mut().insert(WireIdentity::Identified(MessagingIdentity::User { partner_id }));
        return next.run(req).await;
    }
    let dgid = req
        .headers()
        .get(axum::http::header::COOKIE)
        .and_then(|v| v.to_str().ok())
        .and_then(|cookies| {
            cookies.split(';').map(|c| c.trim()).find_map(|c| {
                c.strip_prefix("dgid=").filter(|v| !v.is_empty())
            })
        });
    let identity = dgid
        .and_then(|v| Uuid::parse_str(v).ok())
        .map(|guest_id| MessagingIdentity::Guest { guest_id });
    let wire = match identity {
        Some(id) => WireIdentity::Identified(id),
        None => WireIdentity::Anonymous,
    };
    req.extensions_mut().insert(wire);
    next.run(req).await
}

/// 401 helper — the response every partner-gated route returns for
/// anonymous/guest callers (plain JSON, no detail leak).
pub fn unauthorized() -> Response {
    (
        axum::http::StatusCode::UNAUTHORIZED,
        axum::Json(serde_json::json!({"error": "authentication required"})),
    )
        .into_response()
}

/// 403 helper for procedural denials.
pub fn forbidden(reason: &str) -> Response {
    (
        axum::http::StatusCode::FORBIDDEN,
        axum::Json(serde_json::json!({"error": reason})),
    )
        .into_response()
}
