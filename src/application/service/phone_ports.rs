//! The PhoneBookPort — messaging's seam to another module's phone-bearing
//! records (hand-written; user-owned).
//!
//! Phone validation is mail-hosted by the one-home ruling: THIS module owns
//! the E.164 formatter, the phone blacklist, and the recipient-candidate
//! CONTRACT — but it must never read another module's tables directly (the
//! MAIL-M16 polymorphic-edge precedent: mail attaches to hosts through
//! `(model, res_id)` keys, never through host-table columns or cross-module
//! FKs). The port keeps that shape for reads: the host service registers ONE
//! implementation that answers "which phone candidates does this record
//! carry, in what order" — the walk order (primary first, mobile over land
//! line, the address country as the sanitize hint) is decided in that single
//! implementation, so no consumer ever invents its own walk.
//!
//! Deny-by-default: the module ships [`RefusingPhoneBook`] — until a host
//! registers a real book via `MessagingModule::set_phone_book(...)`, every
//! candidate lookup fails with [`PhoneSourceError::NotComposed`] (fail
//! closed). A host that forgets to register gets a loud refusal, never a
//! silent empty list that would read as "no phone on record".
//!
//! Mirrors [`crate::application::service::mail_ports::MailApiPort`] (module
//! owns the trait, app wires the impl) and
//! [`crate::application::service::chatter_acl::ThreadAclSlot`] (swappable
//! slot, deny-by-default install) in shape and contract.

use std::sync::Arc;

use async_trait::async_trait;
use uuid::Uuid;

/// Why a candidate lookup failed. There is deliberately no "no candidates"
/// error — an empty candidate list is a legitimate `Ok` (a record with no
/// phone fields sanitizes to a walk failure, not a source failure).
#[derive(Debug, thiserror::Error)]
pub enum PhoneSourceError {
    #[error("db: {0}")]
    Db(#[from] sqlx::Error),
    /// No [`PhoneBookPort`] has been composed for this module — the
    /// deny-by-default refusal. Installing one via
    /// `MessagingModule::set_phone_book` is the only cure.
    #[error("phone book not composed: {detail}")]
    NotComposed { detail: String },
}

/// One raw phone value offered to the sanitizer, plus the country that
/// disambiguates a national format. `raw` is UNSANITIZED by contract (any
/// shape the source stores — separators, national prefix, international
/// form); `country_hint` is ISO-3166 alpha-2 (e.g. `ID`, `US`) and is only
/// consulted when `raw` is not already in international (`+NN…`) form.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PhoneCandidate {
    pub raw: String,
    pub country_hint: Option<String>,
}

impl PhoneCandidate {
    /// An international-form candidate (no hint needed).
    pub fn international(raw: impl Into<String>) -> Self {
        Self { raw: raw.into(), country_hint: None }
    }

    /// A national-form candidate with its disambiguating country.
    pub fn national(raw: impl Into<String>, country_hint: impl Into<String>) -> Self {
        Self { raw: raw.into(), country_hint: Some(country_hint.into()) }
    }
}

/// The recipient-candidate seam. ONE method by design: the candidate walk
/// (which fields, what order, what country hint) is decided exactly here, in
/// the registered implementation — the walk is not extended until a consumer
/// demands it, and never duplicated caller-side.
#[async_trait]
pub trait PhoneBookPort: Send + Sync {
    /// The phone candidates for `(model, res_id)`, in the order the
    /// sanitizer should try them (first valid wins). Empty is a legitimate
    /// answer; failures go through [`PhoneSourceError`].
    async fn candidates(
        &self,
        model: &str,
        res_id: Uuid,
    ) -> Result<Vec<PhoneCandidate>, PhoneSourceError>;
}

/// The deny-by-default implementation: the slot's initial tenant. Every call
/// refuses with [`PhoneSourceError::NotComposed`] — a missing composition is
/// a loud failure, never an empty `Ok` that would read as "no phone on
/// record".
pub struct RefusingPhoneBook;

#[async_trait]
impl PhoneBookPort for RefusingPhoneBook {
    async fn candidates(
        &self,
        model: &str,
        res_id: Uuid,
    ) -> Result<Vec<PhoneCandidate>, PhoneSourceError> {
        Err(PhoneSourceError::NotComposed {
            detail: format!(
                "no PhoneBookPort is installed; refusing candidate lookup for \
                 {model}/{res_id} — compose one via MessagingModule::set_phone_book"
            ),
        })
    }
}

/// The test double: replays the canned candidates for every lookup, no
/// database. The [`crate::application::service::mail_ports::NoopMailApi`]
/// analog — tests drive the sanitize walk without a host module.
pub struct NoopPhoneBook {
    candidates: Vec<PhoneCandidate>,
}

impl NoopPhoneBook {
    /// A book that always answers with these candidates (cloned per call).
    pub fn canned(candidates: Vec<PhoneCandidate>) -> Self {
        Self { candidates }
    }

    /// A book that always answers "no phone fields" — the legitimate-empty
    /// shape (distinct from the refusing default).
    pub fn empty() -> Self {
        Self { candidates: Vec::new() }
    }
}

#[async_trait]
impl PhoneBookPort for NoopPhoneBook {
    async fn candidates(
        &self,
        _model: &str,
        _res_id: Uuid,
    ) -> Result<Vec<PhoneCandidate>, PhoneSourceError> {
        Ok(self.candidates.clone())
    }
}

/// A shared, swappable phone-book slot — how a host registers its candidate
/// walk without the generated module builder needing a new field. Services
/// are built over the slot (defaulting to [`RefusingPhoneBook`]);
/// `MessagingModule::set_phone_book` installs the host's book and every
/// service sees it on the NEXT call. Reads are cheap (an `RwLock` read);
/// installs happen once at boot. The
/// [`crate::application::service::chatter_acl::ThreadAclSlot`] analog.
#[derive(Clone)]
pub struct PhoneBookSlot {
    inner: Arc<std::sync::RwLock<Arc<dyn PhoneBookPort>>>,
}

impl PhoneBookSlot {
    /// Install (replace) the active book.
    pub fn install(&self, book: Arc<dyn PhoneBookPort>) {
        // A poisoned lock still holds the old value — recovering it beats
        // panicking every future lookup over one panicked writer.
        *self.inner.write().unwrap_or_else(|e| e.into_inner()) = book;
    }

    /// The currently installed book.
    pub fn current(&self) -> Arc<dyn PhoneBookPort> {
        self.inner.read().unwrap_or_else(|e| e.into_inner()).clone()
    }
}

impl Default for PhoneBookSlot {
    fn default() -> Self {
        Self { inner: Arc::new(std::sync::RwLock::new(Arc::new(RefusingPhoneBook))) }
    }
}

#[async_trait]
impl PhoneBookPort for PhoneBookSlot {
    async fn candidates(
        &self,
        model: &str,
        res_id: Uuid,
    ) -> Result<Vec<PhoneCandidate>, PhoneSourceError> {
        self.current().candidates(model, res_id).await
    }
}
