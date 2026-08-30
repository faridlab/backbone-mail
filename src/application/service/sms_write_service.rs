//! The sms write service (hand-written; user-owned).
//!
//! The port of Odoo `sms.sms` lifecycle (SM-M1/SM-M2 + TR-SM-1/SM-B6/MMB-4 fix):
//!
//! - **enqueue** — mint the `sms` row (`state='outgoing'`) + its uuid-correlated
//!   `sms_tracker`, and stage a `SmsCreated` bus event that RE-ARMS the queue
//!   drainer (TR-SM-1: every create force-wakes the dispatch job; the interval is
//!   the safety net).
//! - **process_queue** — the drainer: claim `state='outgoing'` rows with
//!   `FOR UPDATE SKIP LOCKED` (MMB-4 — concurrent workers drain disjoint sets),
//!   batch 500, commit per batch, advance each row via the [`SmsApiPort`].
//!
//! The notification pump (TR-SM-15): every outcome is mirrored onto the linked
//! `mail_notification` through `SMS_STATE_TO_NOTIFICATION_STATUS` — ONE map, the
//! single source for which advance is legal (SM-M1) — with the service-level rank
//! guard mirroring the SM-B6 DB trigger.
//!
//! Messaging owns the FULL sms lifecycle through the port; increment 1 ships only
//! the NoOp double (no real provider, no HTTP).

use uuid::Uuid;

use crate::application::service::sms_ports::{SmsApiPort, SmsSendRequest};
use crate::domain::event::{partner_channel, record_channel, stage_bus_event};
use crate::infrastructure::persistence::message_pipeline_repository::MessagePipelineRepository;
use crate::infrastructure::persistence::sms_queue_repository::SmsQueueRepository;

/// THE map (SM-M1, sms_tracker.py:22-29) — sms state → notification status. One
/// map, owned here, applied by the pump; the DB trigger enforces its monotonic
/// floor. Every value is the identity EXCEPT `error → exception` and
/// `outgoing → ready` (the enum-vocabulary split between the two tables).
pub fn sms_state_to_notification_status(sms_state: &str) -> Option<&'static str> {
    Some(match sms_state {
        "canceled" => "canceled",
        "process" => "process",
        "error" => "exception",
        "outgoing" => "ready",
        "sent" => "sent",
        "pending" => "pending",
        _ => return None,
    })
}

/// The default drain batch (Odoo `sms.session.batch.size`, default 500).
pub const DEFAULT_SMS_BATCH: i64 = 500;

#[derive(Debug, thiserror::Error)]
pub enum SmsError {
    #[error("db: {0}")]
    Db(#[from] sqlx::Error),
    #[error("invalid input: {0}")]
    Invalid(String),
}

/// What one drain pass did — the audit surface for the scheduler.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct DrainOutcome {
    pub claimed: usize,
    pub accepted: usize,
    pub delivered: usize,
    pub failed: usize,
    /// Rows whose outcome write was a no-op (already advanced — a replay).
    pub replayed: usize,
}

pub struct SmsWriteService {
    pool: sqlx::PgPool,
}

impl SmsWriteService {
    pub fn new(pool: sqlx::PgPool) -> Self {
        Self { pool }
    }

    /// Enqueue an sms for send (the C1 path — numbers already sanitized by the
    /// caller; sanitization itself is the composer's SM §7 concern). Stages
    /// `SmsCreated` IN the enqueue transaction — the queue re-arm is durable with
    /// the row it arms for (TR-SM-1).
    pub async fn enqueue(
        &self,
        number: &str,
        body: &str,
        mail_message_id: Option<Uuid>,
        notification_id: Option<Uuid>,
        model: Option<&str>,
        res_id: Option<Uuid>,
    ) -> Result<(Uuid, String), SmsError> {
        if number.trim().is_empty() {
            return Err(SmsError::Invalid("number is required".into()));
        }
        if body.trim().is_empty() {
            return Err(SmsError::Invalid("body is required".into()));
        }
        let id = Uuid::new_v4();
        let sms_uuid = Uuid::new_v4().simple().to_string();

        let mut tx = self.pool.begin().await?;
        SmsQueueRepository::enqueue(&mut tx, id, &sms_uuid, number, body, mail_message_id, notification_id).await?;
        let channel_key = match (model, res_id) {
            (Some(m), Some(r)) => record_channel(m, r),
            _ => format!("mail.message_{}", mail_message_id.map(|m| m.to_string()).unwrap_or_default()),
        };
        stage_bus_event(
            &mut tx,
            "SmsCreated",
            "Sms",
            id,
            channel_key,
            "SmsCreated",
            serde_json::json!({ "sms_id": id, "uuid": sms_uuid, "number": number }),
        )
        .await?;
        tx.commit().await?;
        Ok((id, sms_uuid))
    }

    /// The queue drainer (`_process_queue`, SJ-SM-1 — with the MMB-4 SKIP LOCKED
    /// pickup): claim a batch (one tx, committed before any provider call —
    /// commit_per_batch), send each through the port, then apply each outcome in
    /// its own short transaction (state advance + tracker mirror + notification
    /// pump + `NotificationStatusChanged` staged in-tx). Loops until the queue
    /// drains or `max_batches` passes (0 = unlimited).
    ///
    /// A hard error from the port leaves rows claimed in `'process'` — the SM-B13
    /// orphan posture, registered for the increment-2 stuck-in-process sweep.
    pub async fn process_queue(
        &self,
        port: &dyn SmsApiPort,
        batch: i64,
        max_batches: usize,
    ) -> Result<DrainOutcome, SmsError> {
        let mut total = DrainOutcome::default();
        let mut passes = 0usize;
        loop {
            // 1. Claim (MMB-4): outgoing → process, FOR UPDATE SKIP LOCKED, one tx.
            let claimed = {
                let mut tx = self.pool.begin().await?;
                let rows = SmsQueueRepository::claim_batch_for_drain(&mut tx, batch).await?;
                tx.commit().await?;
                rows
            };
            if claimed.is_empty() {
                break;
            }
            total.claimed += claimed.len();

            // 2. Send + apply each outcome in its own unit (commit per row: one
            //    poisoned number cannot roll back the batch's good sends).
            for row in &claimed {
                let result = port
                    .send(&SmsSendRequest {
                        uuid: row.uuid.clone(),
                        number: row.number.clone(),
                        body: row.body.clone(),
                    })
                    .await;
                let mut tx = self.pool.begin().await?;
                let applied = self.apply_outcome_in_tx(&mut tx, row.id, &row.uuid, row.mail_message_id, result).await?;
                tx.commit().await?;
                match applied {
                    OutcomeApplied::Accepted => total.accepted += 1,
                    OutcomeApplied::Delivered => total.delivered += 1,
                    OutcomeApplied::Failed => total.failed += 1,
                    OutcomeApplied::Replay => total.replayed += 1,
                }
            }

            passes += 1;
            if max_batches != 0 && passes >= max_batches {
                break;
            }
        }
        Ok(total)
    }

    /// Apply a provider verdict to ONE row: advance the sms state (state-guarded on
    /// `'process'`), mirror the tracker, run the notification pump through the map,
    /// and stage `NotificationStatusChanged` — all in-tx.
    async fn apply_outcome_in_tx(
        &self,
        tx: &mut sqlx::PgConnection,
        sms_id: Uuid,
        sms_uuid: &str,
        mail_message_id: Option<Uuid>,
        result: Result<crate::application::service::sms_ports::SmsSendOutcome, crate::application::service::sms_ports::SmsSendFailure>,
    ) -> Result<OutcomeApplied, SmsError> {
        use crate::application::service::sms_ports::SmsSendOutcome;

        let (state, failure_type, error_message, iap_code, applied_when_new) = match &result {
            // IAP_TO_SMS_STATE_SUCCESS (sms_sms.py:20-26): processing→process,
            // success/sent→pending ('Sent'), delivered→sent ('Delivered' —
            // webhook-only in Odoo; the port may surface it synchronously).
            Ok(SmsSendOutcome::Processing) => ("process", None, None, None, OutcomeApplied::Replay),
            Ok(SmsSendOutcome::Accepted) => ("pending", None, None, None, OutcomeApplied::Accepted),
            Ok(SmsSendOutcome::Delivered) => ("sent", None, None, None, OutcomeApplied::Delivered),
            Err(f) => (
                "error",
                Some(f.failure_type.clone()),
                Some(f.message.clone()),
                f.iap_status_code,
                OutcomeApplied::Failed,
            ),
        };

        // 1. The sms row: state-guarded on 'process' (a replayed result is a no-op).
        let advanced = SmsQueueRepository::apply_outcome(
            tx, sms_id, state, failure_type.as_deref(), error_message.as_deref(), iap_code)
            .await?;

        // 2. The tracker mirror + the notification pump (TR-SM-15). The tracker's
        //    state column is a mail_notification_status — the sms vocabulary's
        //    'error' does not exist there ('exception' does), so BOTH consumers go
        //    through the one map.
        let mapped_status = sms_state_to_notification_status(state);
        SmsQueueRepository::mirror_tracker_state(tx, sms_uuid, mapped_status.unwrap_or(state)).await?;
        if let Some(notification_status) = mapped_status {
            if let Some(nid) = MessagePipelineRepository::find_notification_id_by_sms_uuid(tx, sms_uuid).await? {
                let advanced_status = MessagePipelineRepository::advance_notification_status(
                    tx,
                    nid,
                    notification_status,
                    failure_type.as_deref(),
                    error_message.as_deref(),
                )
                .await?;
                if let Some(new_status) = advanced_status {
                    // Stage NotificationStatusChanged on the RECIPIENT's channel
                    // (the todo counter / failure badge consumer's stream).
                    let (partner, _) =
                        MessagePipelineRepository::notification_status(tx, nid).await?.unwrap_or((None, new_status.clone()));
                    let channel_key = partner
                        .map(partner_channel)
                        .unwrap_or_else(|| format!("mail.message_{}", mail_message_id.unwrap_or(Uuid::nil())));
                    stage_bus_event(
                        tx,
                        "NotificationStatusChanged",
                        "MailNotification",
                        nid,
                        channel_key,
                        "NotificationStatusChanged",
                        serde_json::json!({
                            "notification_id": nid, "sms_uuid": sms_uuid,
                            "notification_status": new_status, "sms_state": state,
                        }),
                    )
                    .await?;
                }
            }
        }

        Ok(if advanced { applied_when_new } else { OutcomeApplied::Replay })
    }

    /// Advance an sms's state from OUTSIDE the drainer (the webhook / late-verdict
    /// seam's entry). Legal sources are `process` and `pending`: a verdict that
    /// races the drainer's own outcome lands from `'process'` exactly as before,
    /// while the DELIVERY REPORT (`pending → sent`, the provider's second
    /// callback) is the transition this seam exists for. An `'outgoing'` row has
    /// not been dispatched — no external verdict can be true for it yet — and a
    /// regression or a re-assertion matches zero rows (`Replay`).
    ///
    /// Runs the same pump as the drainer so notification status, tracker, and
    /// the bus event stay in lockstep; for rows with no linked notification the
    /// TRACKER mirror is the durable delivery fact (the mass-mailing SMS
    /// overlay's pump reads it from the other module — cross-schema, read-only).
    pub async fn advance_state(
        &self,
        sms_uuid: &str,
        target_state: &str,
        failure_type: Option<&str>,
        error_message: Option<&str>,
    ) -> Result<bool, SmsError> {
        let mut tx = self.pool.begin().await?;
        let id = sqlx::query_scalar::<_, Uuid>(
            r#"SELECT id FROM messaging.sms WHERE uuid = $1"#,
        )
        .bind(sms_uuid)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or_else(|| SmsError::Invalid(format!("no sms with uuid {sms_uuid}")))?;

        let advanced = SmsQueueRepository::apply_outcome_from(
            &mut tx, id, target_state, failure_type, error_message, None,
            &["process", "pending"])
            .await?;
        if advanced {
            let mapped = sms_state_to_notification_status(target_state);
            SmsQueueRepository::mirror_tracker_state(&mut tx, sms_uuid, mapped.unwrap_or(target_state)).await?;
            if let Some(status) = mapped {
                if let Some(nid) =
                    MessagePipelineRepository::find_notification_id_by_sms_uuid(&mut tx, sms_uuid).await?
                {
                    MessagePipelineRepository::advance_notification_status(
                        &mut tx, nid, status, failure_type, error_message,
                    )
                    .await?;
                }
            }
        }
        tx.commit().await?;
        Ok(advanced)
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum OutcomeApplied {
    Accepted,
    Delivered,
    Failed,
    Replay,
}
