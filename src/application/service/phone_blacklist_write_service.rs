//! The phone-blacklist verbs (hand-written; user-owned).
//!
//! The sanctioned write path for `messaging.phone_blacklists` — the port of
//! Odoo's `phone.blacklist.add` / `phone.blacklist.remove` (which inherit
//! the mail.blacklist CRUD): a canonical upsert `add`, an archive-only
//! `remove`, and the membership lookups send-time suppression checks call.
//! Generated generic CRUD exists on the substrate surface as with
//! `mail_blacklists`; the guarded compose mounts reads + THESE verbs.
//!
//! Every `add`/`remove` takes an [`E164Number`] — a value that can only
//! exist because the formatter produced it, so the table's canonical unique
//! holds by construction and raw variants converge to one row instead of
//! minting near-duplicates (the documented weakness of the email
//! blacklist's app-layer case-fold).
//!
//! The verbs carry NO reason context: Odoo's `phone.blacklist` has no reason
//! column (STOP-context logging rides the SMS overlay increment). The email
//! blacklist's `opt_out_reason_id` is a logical ref to the mailing module's
//! catalog, opaque here.

use std::collections::HashSet;

use uuid::Uuid;

use crate::application::service::phone_validation_service::E164Number;
use crate::infrastructure::persistence::phone_blacklist_verb_repository::PhoneBlacklistVerbRepository;

#[derive(Debug, thiserror::Error)]
pub enum PhoneBlacklistError {
    #[error("db: {0}")]
    Db(#[from] sqlx::Error),
}

/// The outcome of `add` — which of the three idempotent states the upsert
/// landed in (the surviving row is the same in all three).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AddOutcome {
    /// No row existed; a new active row was created.
    NewlyListed { row_id: Uuid },
    /// A row existed but was archived; it is active again (same row id).
    Reactivated { row_id: Uuid },
    /// A row existed and was already active; nothing changed.
    AlreadyListed { row_id: Uuid },
}

impl AddOutcome {
    /// The surviving row (same row across all three outcomes).
    pub fn row_id(&self) -> Uuid {
        match self {
            AddOutcome::NewlyListed { row_id }
            | AddOutcome::Reactivated { row_id }
            | AddOutcome::AlreadyListed { row_id } => *row_id,
        }
    }
}

/// The outcome of `remove` — the archive pattern: the row is retained
/// (`active = false`), never deleted; absent is a normal, idempotent answer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RemoveOutcome {
    /// An active row was archived (`active` flipped to false, row retained).
    Removed { row_id: Uuid },
    /// The row was already archived; nothing changed.
    AlreadyInactive { row_id: Uuid },
    /// No row exists for the number — removing an absent number is a no-op.
    NotListed,
}

/// The phone-blacklist verb service. SQL lives in
/// [`PhoneBlacklistVerbRepository`]; this layer shapes outcomes.
pub struct PhoneBlacklistWriteService {
    pool: sqlx::PgPool,
}

impl PhoneBlacklistWriteService {
    pub fn new(pool: sqlx::PgPool) -> Self {
        Self { pool }
    }

    /// Add a canonical number to the blacklist. Idempotent: a fresh number
    /// is newly listed, an archived row reactivates (same row), an active
    /// row stays as-is. Parallel adds of the same number converge on ONE
    /// row (unique index + upsert).
    pub async fn add(&self, number: &E164Number) -> Result<AddOutcome, PhoneBlacklistError> {
        let mut conn = self.pool.acquire().await?;
        let state = PhoneBlacklistVerbRepository::upsert_active(
            &mut conn,
            number.as_ref(),
            Uuid::new_v4(),
        )
        .await?;
        let outcome = if !state.existed {
            AddOutcome::NewlyListed { row_id: state.row_id }
        } else if state.was_active {
            AddOutcome::AlreadyListed { row_id: state.row_id }
        } else {
            AddOutcome::Reactivated { row_id: state.row_id }
        };
        Ok(outcome)
    }

    /// Remove a canonical number from the blacklist — the ARCHIVE pattern:
    /// `active = false`, the row itself retained. Idempotent in both
    /// directions (already-archived, and never-existed).
    pub async fn remove(&self, number: &E164Number) -> Result<RemoveOutcome, PhoneBlacklistError> {
        let mut conn = self.pool.acquire().await?;
        let state =
            PhoneBlacklistVerbRepository::archive_if_active(&mut conn, number.as_ref()).await?;
        Ok(match state {
            None => RemoveOutcome::NotListed,
            Some(s) if s.changed => RemoveOutcome::Removed { row_id: s.row_id },
            Some(s) => RemoveOutcome::AlreadyInactive { row_id: s.row_id },
        })
    }

    /// Is the canonical number actively listed? (Unique-index point
    /// lookup — the send-time suppression check.)
    pub async fn is_listed(&self, number: &E164Number) -> Result<bool, PhoneBlacklistError> {
        let mut conn = self.pool.acquire().await?;
        Ok(PhoneBlacklistVerbRepository::exists_active(&mut conn, number.as_ref()).await?)
    }

    /// The actively-listed subset of `numbers` — ONE parameterized query
    /// (`= ANY`), never a load-all (the unbounded blacklist read is an
    /// Odoo defect this port refuses). Empty input short-circuits to the
    /// empty set without touching the database.
    pub async fn listed_among(
        &self,
        numbers: &[E164Number],
    ) -> Result<HashSet<E164Number>, PhoneBlacklistError> {
        if numbers.is_empty() {
            return Ok(HashSet::new());
        }
        let canonical: Vec<String> = numbers.iter().map(|n| n.as_ref().to_string()).collect();
        let mut conn = self.pool.acquire().await?;
        let found = PhoneBlacklistVerbRepository::active_numbers(&mut conn, &canonical).await?;
        Ok(found
            .into_iter()
            .map(E164Number::from_canonical)
            .collect())
    }
}
