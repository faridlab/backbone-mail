//! The follower write service (hand-written; user-owned).
//!
//! The port of Odoo `mail.thread.message_subscribe` / `message_unsubscribe`
//! (MAIL-M10): the G-MAIL-1 SQL unique `(res_model, res_id, partner_id)` is the
//! floor (fires on raw SQL — ADR-0015); the `_insert_followers` existing-policy
//! (skip/force/replace/update) is SERVICE logic riding on top of it.

use uuid::Uuid;

use crate::domain::event::{record_channel, stage_bus_event};
use crate::infrastructure::persistence::follower_repository::{
    ExistingPolicy, FollowerRepository, SubscribeOutcome,
};

#[derive(Debug, thiserror::Error)]
pub enum FollowerError {
    #[error("db: {0}")]
    Db(#[from] sqlx::Error),
    #[error("invalid input: {0}")]
    Invalid(String),
}

pub struct FollowerWriteService {
    pool: sqlx::PgPool,
}

impl FollowerWriteService {
    pub fn new(pool: sqlx::PgPool) -> Self {
        Self { pool }
    }

    /// Subscribe a partner to a document under `policy`, staging the
    /// `FollowerSubscribed` bus event in the same transaction.
    pub async fn subscribe(
        &self,
        res_model: &str,
        res_id: Uuid,
        partner_id: Uuid,
        subtype_ids: Vec<Uuid>,
        policy: ExistingPolicy,
    ) -> Result<SubscribeOutcome, FollowerError> {
        if res_model.trim().is_empty() {
            return Err(FollowerError::Invalid("res_model is required".into()));
        }
        let mut tx = self.pool.begin().await?;
        let outcome = FollowerRepository::subscribe(&mut tx, res_model, res_id, partner_id, &subtype_ids, policy).await?;
        if outcome != SubscribeOutcome::Skipped {
            stage_bus_event(
                &mut tx,
                "FollowerSubscribed",
                "MailFollowers",
                format!("{res_model}/{res_id}/{partner_id}"),
                record_channel(res_model, res_id),
                "FollowerSubscribed",
                serde_json::json!({
                    "res_model": res_model, "res_id": res_id,
                    "partner_id": partner_id, "subtype_ids": subtype_ids,
                    "policy": format!("{policy:?}").to_lowercase(),
                }),
            )
            .await?;
        }
        tx.commit().await?;
        Ok(outcome)
    }

    /// Unsubscribe partners; stages `FollowerUnsubscribed` when rows were removed.
    pub async fn unsubscribe(
        &self,
        res_model: &str,
        res_id: Uuid,
        partner_ids: Vec<Uuid>,
    ) -> Result<u64, FollowerError> {
        if partner_ids.is_empty() {
            return Ok(0);
        }
        let mut tx = self.pool.begin().await?;
        let removed = FollowerRepository::unsubscribe(&mut tx, res_model, res_id, &partner_ids).await?;
        if removed > 0 {
            stage_bus_event(
                &mut tx,
                "FollowerUnsubscribed",
                "MailFollowers",
                format!("{res_model}/{res_id}"),
                record_channel(res_model, res_id),
                "FollowerUnsubscribed",
                serde_json::json!({
                    "res_model": res_model, "res_id": res_id, "partner_ids": partner_ids,
                }),
            )
            .await?;
        }
        tx.commit().await?;
        Ok(removed)
    }
}
