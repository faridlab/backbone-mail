//! The user-scope verifier (ADR-0014 posture 4: company_fence none ⇒ USER
//! scope, not company_auth — messaging has no company dimension to fence).
//!
//! Thin by design (the plan's "add a thin one in the app if backbone-auth
//! lacks it" — it does): Bearer JWT in, `AuthPartnerId` + `IsAdmin`
//! extensions out. The downstream `guest_context` middleware turns those
//! extensions into the wire identity.
//!
//! The token contract: `sub` carries the caller's PARTNER id (Odoo's
//! res.users → res.partner convergence — the messaging identity IS the
//! partner). A token whose `sub` is not a uuid is a 401, not a silent
//! downgrade. Requests WITHOUT an Authorization header pass through
//! untouched — the guest/anonymous path is a first-class caller here, and
//! each guarded route applies its own 401.
//!
//! `IsAdmin` comes from the composition's `admin_partners` allowlist (the JWT
//! carries no roles) — the author-or-admin gates on message edits and
//! attachment ownership.
//!
//! Promoted from backbone-messaging-app so every composing service binds the
//! same partner-scope contract.

use std::sync::Arc;

use axum::extract::{Request, State};
use axum::http::{header, StatusCode};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use backbone_auth::jwt::JwtService;
use uuid::Uuid;

use super::{AuthPartnerId, IsAdmin};

#[derive(Clone)]
pub struct UserScope {
    jwt: Arc<JwtService>,
    admin_partners: Arc<Vec<Uuid>>,
}

impl UserScope {
    pub fn new(jwt_secret: &str, admin_partners: Vec<Uuid>) -> Self {
        Self {
            jwt: Arc::new(JwtService::new(jwt_secret)),
            admin_partners: Arc::new(admin_partners),
        }
    }

    fn is_admin(&self, partner: Uuid) -> bool {
        self.admin_partners.contains(&partner)
    }
}

/// The middleware fn: Bearer token → partner identity extensions. Absent
/// header = pass-through (guest/anonymous); present-but-invalid = 401.
pub async fn user_scope(State(scope): State<UserScope>, mut req: Request, next: Next) -> Response {
    let bearer = req
        .headers()
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "));

    let Some(token) = bearer else {
        return next.run(req).await;
    };

    let claims = match scope.jwt.validate_token(token) {
        Ok(c) => c,
        Err(e) => {
            tracing::warn!(target: "auth::user_scope", error = %e, "invalid bearer token");
            return invalid_token();
        }
    };
    let partner: Uuid = match claims.sub.parse() {
        Ok(id) => id,
        Err(_) => {
            tracing::warn!(target: "auth::user_scope", "token sub is not a partner uuid");
            return invalid_token();
        }
    };
    req.extensions_mut().insert(AuthPartnerId(partner));
    req.extensions_mut().insert(IsAdmin(scope.is_admin(partner)));
    next.run(req).await
}

fn invalid_token() -> Response {
    (
        StatusCode::UNAUTHORIZED,
        "invalid or expired bearer token",
    )
        .into_response()
}
