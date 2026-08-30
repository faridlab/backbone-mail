//! The E.164 sanitize contract (hand-written; user-owned).
//!
//! The port of Odoo's `phone_validation.phone_format`: ONE typed formatter
//! that turns any raw phone shape into a canonical E.164 `+NN…` string, and
//! ONE first-valid-wins walk over a record's candidates (the
//! `_phone_get_number_fields` walk — first valid sanitized wins; see
//! docs/odoo/messaging/sms/sms-business-logic.md §7.1).
//!
//! REFUSE-LOUDLY, the whole way down: every failure is a TYPED variant that
//! carries the raw input (and the country where one was in play). There is
//! no path that returns an empty string, a `None`, or a stringly error — a
//! number that cannot be sanitized honestly is an `Err`, because a silent
//! fallback here would send SMS to a wrong-or-garbage number rather than
//! surface the refusal (the same posture the sms queue applies when it
//! cancels a row with `sms_number_format` instead of guessing).
//!
//! No stored cache: the verb [`PhoneValidationService::sanitized_for`]
//! computes a record's sanitized number (and its blacklist standing) live on
//! every call. Odoo caches `phone_sanitized` as a stored compute for
//! UI/statistics convenience; the recorded deviation drops the stored half —
//! a fence-none messaging schema must not persist copies of strict-fenced
//! party data, and send-time consumers need the value exactly when this
//! verb computes it anyway. The one consumer that genuinely needs a stored
//! column (mailing contact statistics) writes it on ITS OWN model through
//! this formatter.

use std::collections::HashSet;
use std::fmt;
use std::str::FromStr;

use serde::Serialize;
use uuid::Uuid;

use crate::application::service::phone_blacklist_write_service::{
    PhoneBlacklistError, PhoneBlacklistWriteService,
};
use crate::application::service::phone_ports::{PhoneBookPort, PhoneBookSlot, PhoneCandidate};

/// A canonical E.164 phone number (`+NN…`, the shape the phone blacklist
/// keys on and the sms queue sends to).
///
/// The invariant — `+`, a non-zero country digit, then 7–15 digits (the
/// E.164 bound `^\+[1-9]\d{6,14}$`) — holds for every value of this type by
/// construction: [`phone_format`] is the ONLY public constructor. The
/// internal [`E164Number::from_canonical`] exists for values already known
/// canonical (formatter output round-trips, canonical columns this module
/// itself wrote) and re-asserts the invariant.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct E164Number(String);

impl E164Number {
    /// Constructor for values already known canonical (formatter output,
    /// columns written only by the formatter). Panics in debug builds if the
    /// invariant is violated — a non-canonical input here is a bug in the
    /// write path, not a data condition to shrug off.
    pub(crate) fn from_canonical(canonical: String) -> Self {
        debug_assert!(
            is_canonical_e164(&canonical),
            "E164Number invariant violated: {canonical:?}"
        );
        Self(canonical)
    }
}

impl fmt::Display for E164Number {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl AsRef<str> for E164Number {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

impl Serialize for E164Number {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.0)
    }
}

/// The E.164 shape check: `+`, a `1..=9` country digit, then 6–14 more
/// digits — i.e. `^\+[1-9]\d{6,14}$` (8–16 characters total).
fn is_canonical_e164(s: &str) -> bool {
    let mut chars = s.chars();
    if chars.next() != Some('+') {
        return false;
    }
    match chars.next() {
        Some(c) if c.is_ascii_digit() && c != '0' => {}
        _ => return false,
    }
    let rest: Vec<char> = chars.collect();
    (6..=14).contains(&rest.len()) && rest.iter().all(|c| c.is_ascii_digit())
}

/// Why a raw value could not be sanitized. Every variant carries the raw
/// input (and the country, where one was in play) — refusals are auditable,
/// never just a boolean.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum PhoneFormatError {
    #[error("phone number is empty")]
    Empty { raw: String },
    #[error("not a phone number ({detail}): {raw:?}")]
    NotANumber { raw: String, detail: String },
    #[error("invalid country hint {hint:?}: {raw:?}")]
    InvalidCountryHint { raw: String, hint: String },
    #[error("national-format number with no country hint: {raw:?}")]
    MissingCountryHint { raw: String },
    #[error("phone number too long for E.164: {raw:?}")]
    TooLong { raw: String },
    #[error("phone number too short to be a phone number: {raw:?}")]
    TooShort { raw: String },
    #[error("number is not valid for country {country}: {raw:?}")]
    InvalidForCountry { raw: String, country: String },
}

/// Normalize a country hint to the ISO-3166 alpha-2 form the metadata
/// database keys on (trim + ASCII-uppercase, e.g. `id` → `ID`). `None` when
/// the hint is not two alphabetic characters — the caller refuses loudly
/// with it rather than dropping it.
fn normalize_hint(hint: &str) -> Option<String> {
    let up = hint.trim().to_ascii_uppercase();
    (up.len() == 2 && up.chars().all(|c| c.is_ascii_alphabetic())).then_some(up)
}

/// Sanitize one raw phone value to a canonical [`E164Number`].
///
/// Input already in international form (leading `+`) ignores the country
/// hint; national input REQUIRES one (which field of which record carried
/// the number is the caller's knowledge — the geo-IP/company ladder Odoo's
/// STOP route applies at its layer composes the same hint it passes here).
///
/// Refuses loudly on every failure shape: empty, not-a-number, over/under
/// length, bogus hint, national-without-hint, and parses-but-is-not-valid
/// (a number shaped for the wrong country).
pub fn phone_format(
    raw: &str,
    country_hint: Option<&str>,
) -> Result<E164Number, PhoneFormatError> {
    let raw = raw.trim();
    if raw.is_empty() {
        return Err(PhoneFormatError::Empty { raw: raw.into() });
    }

    // Already-international input ignores the hint entirely.
    let international = raw.starts_with('+');

    let country = if international {
        None
    } else {
        let hint = country_hint
            .ok_or_else(|| PhoneFormatError::MissingCountryHint { raw: raw.into() })?;
        let normalized = normalize_hint(hint).ok_or_else(|| PhoneFormatError::InvalidCountryHint {
            raw: raw.into(),
            hint: hint.into(),
        })?;
        let id = phonenumber::country::Id::from_str(&normalized)
            .map_err(|_| PhoneFormatError::InvalidCountryHint {
                raw: raw.into(),
                hint: normalized.clone(),
            })?;
        Some((normalized, id))
    };

    let parsed = phonenumber::parse(country.as_ref().map(|(_, id)| *id), raw)
        .map_err(|e| map_parse_error(e, raw, country.as_ref().map(|(n, _)| n.clone())))?;

    // The E.164 digit budget (7–15 digits including the country code),
    // enforced on THIS side of the contract: the parser occasionally lets an
    // over-length national significant number through (its viability check
    // and its NSN bound disagree at the edges), and an over-budget number
    // must refuse as TooLong — not fall through to the country-validity
    // check with a nonsense country citation.
    let total_digits = parsed.country().code().to_string().len()
        + parsed.national().to_string().len();
    if total_digits > 15 {
        return Err(PhoneFormatError::TooLong { raw: raw.into() });
    }
    if total_digits < 7 {
        return Err(PhoneFormatError::TooShort { raw: raw.into() });
    }

    // Parsed but wrong shape for the country in play (e.g. a national number
    // hinted US that is not a valid US number): refuse, do not guess.
    if !phonenumber::is_valid(&parsed) {
        return Err(PhoneFormatError::InvalidForCountry {
            raw: raw.into(),
            country: country
                .map(|(n, _)| n)
                .unwrap_or_else(|| parsed_country_or_prefix(&parsed)),
        });
    }

    let canonical = phonenumber::format(&parsed).to_string();
    if !is_canonical_e164(&canonical) {
        // Unreachable while the metadata database agrees with E.164; kept as
        // a loud guard so a metadata drift cannot mint a non-canonical row.
        return Err(PhoneFormatError::NotANumber {
            raw: raw.into(),
            detail: format!("formatted to non-canonical {canonical:?}"),
        });
    }
    Ok(E164Number(canonical))
}

/// Map the parser's error onto the typed refusal vocabulary. The parser has
/// no missing-country variant of its own (national input with no country
/// surfaces there as an invalid country code), so the hint handling above
/// decides that class before the parser ever sees it.
fn map_parse_error(
    e: phonenumber::ParseError,
    raw: &str,
    hint_country: Option<String>,
) -> PhoneFormatError {
    use phonenumber::ParseError as E;
    match e {
        E::TooLong => PhoneFormatError::TooLong { raw: raw.into() },
        E::TooShortNsn | E::TooShortAfterIdd => PhoneFormatError::TooShort { raw: raw.into() },
        E::InvalidCountryCode => match hint_country {
            // A real hint was in play and the number still does not carry a
            // usable country shape — wrong-country class.
            Some(country) => {
                PhoneFormatError::InvalidForCountry { raw: raw.into(), country }
            }
            None => PhoneFormatError::NotANumber {
                raw: raw.into(),
                detail: "invalid country code".into(),
            },
        },
        other => PhoneFormatError::NotANumber { raw: raw.into(), detail: other.to_string() },
    }
}

/// The display country for an already-international refusal: the parsed
/// country code (there is no alpha-2 hint to cite).
fn parsed_country_or_prefix(parsed: &phonenumber::PhoneNumber) -> String {
    format!("+{}", parsed.country().code())
}

/// The total-failure audit trail of a candidate walk: every candidate, with
/// the index it was tried at and the typed refusal it produced.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("no sanitizable phone candidate: {}", .0
    .iter()
    .map(|(i, e)| format!("[{i}] {e}"))
    .collect::<Vec<_>>()
    .join("; "))]
pub struct SanitizeFailure(pub Vec<(usize, PhoneFormatError)>);

/// The first-valid-wins candidate walk (the `_phone_get_number_fields`
/// order): try each candidate through [`phone_format`]; the first success is
/// the record's number. Total failure refuses loudly WITH the trail — one
/// typed error per candidate, indexed.
pub fn sanitize_candidates(candidates: &[PhoneCandidate]) -> Result<E164Number, SanitizeFailure> {
    let mut trail = Vec::with_capacity(candidates.len());
    for (index, candidate) in candidates.iter().enumerate() {
        match phone_format(&candidate.raw, candidate.country_hint.as_deref()) {
            Ok(number) => return Ok(number),
            Err(e) => trail.push((index, e)),
        }
    }
    Err(SanitizeFailure(trail))
}

/// The live read-side answer for one record: its sanitized number and
/// whether that number is on the active phone blacklist. The two partner
/// "fields" of the spec (phone_sanitized / phone_blacklisted), as ONE verb —
/// computed live, persisting nothing.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SanitizedPhone {
    pub number: E164Number,
    pub blacklisted: bool,
}

/// Failures of the live verb: the source refused (not composed / db), the
/// walk refused (typed, with trail), or the blacklist lookup failed.
#[derive(Debug, thiserror::Error)]
pub enum PhoneValidationError {
    #[error(transparent)]
    Source(#[from] crate::application::service::phone_ports::PhoneSourceError),
    #[error(transparent)]
    Walk(#[from] SanitizeFailure),
    #[error(transparent)]
    Blacklist(#[from] PhoneBlacklistError),
}

/// The phone-validation facade: the record verb over the [`PhoneBookSlot`]
/// (the host-composed candidate walk) and the blacklist verbs.
pub struct PhoneValidationService {
    #[allow(dead_code)] // the pool threads into future route-layer surfaces
    pool: sqlx::PgPool,
    phone_book: PhoneBookSlot,
    blacklist: std::sync::Arc<PhoneBlacklistWriteService>,
}

impl PhoneValidationService {
    pub fn new(
        pool: sqlx::PgPool,
        phone_book: PhoneBookSlot,
        blacklist: std::sync::Arc<PhoneBlacklistWriteService>,
    ) -> Self {
        Self { pool, phone_book, blacklist }
    }

    /// The record verb: candidates from the composed phone book, sanitized
    /// first-valid-wins, checked against the active blacklist. Live compute,
    /// no persistence — see the module header for the recorded deviation
    /// from Odoo's stored `phone_sanitized` cache.
    pub async fn sanitized_for(
        &self,
        model: &str,
        res_id: Uuid,
    ) -> Result<SanitizedPhone, PhoneValidationError> {
        let candidates = self.phone_book.candidates(model, res_id).await?;
        let number = sanitize_candidates(&candidates)?;
        let blacklisted = self.blacklist.is_listed(&number).await?;
        Ok(SanitizedPhone { number, blacklisted })
    }

    /// Is this canonical number on the active blacklist? (The point lookup
    /// behind send-time suppression checks.)
    pub async fn check(&self, number: &E164Number) -> Result<bool, PhoneBlacklistError> {
        self.blacklist.is_listed(number).await
    }

    /// The active subset of `numbers` — ONE parameterized query, never a
    /// load-all (the unbounded blacklist load is a known Odoo defect this
    /// port refuses to copy). Exposed for callers that batch-check.
    pub async fn check_all(
        &self,
        numbers: &[E164Number],
    ) -> Result<HashSet<E164Number>, PhoneBlacklistError> {
        self.blacklist.listed_among(numbers).await
    }
}
