//! Platform constants for the messaging outbox: the fence-none sentinel company id,
//! the server-side bus channel-key constructors, and the in-tx outbox stage helper.
//!
//! Hand-authored (user-owned; declared in `metaphor.codegen.yaml`). The port of Odoo's
//! `bus` role onto `backbone-outbox` per ADR-0017 — see `docs/adr-notes/channel-key-dispatch.md`
//! and `docs/adr-notes/outbox-fence-and-credentials.md` for the two binding decisions.

use uuid::Uuid;

/// ADR-0014 posture 4 (company_fence: none): messaging has no company dimension.
/// The outbox column is NOT NULL, so platform events carry the nil sentinel.
/// Consumers key on channels, not companies — never filter these rows by company.
///
/// This is a CONSTANT, not a config knob: a configurable "default company" would
/// quietly reintroduce the synthesized fence ADR-0014 forbids (see
/// `docs/adr-notes/outbox-fence-and-credentials.md` §a).
pub const MESSAGING_PLATFORM_COMPANY_ID: Uuid = Uuid::nil();

/// The outbox schema this module stages into (module/schema name: `messaging`).
pub const OUTBOX_SCHEMA: &str = "messaging";

// ============================================================================
// Channel keys (BUS-B2 / ADR-0017 decision 1)
// ============================================================================
//
// Odoo's `bus.bus.channel` was a db-scoped tuple `channel_with_db(dbname, target)`
// serialized as JSON. Under ONE database the db-scoping is obsolete, so the bare
// `"<model>_<id>"` record-channel key remains (`docs/adr-notes/channel-key-dispatch.md`).
//
// Confidentiality is channel-key CONSTRUCTION, not a constraint (BUS-B2): these
// constructors are the ONLY way channel keys come into existence — built server-side
// at stage time from server-known identity, never accepted from a client or consumer.
// There is no subscriber-writable dispatch table anywhere in the design.

/// The record channel for a chatter host: `"<res_model>_<res_id>"` — the port of
/// Odoo's `(model, id)` bus channel. Every host with chatter gets its message and
/// notification events addressed here.
pub fn record_channel(res_model: &str, res_id: Uuid) -> String {
    format!("{res_model}_{res_id}")
}

/// The per-partner channel: `res.partner_<id>`. Odoo's bus topology converges
/// user + partner channels onto the partner (`res.users._bus_channel` returns
/// `self.partner_id`) — preserved: a user's inbox events are addressed to their
/// partner's channel.
pub fn partner_channel(partner_id: Uuid) -> String {
    format!("res.partner_{partner_id}")
}

/// The Discuss channel stream: `discuss.channel_<id>` (MAIL-M37).
pub fn discuss_channel(channel_id: Uuid) -> String {
    format!("discuss.channel_{channel_id}")
}

/// The per-guest channel: `mail.guest_<id>` (MAIL-M43).
pub fn guest_channel(guest_id: Uuid) -> String {
    format!("mail.guest_{guest_id}")
}

// ============================================================================
// The payload envelope (bus.bus shape, verbatim)
// ============================================================================

/// Build the `bus.bus`-shaped payload envelope every bus-derived event carries:
///
/// ```json
/// { "channel": "<channel-key>", "message": { "type": "...", "payload": { ... } } }
/// ```
///
/// - `channel` is the fully-constructed channel key (from one of the constructors
///   above — callers must NOT pass client-supplied strings).
/// - `message` is the port of `bus.bus.message`: Odoo's own `{"type", "payload"}`
///   notification wrapper preserved as-is so the 8 bus-listener host contracts
///   translate without rewriting every consumer (ADR-0017 decision 1).
pub fn bus_envelope(
    channel: impl Into<String>,
    message_type: impl Into<String>,
    payload: serde_json::Value,
) -> serde_json::Value {
    serde_json::json!({
        "channel": channel.into(),
        "message": {
            "type": message_type.into(),
            "payload": payload,
        },
    })
}

// ============================================================================
// The in-tx stage helper
// ============================================================================

/// Stage a bus-derived event to the `messaging` outbox on the caller's OPEN
/// transaction, in-tx with the state change that produced it — so a crash between
/// the write and any downstream publish cannot drop the event (the durability rule
/// from the backbone-notification exemplar).
///
/// - `event_type` stays a NOMINAL type (`"MessagePosted"`, `"SmsCreated"`, ...) — it
///   says what kind of wire event this is; the channel says who it is addressed to.
///   Keeping them orthogonal preserves routing/filtering by type (ADR-0017).
/// - `company_id` on the row is ALWAYS [`MESSAGING_PLATFORM_COMPANY_ID`] — that is
///   the whole point of the sentinel, so callers cannot get it wrong.
pub async fn stage_bus_event(
    conn: &mut sqlx::PgConnection,
    event_type: &str,
    aggregate_type: &str,
    aggregate_id: impl std::fmt::Display,
    channel: String,
    message_type: &str,
    payload: serde_json::Value,
) -> Result<(), sqlx::Error> {
    let record = backbone_outbox::OutboxRecord::new(
        event_type,
        aggregate_type,
        aggregate_id.to_string(),
        MESSAGING_PLATFORM_COMPANY_ID,
        bus_envelope(channel, message_type, payload),
        chrono::Utc::now(),
    );
    backbone_outbox::outbox::stage(conn, OUTBOX_SCHEMA, &record)
        .await
        .map_err(|e| sqlx::Error::InvalidArgument(format!("outbox stage: {e}")))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn record_channel_is_model_underscore_id() {
        // The bare key, db-scoping dropped (one database — ADR-0017 note).
        assert_eq!(
            record_channel("crm.lead", Uuid::nil()),
            format!("crm.lead_{}", Uuid::nil())
        );
    }

    #[test]
    fn envelope_mirrors_bus_bus_two_column_shape() {
        let env = bus_envelope(
            partner_channel(Uuid::nil()),
            "mail.message/insert",
            serde_json::json!({"id": 1}),
        );
        assert_eq!(env["channel"], format!("res.partner_{}", Uuid::nil()));
        assert_eq!(env["message"]["type"], "mail.message/insert");
        assert_eq!(env["message"]["payload"]["id"], 1);
    }
}
