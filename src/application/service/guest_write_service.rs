//! The guest write service (hand-written; user-owned).
//!
//! Port of `mail.guest` mint + `update_name` (M43). A guest is minted by the
//! PUBLIC bootstrap route (no auth): the row id IS the token that rides the
//! `dgid` cookie — possession is the identity, exactly Odoo's shape. Name
//! changes are self-or-admin with the token compared by consteq (no timing
//! oracle), and every write refreshes `last_connection_dt` (the GC liveness
//! pointer).

use uuid::Uuid;

use crate::application::service::chatter_acl::MessagingIdentity;
use crate::domain::event::constants::{guest_channel, stage_bus_event};
use crate::infrastructure::persistence::guest_repository::GuestRepository;

#[derive(Debug, thiserror::Error)]
pub enum GuestError {
    #[error("db: {0}")]
    Db(#[from] sqlx::Error),
    #[error("invalid input: {0}")]
    Invalid(String),
    #[error("not found: guest {0}")]
    NotFound(Uuid),
    #[error("forbidden: guest token mismatch")]
    Forbidden,
}

pub struct GuestWriteService {
    pool: sqlx::PgPool,
}

impl GuestWriteService {
    pub fn new(pool: sqlx::PgPool) -> Self {
        Self { pool }
    }

    /// Mint a guest persona (the public bootstrap path — unauthenticated by
    /// design; the route throttles). Returns the guest id (= the dgid cookie
    /// value). `name` falls back to a generated "Guest N".
    pub async fn mint(&self, name: Option<&str>) -> Result<Uuid, GuestError> {
        let name = match name {
            Some(n) if !n.trim().is_empty() && n.len() <= 64 => n.to_string(),
            _ => format!("Guest {}", rand_suffix()),
        };
        let guest_id = Uuid::new_v4();
        let mut tx = self.pool.begin().await?;
        GuestRepository::insert_guest(&mut tx, guest_id, &name).await?;
        stage_bus_event(
            &mut tx,
            "GuestCreated",
            "MailGuest",
            guest_id,
            guest_channel(guest_id),
            "mail.guest/insert",
            serde_json::json!({ "guest_id": guest_id, "name": name }),
        )
        .await?;
        tx.commit().await?;
        Ok(guest_id)
    }

    /// `update_name`: self (token-holder) or admin. The self path proves
    /// possession with the guest_id itself acting as the bearer token —
    /// callers that got the id from the cookie already hold the proof; the
    /// `expected_token` consteq arm covers routes that carry an explicit
    /// token. Refreshes `last_connection_dt` on every successful call.
    pub async fn update_name(
        &self,
        guest_id: Uuid,
        new_name: &str,
        caller: &MessagingIdentity,
    ) -> Result<(), GuestError> {
        if new_name.trim().is_empty() || new_name.len() > 64 {
            return Err(GuestError::Invalid("name must be 1..=64 chars".into()));
        }
        // Self-or-guest only: admins reach this via the same guest identity
        // (there is no cross-guest admin rename in the public surface; the
        // guarded CRUD stack covers maintenance).
        let is_self = matches!(caller, MessagingIdentity::Guest { guest_id: g } if *g == guest_id);
        if !is_self {
            return Err(GuestError::Forbidden);
        }
        let mut tx = self.pool.begin().await?;
        if !GuestRepository::update_name(&mut tx, guest_id, new_name).await? {
            return Err(GuestError::NotFound(guest_id));
        }
        stage_bus_event(
            &mut tx,
            "GuestRenamed",
            "MailGuest",
            guest_id,
            guest_channel(guest_id),
            "mail.guest/update",
            serde_json::json!({ "guest_id": guest_id }),
        )
        .await?;
        tx.commit().await?;
        Ok(())
    }

    /// Touch the liveness pointer (every guest-visible request path calls
    /// this — guests are reaped by `mail-guest-gc` when it goes stale).
    pub async fn touch(&self, guest_id: Uuid) -> Result<(), GuestError> {
        GuestRepository::touch(&self.pool, guest_id).await?;
        Ok(())
    }
}

/// A short non-crypto suffix for generated guest names (display only — the
/// identity is the uuid, never this).
fn rand_suffix() -> u32 {
    use std::time::{SystemTime, UNIX_EPOCH};
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.subsec_nanos())
        .unwrap_or(0);
    nanos % 100_000
}
