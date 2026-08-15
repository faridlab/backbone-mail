//! Alias resolution (MAIL-M33): the COALESCE-unique semantics — a NULL domain
//! matches ONLY a NULL-domain alias and vice versa; lookup is case-insensitive
//! on the local part.

use backbone_mail::application::service::AliasWriteService;
use uuid::Uuid;

use super::common;

async fn seed_alias(pool: &sqlx::PgPool, name: &str, domain: Option<Uuid>) -> Uuid {
    let id = Uuid::new_v4();
    sqlx::query("INSERT INTO messaging.mail_aliases (id, alias_name, alias_domain_id) VALUES ($1, $2, $3)")
        .bind(id)
        .bind(name)
        .bind(domain)
        .execute(pool)
        .await
        .expect("seed alias");
    id
}

#[tokio::test]
async fn resolve_honors_null_domain_semantics() {
    let Some(pool) = common::test_pool().await else {
        common::skipped("alias COALESCE-unique resolution");
        return;
    };
    let svc = AliasWriteService::new(pool.clone());
    let d1 = Uuid::new_v4();
    let null_id = seed_alias(&pool, "sales", None).await;
    let dom_id = seed_alias(&pool, "sales", Some(d1)).await;

    // NULL domain resolves against the NULL-domain alias only.
    let got = svc.resolve("sales", None).await.unwrap().expect("null-domain alias");
    assert_eq!(got.id, null_id);
    assert!(got.alias_domain_id.is_none());

    // A concrete domain resolves against the matching domain row only.
    let got = svc.resolve("sales", Some(d1)).await.unwrap().expect("domain alias");
    assert_eq!(got.id, dom_id);

    // A DIFFERENT domain must NOT fall back to the NULL-domain row — that is
    // the whole point of MAIL-M33's COALESCE unique.
    let other = Uuid::new_v4();
    assert!(svc.resolve("sales", Some(other)).await.unwrap().is_none());

    // Unknown local part → None (the caller's bounce/catchall decision).
    assert!(svc.resolve("nope", None).await.unwrap().is_none());

    sqlx::query("DELETE FROM messaging.mail_aliases WHERE id = ANY($1)")
        .bind(&[null_id, dom_id])
        .execute(&pool)
        .await
        .ok();
}

#[tokio::test]
async fn resolve_normalizes_case_and_rejects_empty() {
    let Some(pool) = common::test_pool().await else {
        common::skipped("alias normalization");
        return;
    };
    let svc = AliasWriteService::new(pool.clone());
    let id = seed_alias(&pool, "helpdesk", None).await;

    let got = svc.resolve("  HELPDESK ", None).await.unwrap().expect("case-insensitive + trimmed");
    assert_eq!(got.id, id);
    assert!(svc.resolve("", None).await.is_err());

    sqlx::query("DELETE FROM messaging.mail_aliases WHERE id = $1")
        .bind(id)
        .execute(&pool)
        .await
        .ok();
}
