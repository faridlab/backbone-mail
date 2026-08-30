//! Phone-validation opener cases (hand-written; user-owned).
//!
//! Exercises the E.164 sanitize contract against a real Postgres: format
//! round-trips (including the refusal corpus — typed variants that carry
//! the raw input, never a silent `Ok`), the first-valid-wins walk, the
//! blacklist verbs (canonical convergence, archive semantics, batch
//! membership, parallel-add convergence), the port slot (composed and
//! fail-closed), and the no-storage / fence posture probes. Each test runs
//! on its own scratch database (`sqlx::test`) with the module migrations
//! applied; the scratch DB is created and dropped by the harness.

use std::collections::HashSet;
use std::sync::Arc;

use backbone_mail::application::service::phone_blacklist_write_service::{
    AddOutcome, PhoneBlacklistWriteService, RemoveOutcome,
};
use backbone_mail::application::service::phone_ports::{
    NoopPhoneBook, PhoneBookPort, PhoneBookSlot, PhoneCandidate, PhoneSourceError,
};
use backbone_mail::application::service::phone_validation_service::{
    phone_format, sanitize_candidates, E164Number, PhoneFormatError, PhoneValidationError,
    PhoneValidationService, SanitizeFailure,
};
use sqlx::PgPool;
use uuid::Uuid;

/// The E.164 shape, re-stated for the assertion side: `^\+[1-9]\d{6,14}$`.
fn looks_e164(s: &str) -> bool {
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

fn fmt(raw: &str, hint: Option<&str>) -> E164Number {
    phone_format(raw, hint).expect("expected sanitizable input")
}

fn verbs(pool: &PgPool) -> PhoneBlacklistWriteService {
    PhoneBlacklistWriteService::new(pool.clone())
}

/// A validation service over the canned `NoopPhoneBook` (the composed-host
/// shape, without a host module).
fn validation_with_book(
    pool: &PgPool,
    candidates: Vec<PhoneCandidate>,
) -> PhoneValidationService {
    let slot = PhoneBookSlot::default();
    slot.install(Arc::new(NoopPhoneBook::canned(candidates)));
    PhoneValidationService::new(
        pool.clone(),
        slot,
        Arc::new(PhoneBlacklistWriteService::new(pool.clone())),
    )
}

/// A validation service over the UNSET slot (the fail-closed default).
fn validation_uncomposed(pool: &PgPool) -> PhoneValidationService {
    PhoneValidationService::new(
        pool.clone(),
        PhoneBookSlot::default(),
        Arc::new(PhoneBlacklistWriteService::new(pool.clone())),
    )
}

async fn rows_in_phone_blacklist(pool: &PgPool) -> i64 {
    sqlx::query_scalar("SELECT count(*) FROM messaging.phone_blacklists")
        .fetch_one(pool)
        .await
        .expect("count phone_blacklists")
}

// ─── format round-trips ──────────────────────────────────────────────────────

#[sqlx::test(migrations = "./migrations")]
async fn format_corpus_national_international_and_separators(_pool: PgPool) {
    // National + hint.
    assert_eq!(fmt("08123456789", Some("ID")).as_ref(), "+628123456789");
    // US national with punctuation.
    assert_eq!(fmt("(415) 555-2671", Some("US")).as_ref(), "+14155552671");
    // E.164 passthrough is identity.
    assert_eq!(fmt("+628123456789", None).as_ref(), "+628123456789");
    // Separators (spaces) stripped from already-international input.
    assert_eq!(fmt("+62 812 3456 7890", None).as_ref(), "+6281234567890");
    // Separators (dashes) in national input.
    assert_eq!(fmt("0812-3456-7890", Some("ID")).as_ref(), "+6281234567890");
    // A hint is IGNORED for already-international input: the US hint must
    // not pull an ID number toward +1.
    assert_eq!(fmt("+6281234567890", Some("US")).as_ref(), "+6281234567890");
    // Lowercase hints normalize to the alpha-2 form.
    assert_eq!(fmt("08123456789", Some("id")).as_ref(), "+628123456789");
    // Every Ok carries the E.164 shape.
    for (raw, hint) in [
        ("08123456789", Some("ID")),
        ("(415) 555-2671", Some("US")),
        ("+628123456789", None),
        ("+1 415 555 2671", None),
    ] {
        let n = fmt(raw, hint);
        assert!(looks_e164(n.as_ref()), "{raw:?} -> {n} is not E.164");
    }
}

#[test]
fn refusal_corpus_is_typed_and_carries_the_raw() {
    // Empty.
    match phone_format("   ", None) {
        Err(PhoneFormatError::Empty { raw }) => assert_eq!(raw, ""),
        other => panic!("empty input must refuse with Empty, got {other:?}"),
    }
    // Alphabetic.
    match phone_format("not-a-phone", Some("US")) {
        Err(PhoneFormatError::NotANumber { raw, .. }) => assert_eq!(raw, "not-a-phone"),
        other => panic!("alphabetic must refuse with NotANumber, got {other:?}"),
    }
    // Over the E.164 digit budget.
    match phone_format("+628123456789012345", None) {
        Err(PhoneFormatError::TooLong { raw }) => {
            assert_eq!(raw, "+628123456789012345");
        }
        other => panic!("19 digits must refuse with TooLong, got {other:?}"),
    }
    // Truncated international.
    match phone_format("+62 81", None) {
        Err(PhoneFormatError::TooShort { raw }) => assert_eq!(raw, "+62 81"),
        other => panic!("truncated must refuse with TooShort, got {other:?}"),
    }
    // National with no hint.
    match phone_format("08123456789", None) {
        Err(PhoneFormatError::MissingCountryHint { raw }) => assert_eq!(raw, "08123456789"),
        other => panic!("hintless national must refuse with MissingCountryHint, got {other:?}"),
    }
    // Hints that are not ISO-3166 alpha-2 countries.
    for bogus in ["XX", "usa", "1A"] {
        match phone_format("08123456789", Some(bogus)) {
            Err(PhoneFormatError::InvalidCountryHint { raw, hint }) => {
                assert_eq!(raw, "08123456789");
                assert_ne!(hint, "ID");
            }
            other => panic!("bogus hint {bogus:?} must refuse with InvalidCountryHint, got {other:?}"),
        }
    }
    // National number hinted for the WRONG country: parses, but is not a
    // valid number there — refused, not guessed.
    match phone_format("08123456789", Some("US")) {
        Err(PhoneFormatError::InvalidForCountry { raw, country }) => {
            assert_eq!(raw, "08123456789");
            assert_eq!(country, "US");
        }
        other => panic!("wrong-country shape must refuse with InvalidForCountry, got {other:?}"),
    }
}

// ─── the candidate walk ──────────────────────────────────────────────────────

#[test]
fn walk_is_first_valid_wins() {
    let candidates = vec![
        PhoneCandidate::national("not-a-phone", "ID"),
        PhoneCandidate::national("0812", "ID"), // too short
        PhoneCandidate::national("08123456789", "ID"),
    ];
    let winner = sanitize_candidates(&candidates).expect("third candidate must win");
    assert_eq!(winner.as_ref(), "+628123456789");
}

#[test]
fn walk_total_failure_refuses_with_the_full_trail() {
    let candidates = vec![
        PhoneCandidate::national("not-a-phone", "ID"),
        PhoneCandidate::national("08123456789", "US"), // wrong country
    ];
    let err = sanitize_candidates(&candidates).expect_err("all-bad must refuse");
    let SanitizeFailure(trail) = &err;
    assert_eq!(trail.len(), 2, "one typed cause per candidate");
    assert_eq!(trail[0].0, 0);
    assert!(matches!(trail[0].1, PhoneFormatError::NotANumber { .. }));
    assert_eq!(trail[1].0, 1);
    assert!(matches!(trail[1].1, PhoneFormatError::InvalidForCountry { .. }));
}

// ─── blacklist verbs ─────────────────────────────────────────────────────────

#[sqlx::test(migrations = "./migrations")]
async fn raw_variants_converge_on_one_canonical_row(pool: PgPool) {
    let svc = verbs(&pool);

    // Three raw shapes of the SAME number: international with separators,
    // international run-together, and national (leading 0) with a country
    // hint.
    let a = svc.add(&fmt("+62 812 3456 7890", None)).await.expect("add a");
    let b = svc.add(&fmt("+6281234567 890", None)).await.expect("add b");
    let c = svc.add(&fmt("0812 3456 7890", Some("ID"))).await.expect("add c");

    assert!(matches!(a, AddOutcome::NewlyListed { .. }));
    // The near-duplicate class the email blacklist documents cannot happen
    // here: the value is canonical before the unique ever sees it.
    assert!(matches!(b, AddOutcome::AlreadyListed { .. }));
    assert!(matches!(c, AddOutcome::AlreadyListed { .. }));
    assert_eq!(a.row_id(), b.row_id());
    assert_eq!(b.row_id(), c.row_id());

    assert_eq!(rows_in_phone_blacklist(&pool).await, 1);
    let stored: String =
        sqlx::query_scalar("SELECT number FROM messaging.phone_blacklists")
            .fetch_one(&pool)
            .await
            .expect("stored number");
    assert_eq!(stored, "+6281234567890");
}

#[sqlx::test(migrations = "./migrations")]
async fn remove_archives_and_readd_reactivates_the_same_row(pool: PgPool) {
    let svc = verbs(&pool);
    let number = fmt("08123456789", Some("ID"));

    let added = svc.add(&number).await.expect("add");
    let removed = svc.remove(&number).await.expect("remove");
    match removed {
        RemoveOutcome::Removed { row_id } => assert_eq!(row_id, added.row_id()),
        other => panic!("expected Removed, got {other:?}"),
    }

    // The row is RETAINED, archived.
    assert_eq!(rows_in_phone_blacklist(&pool).await, 1);
    let active: bool =
        sqlx::query_scalar("SELECT active FROM messaging.phone_blacklists WHERE number = $1")
            .bind(number.as_ref())
            .fetch_one(&pool)
            .await
            .expect("active flag");
    assert!(!active, "remove archives (active=false), never deletes");
    assert!(!svc.is_listed(&number).await.expect("is_listed"));

    // Re-add reactivates the SAME row.
    let readded = svc.add(&number).await.expect("re-add");
    assert!(matches!(readded, AddOutcome::Reactivated { .. }));
    assert_eq!(readded.row_id(), added.row_id());
    assert_eq!(rows_in_phone_blacklist(&pool).await, 1, "no second row");
    assert!(svc.is_listed(&number).await.expect("is_listed after re-add"));

    // Double remove: first flips, second is idempotent.
    assert!(matches!(svc.remove(&number).await.expect("remove 2"), RemoveOutcome::Removed { .. }));
    assert!(matches!(
        svc.remove(&number).await.expect("remove 3"),
        RemoveOutcome::AlreadyInactive { .. }
    ));

    // Removing a number that was never listed is a normal no-op.
    let absent = fmt("+14155552671", None);
    assert!(matches!(svc.remove(&absent).await.expect("remove absent"), RemoveOutcome::NotListed));
}

#[sqlx::test(migrations = "./migrations")]
async fn listed_among_answers_membership_not_load_all(pool: PgPool) {
    let svc = verbs(&pool);

    let listed: Vec<E164Number> = vec![
        fmt("08123456789", Some("ID")),
        fmt("(415) 555-2671", Some("US")),
        fmt("+447400123456", None),
    ];
    for n in &listed {
        svc.add(n).await.expect("add listed");
    }
    // An ARCHIVED row and an unknown number: neither may come back.
    let archived = fmt("08120000000", Some("ID"));
    svc.add(&archived).await.expect("add to-be-archived");
    svc.remove(&archived).await.expect("archive");
    let unknown = fmt("+819012345678", None);

    let mut probe = listed.clone();
    probe.push(archived.clone());
    probe.push(unknown.clone());

    let found = svc.listed_among(&probe).await.expect("listed_among");
    let expected: HashSet<E164Number> = listed.iter().cloned().collect();
    assert_eq!(found, expected, "exactly the active subset");
    assert!(!found.contains(&archived), "archived rows are not listed");
    assert!(!found.contains(&unknown), "unknown numbers are not listed");

    // Empty input: the empty set, without touching the database (the table
    // is non-empty here — a load-all shape would leak rows into the answer).
    assert!(svc.listed_among(&[]).await.expect("empty probe").is_empty());
}

#[sqlx::test(migrations = "./migrations")]
async fn parallel_adds_converge_on_one_row(pool: PgPool) {
    let svc = Arc::new(verbs(&pool));
    let number = fmt("08123456789", Some("ID"));

    let (a, b) = tokio::join!(
        async { svc.add(&number).await },
        async { svc.add(&number).await },
    );
    let (a, b) = (a.expect("add a"), b.expect("add b"));
    // Whichever statement inserted, the survivor is ONE row at ONE id.
    assert_eq!(a.row_id(), b.row_id());
    assert_eq!(rows_in_phone_blacklist(&pool).await, 1, "unique + upsert converge");
    assert!(svc.is_listed(&number).await.expect("is_listed"));
}

// ─── the port + the live verb ────────────────────────────────────────────────

#[sqlx::test(migrations = "./migrations")]
async fn sanitized_for_computes_number_and_blacklist_standing_live(pool: PgPool) {
    // Composed book: national candidate with its country hint (the host
    // walk supplies the hint; the module sanitizes).
    let unlisted_number = fmt("08123456789", Some("ID"));
    let svc = validation_with_book(
        &pool,
        vec![PhoneCandidate::national("0812 3456 789", "ID")],
    );

    let res_id = Uuid::new_v4();
    let got = svc
        .sanitized_for("party.party", res_id)
        .await
        .expect("sanitized_for over composed book");
    assert_eq!(got.number, unlisted_number);
    assert!(!got.blacklisted, "number not yet blacklisted");

    // Blacklist the number; the SAME live verb now reports blacklisted —
    // no cache, no stored column, no invalidation to get wrong.
    verbs(&pool).add(&unlisted_number).await.expect("blacklist");
    let got = svc
        .sanitized_for("party.party", res_id)
        .await
        .expect("sanitized_for again");
    assert_eq!(got.number, unlisted_number);
    assert!(got.blacklisted);
}

#[sqlx::test(migrations = "./migrations")]
async fn unset_phone_book_refuses_loudly_never_empty_ok(pool: PgPool) {
    let svc = validation_uncomposed(&pool);
    let err = svc
        .sanitized_for("party.party", Uuid::new_v4())
        .await
        .expect_err("uncomposed book must refuse");
    match err {
        PhoneValidationError::Source(PhoneSourceError::NotComposed { detail }) => {
            assert!(detail.contains("set_phone_book"), "the refusal must name the cure: {detail}");
        }
        other => panic!("expected NotComposed, got {other:?}"),
    }
}

#[sqlx::test(migrations = "./migrations")]
async fn check_and_check_all_answer_from_the_blacklist(pool: PgPool) {
    let listed = fmt("08123456789", Some("ID"));
    let other = fmt("+14155552671", None);
    verbs(&pool).add(&listed).await.expect("add");

    let svc = validation_with_book(&pool, vec![]);
    assert!(svc.check(&listed).await.expect("check listed"));
    assert!(!svc.check(&other).await.expect("check other"));
    assert_eq!(
        svc.check_all(&[listed.clone(), other]).await.expect("check_all"),
        HashSet::from([listed])
    );
}

// ─── posture probes ──────────────────────────────────────────────────────────

/// The read-side-only posture is PROBE-ENFORCED: after a live
/// `sanitized_for`, the messaging schema must contain no sanitized-phone
/// column or table anywhere — the compute is live or it does not exist.
/// (The recorded deviation from Odoo's stored `phone_sanitized` cache: a
/// fence-none messaging schema does not persist strict-fenced party data.)
#[sqlx::test(migrations = "./migrations")]
async fn no_sanitized_storage_appears_anywhere_in_messaging(pool: PgPool) {
    let svc = validation_with_book(
        &pool,
        vec![PhoneCandidate::national("0812 3456 789", "ID")],
    );
    let _ = svc
        .sanitized_for("party.party", Uuid::new_v4())
        .await
        .expect("sanitized_for");

    let columns: i64 = sqlx::query_scalar(
        r#"SELECT count(*) FROM information_schema.columns
           WHERE table_schema = 'messaging'
             AND column_name ILIKE '%sanitize%'"#,
    )
    .fetch_one(&pool)
    .await
    .expect("column sweep");
    assert_eq!(columns, 0, "no phone-sanitized column may exist in messaging");

    let tables: i64 = sqlx::query_scalar(
        r#"SELECT count(*) FROM information_schema.tables
           WHERE table_schema = 'messaging'
             AND table_name ILIKE '%sanitize%'"#,
    )
    .fetch_one(&pool)
    .await
    .expect("table sweep");
    assert_eq!(tables, 0, "no sanitized-cache table may exist in messaging");
}

/// FENCE POSTURE (documented in-test, per the recorded register row):
/// `messaging.phone_blacklists` is a GLOBAL consent list — it carries NO
/// company column and no RLS, by declaration (Odoo's phone.blacklist
/// suppresses sends regardless of company context; the module is
/// company_fence: none per ADR-0014 posture 4). The cross-company probe
/// governs fenced family entities, not this table; routes over it stay
/// operator-gated.
#[sqlx::test(migrations = "./migrations")]
async fn phone_blacklists_is_global_by_declaration_no_company_column(pool: PgPool) {
    // Probed statically (no rows needed): the shape itself is the posture.
    let company_columns: i64 = sqlx::query_scalar(
        r#"SELECT count(*) FROM information_schema.columns
           WHERE table_schema = 'messaging'
             AND table_name = 'phone_blacklists'
             AND column_name = 'company_id'"#,
    )
    .fetch_one(&pool)
    .await
    .expect("company column probe");
    assert_eq!(
        company_columns, 0,
        "phone_blacklists is global-by-declaration; a company column would be decorative fencing"
    );
}
