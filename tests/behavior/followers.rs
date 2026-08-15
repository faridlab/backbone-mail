//! Follower policies (MAIL-M10): the G-MAIL-1 upsert is the floor, the
//! skip/force/replace/update existing-policy is service logic on top.

use backbone_mail::application::service::{
    ExistingPolicy, FollowerWriteService, SubscribeOutcome,
};
use sqlx::Row;
use uuid::Uuid;

use super::common;

async fn follower_subtypes(pool: &sqlx::PgPool, model: &str, res_id: Uuid, partner: Uuid) -> Vec<Uuid> {
    let row = sqlx::query(
        "SELECT subtype_ids FROM messaging.mail_followers WHERE res_model = $1 AND res_id = $2 AND partner_id = $3",
    )
    .bind(model)
    .bind(res_id)
    .bind(partner)
    .fetch_one(pool)
    .await
    .expect("follower row");
    let json: serde_json::Value = row.get("subtype_ids");
    json.as_array()
        .map(|a| a.iter().filter_map(|v| v.as_str().and_then(|s| Uuid::parse_str(s).ok())).collect())
        .unwrap_or_default()
}

#[tokio::test]
async fn subscribe_policies_skip_force_replace_update() {
    let Some(pool) = common::test_pool().await else {
        common::skipped("follower policies");
        return;
    };
    let svc = FollowerWriteService::new(pool.clone());
    let model = format!("test.doc.{}", Uuid::new_v4().simple());
    let res_id = Uuid::new_v4();
    let partner = Uuid::new_v4();
    let s1 = Uuid::new_v4();
    let s2 = Uuid::new_v4();
    let s3 = Uuid::new_v4();

    // Insert.
    assert_eq!(
        svc.subscribe(&model, res_id, partner, vec![s1], ExistingPolicy::Skip).await.unwrap(),
        SubscribeOutcome::Inserted
    );
    assert_eq!(follower_subtypes(&pool, &model, res_id, partner).await, vec![s1]);

    // Skip: existing row wins.
    assert_eq!(
        svc.subscribe(&model, res_id, partner, vec![s2], ExistingPolicy::Skip).await.unwrap(),
        SubscribeOutcome::Skipped
    );
    assert_eq!(follower_subtypes(&pool, &model, res_id, partner).await, vec![s1]);

    // Update: union, deduped.
    assert_eq!(
        svc.subscribe(&model, res_id, partner, vec![s2, s1], ExistingPolicy::Update).await.unwrap(),
        SubscribeOutcome::Updated
    );
    let mut merged = follower_subtypes(&pool, &model, res_id, partner).await;
    merged.sort();
    let mut want = vec![s1, s2];
    want.sort();
    assert_eq!(merged, want, "Update policy unions + dedups");

    // Replace: overwrite wholesale.
    assert_eq!(
        svc.subscribe(&model, res_id, partner, vec![s3], ExistingPolicy::Replace).await.unwrap(),
        SubscribeOutcome::Updated
    );
    assert_eq!(follower_subtypes(&pool, &model, res_id, partner).await, vec![s3]);

    // Unsubscribe.
    assert_eq!(svc.unsubscribe(&model, res_id, vec![partner]).await.unwrap(), 1);
    assert_eq!(svc.unsubscribe(&model, res_id, vec![partner]).await.unwrap(), 0, "idempotent");

    sqlx::query("DELETE FROM messaging.outbox_events WHERE payload->>'res_model' = $1")
        .bind(&model)
        .execute(&pool)
        .await
        .ok();
}

#[tokio::test]
async fn concurrent_subscribe_is_unique_guaranteed_by_sql() {
    let Some(pool) = common::test_pool().await else {
        common::skipped("follower G-MAIL-1 uniqueness");
        return;
    };
    let svc = FollowerWriteService::new(pool.clone());
    let model = format!("test.doc.{}", Uuid::new_v4().simple());
    let res_id = Uuid::new_v4();
    let partner = Uuid::new_v4();

    // Two racing subscribes: G-MAIL-1 (ON CONFLICT DO NOTHING) keeps exactly one row.
    let a = svc.subscribe(&model, res_id, partner, vec![], ExistingPolicy::Skip);
    let b = svc.subscribe(&model, res_id, partner, vec![], ExistingPolicy::Skip);
    let (ra, rb) = tokio::join!(a, b);
    let outcomes = [ra.unwrap(), rb.unwrap()];
    assert_eq!(
        outcomes.iter().filter(|o| **o == SubscribeOutcome::Inserted).count(),
        1,
        "exactly one insert wins: {outcomes:?}"
    );

    let count: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM messaging.mail_followers WHERE res_model = $1 AND res_id = $2 AND partner_id = $3",
    )
    .bind(&model)
    .bind(res_id)
    .bind(partner)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(count, 1, "the SQL unique is the floor (fires on raw SQL too)");

    sqlx::query("DELETE FROM messaging.mail_followers WHERE res_model = $1")
        .bind(&model)
        .execute(&pool)
        .await
        .ok();
    sqlx::query("DELETE FROM messaging.outbox_events WHERE payload->>'res_model' = $1")
        .bind(&model)
        .execute(&pool)
        .await
        .ok();
}
