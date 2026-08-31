//! The email-blacklist verbs (hand-written; user-owned).
//!
//! The sanctioned write path for `messaging.mail_blacklists` — the port of
//! Odoo's `mail.blacklist.add` / `mail.blacklist.remove`: a canonical upsert
//! `add`, an archive-only `remove`, and the membership lookups send-time
//! suppression checks call. Generated generic CRUD exists on the substrate
//! surface (as with `phone_blacklists`); the guarded compose mounts reads +
//! THESE verbs.
//!
//! Case-folding is app-layer on the email side (unlike the phone side's
//! formatter-gated canonical form): every verb lowercases the address
//! (after trimming) before it touches the table — the unique index fires on
//! the as-stored value, so the fold in THIS service is what makes
//! `User@Example.com` and `user@example.com` converge on one row instead of
//! minting near-duplicates.
//!
//! `opt_out_reason_id` — why the address was listed — is carried by `add`
//! and persisted when supplied (nullable, backward-compatible: adds without
//! a reason keep working and never erase an already-recorded reason; the
//! column references the mailing module's OptOutReason catalog logically,
//! opaque here). `remove` never touches it: the archive pattern retains the
//! row, so the reason a listing existed stays inspectable after the address
//! is un-suppressed.

use std::collections::HashSet;

use uuid::Uuid;

use crate::infrastructure::persistence::mail_blacklist_verb_repository::MailBlacklistVerbRepository;

#[derive(Debug, thiserror::Error)]
pub enum MailBlacklistError {
    #[error("db: {0}")]
    Db(#[from] sqlx::Error),
    #[error("invalid input: {0}")]
    Invalid(String),
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
    /// No row exists for the address — removing an absent address is a no-op.
    NotListed,
}

impl RemoveOutcome {
    /// The archived row, when one exists (`NotListed` has none).
    pub fn row_id(&self) -> Option<Uuid> {
        match self {
            RemoveOutcome::Removed { row_id } | RemoveOutcome::AlreadyInactive { row_id } => {
                Some(*row_id)
            }
            RemoveOutcome::NotListed => None,
        }
    }
}

/// The email-blacklist verb service. SQL lives in
/// [`MailBlacklistVerbRepository`]; this layer shapes outcomes and owns the
/// lowercase fold.
pub struct MailBlacklistWriteService {
    pool: sqlx::PgPool,
}

impl MailBlacklistWriteService {
    pub fn new(pool: sqlx::PgPool) -> Self {
        Self { pool }
    }

    /// Add an address to the blacklist, optionally recording why
    /// (`opt_out_reason_id` — a logical ref to the mailing module's
    /// OptOutReason catalog; `None` is always valid and never erases an
    /// already-recorded reason). Idempotent: a fresh address is newly
    /// listed, an archived row reactivates (same row), an active row stays
    /// as-is. Parallel adds of the same address converge on ONE row (unique
    /// index + upsert). The address is lowercased first (the app-layer
    /// case-fold the schema documents).
    pub async fn add(
        &self,
        email: &str,
        opt_out_reason_id: Option<Uuid>,
    ) -> Result<AddOutcome, MailBlacklistError> {
        let canonical = canonical_email(email)?;
        let mut conn = self.pool.acquire().await?;
        let state = MailBlacklistVerbRepository::upsert_active(
            &mut conn,
            &canonical,
            Uuid::new_v4(),
            opt_out_reason_id,
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

    /// Remove an address from the blacklist — the ARCHIVE pattern:
    /// `active = false`, the row itself (and its recorded opt-out reason)
    /// retained. Idempotent in both directions (already-archived, and
    /// never-existed).
    pub async fn remove(&self, email: &str) -> Result<RemoveOutcome, MailBlacklistError> {
        let canonical = canonical_email(email)?;
        let mut conn = self.pool.acquire().await?;
        let state =
            MailBlacklistVerbRepository::archive_if_active(&mut conn, &canonical).await?;
        Ok(match state {
            None => RemoveOutcome::NotListed,
            Some(s) if s.changed => RemoveOutcome::Removed { row_id: s.row_id },
            Some(s) => RemoveOutcome::AlreadyInactive { row_id: s.row_id },
        })
    }

    /// Is the address actively listed? (Unique-index point lookup — the
    /// send-time suppression check. The probe is lowercased with the same
    /// fold as the write path, so a mixed-case probe matches the stored
    /// canonical row.)
    pub async fn is_listed(&self, email: &str) -> Result<bool, MailBlacklistError> {
        let canonical = canonical_email(email)?;
        let mut conn = self.pool.acquire().await?;
        Ok(MailBlacklistVerbRepository::exists_active(&mut conn, &canonical).await?)
    }

    /// The actively-listed subset of `emails` — ONE parameterized query
    /// (`= ANY`), never a load-all (the unbounded blacklist read is an
    /// Odoo defect this port refuses). Returned addresses are the stored
    /// canonical (lowercased) forms. Empty input short-circuits to the
    /// empty set without touching the database.
    pub async fn listed_among(
        &self,
        emails: &[String],
    ) -> Result<HashSet<String>, MailBlacklistError> {
        if emails.is_empty() {
            return Ok(HashSet::new());
        }
        let canonical: Vec<String> =
            emails.iter().map(|e| canonical_email(e)).collect::<Result<_, _>>()?;
        let mut conn = self.pool.acquire().await?;
        let found = MailBlacklistVerbRepository::active_emails(&mut conn, &canonical).await?;
        Ok(found.into_iter().collect())
    }
}

/// The app-layer case-fold: trim + lowercase. Refuses blank input the same
/// way the queue refuses a blank recipient — a blacklist row keyed on an
/// empty string can never match a real send.
fn canonical_email(raw: &str) -> Result<String, MailBlacklistError> {
    let folded = raw.trim().to_lowercase();
    if folded.is_empty() {
        return Err(MailBlacklistError::Invalid(
            "blacklist email is blank (empty or whitespace)".into(),
        ));
    }
    Ok(folded)
}

#[cfg(test)]
mod tests {
    use super::canonical_email;

    #[test]
    fn the_fold_trims_and_lowercases() {
        assert_eq!(canonical_email("  User@Example.COM  ").unwrap(), "user@example.com");
        assert_eq!(canonical_email("already@lower.example").unwrap(), "already@lower.example");
    }

    #[test]
    fn a_blank_email_is_refused_not_silently_folded() {
        assert!(canonical_email("   ").is_err());
        assert!(canonical_email("").is_err());
    }
}
