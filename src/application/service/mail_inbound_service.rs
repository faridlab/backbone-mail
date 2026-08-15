//! The inbound-email webhook pipeline (hand-written; user-owned) — MAIL-M27's
//! webhook mode, the MAIL-M28 allowlist, and the MAIL-B2/B6 fixes.
//!
//! Shape (owner-locked decision, port-notes §8): an upstream relay
//! (SES/SendGrid-style inbound parse) POSTs one parsed message to
//! `POST /mail/inbound/:server_id` with a per-server bearer token. The route
//! delegates here. Ordering is the whole design, and it is fail-closed:
//!
//!   ① AUTH, zero writes — hash the presented token (SHA-256), consteq against
//!      the server row's `token_hash`. Unknown server, inactive server, or bad
//!      token all fail the SAME way (no server-existence oracle); nothing has
//!      been written at that point (the route's snapshot test proves it).
//!   ② ONE TRANSACTION PER MESSAGE (MAIL-B2 — Odoo's fetchmail commits the
//!      whole batch in one tx, so one poisoned message loses its whole batch;
//!      the port processes one message per tx, and a webhook relay retries per
//!      message, which the dedup makes safe):
//!        a. dedup probe on RFC message_id (MAIL-B6; the UNIQUE partial index
//!           backstops races — a 23505 on insert folds into `Duplicate`);
//!        b. allowlist match on the envelope sender (MAIL-M28 — the gateway's
//!           post-permission; external senders have no MessagingIdentity, so
//!           the MAIL-B1 ThreadAccessResolver seam does NOT apply to gateway
//!           posts — it guards HTTP chatter reads/writes);
//!        c. route: In-Reply-To → parent thread's (model, res_id); else the
//!           `to` address resolved through MailAlias/MailAliasDomain (M33);
//!           else the server's default_thread_model; else drop-with-event;
//!        d. insert the mail.message (type `email`), stage
//!           `InboundEmailRouted` on the thread's record channel, touch the
//!           server bookkeeping — all IN the tx.
//!
//! Everything that refuses the message (rejected sender, unroutable) still
//! commits its `InboundEmailRejected`/`InboundEmailDropped` event — the relay
//! gets a durable 2xx-verdict trail, never a silent black hole.

use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::application::service::mail_server_query_service::split_address;
use crate::domain::event::{record_channel, stage_bus_event};
use crate::infrastructure::persistence::alias_resolution_repository::AliasResolutionRepository;
use crate::infrastructure::persistence::inbound_repository::InboundRepository;
use crate::infrastructure::persistence::message_pipeline_repository::{
    MessagePipelineRepository, NewMailMessageRow,
};

/// The ops channel inbound verdicts ride (built here, never accepted from a wire).
const GATEWAY_OPS_CHANNEL: &str = "mail.gateway_ops";

#[derive(Debug, thiserror::Error)]
pub enum MailInboundError {
    /// Auth failed — unknown server, inactive server, or bad token. Deliberately
    /// ONE variant: the caller must answer all three identically (401).
    #[error("inbound auth failed")]
    Auth,
    #[error("invalid payload: {0}")]
    Invalid(String),
    #[error("db: {0}")]
    Db(#[from] sqlx::Error),
}

/// One parsed inbound message (the relay's JSON body — the route deserializes
/// it BEFORE calling, so an unparseable body never reaches a tx).
#[derive(Debug, Clone, serde::Deserialize)]
pub struct InboundMessage {
    /// The RFC `Message-ID` — the MAIL-B6 dedup key. Absent on malformed relay
    /// payloads; the message is then accepted un-deduped (NULLs are outside
    /// the partial unique index by design).
    pub message_id: Option<String>,
    /// Envelope sender — the allowlist matches on this.
    pub from: String,
    /// Envelope recipients — alias routing walks them in order.
    pub to: Vec<String>,
    pub subject: Option<String>,
    /// The relayed html body.
    pub body_html: String,
    /// RFC `In-Reply-To` — reply collation's parent key.
    pub in_reply_to: Option<String>,
}

/// What one inbound message did.
#[derive(Debug, Clone, PartialEq)]
pub enum InboundOutcome {
    /// Routed and stored: the new mail.message id + where it landed.
    Routed {
        message_row: Uuid,
        model: Option<String>,
        res_id: Option<Uuid>,
    },
    /// A replay of an already-stored message_id — idempotent success (the
    /// relay's retry contract). Carries the existing row.
    Duplicate { existing: Uuid },
    /// Envelope sender not on the allowlist (MAIL-M28). No message row; the
    /// rejection event IS committed.
    Rejected,
    /// Allowed but unroutable (no parent thread, no alias, no default model).
    /// No message row; the drop event IS committed.
    Dropped,
}

pub struct MailInboundService {
    pool: sqlx::PgPool,
}

impl MailInboundService {
    pub fn new(pool: sqlx::PgPool) -> Self {
        Self { pool }
    }

    /// Process one inbound message (the whole pipeline — see the module doc for
    /// the fail-closed ordering).
    pub async fn process_inbound(
        &self,
        server_id: Uuid,
        token: &str,
        msg: &InboundMessage,
    ) -> Result<InboundOutcome, MailInboundError> {
        if msg.from.trim().is_empty() || msg.to.is_empty() {
            return Err(MailInboundError::Invalid("from and to are required".into()));
        }

        // ---- ① AUTH. NOTHING below this block runs before it passes, and the
        // probe is read-only — zero writes before auth. ----
        let default_thread_model = {
            let conn = &mut self.pool.acquire().await?;
            let server = InboundRepository::find_server(conn, server_id)
                .await?
                .ok_or(MailInboundError::Auth)?;
            if !server.active {
                return Err(MailInboundError::Auth);
            }
            let presented = sha256_hex(token);
            if !const_eq(&presented, &server.token_hash.to_ascii_lowercase()) {
                return Err(MailInboundError::Auth);
            }
            // `state` (draft/done) documents relay verification — it does not
            // gate posts; `active` is the gate. The default model is carried
            // into the tx (no second server read inside it).
            server.default_thread_model
        };

        // ---- ② ONE TX PER MESSAGE (MAIL-B2). ----
        let mut tx = self.pool.begin().await?;
        let outcome = self
            .process_in_tx(&mut tx, server_id, default_thread_model.as_deref(), msg)
            .await?;
        tx.commit().await?;
        Ok(outcome)
    }

    /// Steps ②a–②d on the caller's transaction. A 23505 on the message insert
    /// is the MAIL-B6 backstop firing (a concurrent relay retry won the race)
    /// and folds into `Duplicate`.
    async fn process_in_tx(
        &self,
        tx: &mut sqlx::PgConnection,
        server_id: Uuid,
        default_thread_model: Option<&str>,
        msg: &InboundMessage,
    ) -> Result<InboundOutcome, MailInboundError> {
        // ②a dedup probe (the index backstops the race window after this read).
        if let Some(mid) = &msg.message_id {
            if let Some(existing) = InboundRepository::find_by_message_id(tx, mid).await? {
                return Ok(InboundOutcome::Duplicate { existing });
            }
        }

        // ②b allowlist (MAIL-M28). The rejection event commits (durability),
        // the message row never exists.
        if !InboundRepository::sender_allowed(tx, server_id, &msg.from).await? {
            stage_bus_event(
                tx,
                "InboundEmailRejected",
                "FetchmailServer",
                server_id,
                GATEWAY_OPS_CHANNEL.to_string(),
                "InboundEmailRejected",
                serde_json::json!({
                    "server_id": server_id,
                    "email_from": msg.from,
                    "message_id": msg.message_id,
                }),
            )
            .await?;
            return Ok(InboundOutcome::Rejected);
        }

        // ②c routing.
        let (model, res_id) = self.route(tx, default_thread_model, msg).await?;
        let Some(model) = model else {
            // Unroutable — drop WITH an event (never a silent black hole).
            stage_bus_event(
                tx,
                "InboundEmailDropped",
                "FetchmailServer",
                server_id,
                GATEWAY_OPS_CHANNEL.to_string(),
                "InboundEmailDropped",
                serde_json::json!({
                    "server_id": server_id,
                    "email_from": msg.from,
                    "to": msg.to,
                    "reason": "no_parent_no_alias_no_default_model",
                }),
            )
            .await?;
            return Ok(InboundOutcome::Dropped);
        };

        // ②d the message row + the routed event + bookkeeping, all in-tx.
        let id = Uuid::new_v4();
        // 23505 on the insert = the MAIL-B6 partial index won the race: a
        // concurrent relay retry inserted the same message_id between our probe
        // and this insert. Fold it into Duplicate below (re-probe the winner).
        let mut race_lost = false;
        MessagePipelineRepository::insert_mail_message(
            tx,
            &NewMailMessageRow {
                id,
                subject: msg.subject.as_deref(),
                body: &msg.body_html,
                message_type: "email",
                subtype_id: None,
                is_internal: false,
                author_id: None,
                author_guest_id: None,
                email_from: Some(&msg.from),
                message_id: msg.message_id.as_deref(),
                reply_to: None,
                model: Some(&model),
                res_id,
                record_name: None,
            },
        )
        .await
        .or_else(|e| match e {
            sqlx::Error::Database(ref db)
                if db.code().map(|c| c.into_owned()) == Some("23505".into())
                    && msg.message_id.is_some() =>
            {
                race_lost = true;
                Ok(())
            }
            other => Err(other),
        })?;
        if race_lost {
            let mid = msg.message_id.as_deref().unwrap_or_default();
            let existing = InboundRepository::find_by_message_id(tx, mid).await?.unwrap_or(id);
            return Ok(InboundOutcome::Duplicate { existing });
        }

        let channel_key = match res_id {
            Some(rid) => record_channel(&model, rid),
            // Record-less landing (default-model rung): the gateway ops trail
            // is the only audience — there is no record channel to address.
            None => GATEWAY_OPS_CHANNEL.to_string(),
        };
        stage_bus_event(
            tx,
            "InboundEmailRouted",
            "MailMessage",
            id,
            channel_key,
            "InboundEmailRouted",
            serde_json::json!({
                "message_row": id,
                "model": model,
                "res_id": res_id,
                "email_from": msg.from,
                "server_id": server_id,
            }),
        )
        .await?;
        InboundRepository::touch_server_success(tx, server_id, chrono::Utc::now()).await?;
        Ok(InboundOutcome::Routed { message_row: id, model: Some(model), res_id })
    }

    /// The routing ladder: parent thread → alias → server default. Returns the
    /// `(model, res_id)` pair; `model: None` means unroutable (the caller drops
    /// with an event). `res_id` may be None only on the default-model rung (a
    /// model-attached but record-less message — Odoo's default_thread_model
    /// says WHERE records go, and record creation is host logic, not gateway
    /// logic).
    async fn route(
        &self,
        tx: &mut sqlx::PgConnection,
        default_thread_model: Option<&str>,
        msg: &InboundMessage,
    ) -> Result<(Option<String>, Option<Uuid>), MailInboundError> {
        // Rung 1 — reply collation: the parent's thread is the thread.
        if let Some(parent_id) = &msg.in_reply_to {
            if let Some((model, res_id)) =
                InboundRepository::find_parent_thread(tx, parent_id).await?
            {
                return Ok((model, res_id));
            }
        }

        // Rung 2 — alias routing (M33): local@domain → alias → forced/parent
        // thread. The model STRING is learned by collation from an existing
        // message on that thread (the alias registry refs carry no names).
        for addr in &msg.to {
            let Some((local, domain)) = split_address(addr) else { continue };
            let Some(domain_id) = InboundRepository::find_domain_id_by_name(tx, &domain).await?
            else {
                continue;
            };
            let Some(alias) =
                AliasResolutionRepository::resolve(tx, &local, Some(domain_id)).await?
            else {
                continue;
            };
            let target =
                alias.alias_force_thread_id.or(alias.alias_parent_thread_id);
            if let Some(target) = target {
                // Collation first; a brand-new thread (no message on it yet)
                // falls back to the server's default model as the only
                // remaining string source. Still no model → keep walking.
                let model = InboundRepository::learn_thread_model(tx, target)
                    .await?
                    .or_else(|| default_thread_model.map(str::to_string));
                if let Some(model) = model {
                    return Ok((Some(model), Some(target)));
                }
            }
        }

        // Rung 3 — the server's default thread model (record-less attach).
        Ok((default_thread_model.map(str::to_string), None))
    }
}

/// SHA-256 hex of the presented token (the row stores the same hex).
fn sha256_hex(input: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(input.as_bytes());
    let digest = hasher.finalize();
    let mut out = String::with_capacity(64);
    for byte in digest {
        use std::fmt::Write as _;
        let _ = write!(out, "{byte:02x}");
    }
    out
}

/// Constant-time equality (no early-exit byte leak on the token).
fn const_eq(a: &str, b: &str) -> bool {
    if a.len() != b.len() {
        return false;
    }
    a.bytes().zip(b.bytes()).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

#[cfg(test)]
mod tests {
    use super::{const_eq, sha256_hex};

    #[test]
    fn sha256_hex_matches_known_vector() {
        // sha256("token") via `printf 'token' | shasum -a 256`.
        assert_eq!(
            sha256_hex("token"),
            "3c469e9d6c5875d37a43f353d4f88e61fcf812c66eee3457465a40b0da4153e0"
        );
    }

    #[test]
    fn const_eq_is_length_safe() {
        assert!(const_eq("abc", "abc"));
        assert!(!const_eq("abc", "abd"));
        assert!(!const_eq("abc", "abcd"));
    }
}
