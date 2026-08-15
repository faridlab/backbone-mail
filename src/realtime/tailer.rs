//! The read-only outbox tailer (hand-written; user-owned).
//!
//! Each app replica runs ONE of these (spawned by the host app, Stage 5).
//! Every `poll_ms` it re-reads the outbox rows inside the replay window and
//! publishes the ones it hasn't seen to the local [`RealtimeRegistry`]. It
//! NEVER writes: `published_at` stays untouched (the relay is the carrier of
//! record for durable consumers — ADR-0017; the tailer is a replica-local
//! fan-out convenience, so a crashed replica loses nothing but its own
//! in-memory ring).
//!
//! Dedup is in-process by outbox id, pruned by the window: the query window
//! and the dedup horizon are the SAME `window_seconds` constant, so an id
//! drops out of the map at the same horizon the row leaves the query —
//! no re-delivery, no leak.
//!
//! The window deliberately OVERLAPS (each poll re-reads ~55s of rows) rather
//! than tracking a high-watermark: a watermark after a poll gap (GC pause,
//! replica stall) would silently skip rows, while overlap + dedup cannot.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use sqlx::Row;
use uuid::Uuid;

use super::registry::{RealtimeRegistry, TailedEvent};

/// Tailer tuning (mirrors `config/application.yml` `realtime:`).
#[derive(Debug, Clone, Copy)]
pub struct TailerConfig {
    /// The replay/dedup window in seconds (55 — Odoo's ~50s catch-up analog).
    pub window_seconds: i64,
    /// Poll interval.
    pub poll_ms: u64,
}

impl Default for TailerConfig {
    fn default() -> Self {
        Self { window_seconds: 55, poll_ms: 1000 }
    }
}

/// BUS-B2: the channel-key grammar every event must satisfy before it
/// reaches the registry. Record keys are built server-side
/// (`"<model>_<uuid>"` — models may contain dots, ids are uuids), so the
/// alphabet is `[A-Za-z0-9_.-]`. Anything else on an outbox row means a
/// producer bypassed the constructors — dropped with a warn, never pushed.
pub fn valid_channel_key(channel: &str) -> bool {
    !channel.is_empty()
        && channel.len() <= 128
        && channel.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'.' || b == b'-')
}

/// Split a record-shaped key `"<model>_<uuid>"` into its parts. Returns
/// `None` for non-record shapes (partner/guest/discuss keys parse fine —
/// `res.partner` and `mail.guest` ARE models — the caller decides what to do
/// with the model name). The split is on the LAST underscore so model names
/// containing underscores still parse.
pub fn parse_record_key(channel: &str) -> Option<(&str, Uuid)> {
    let (model, id) = channel.rsplit_once('_')?;
    let id = Uuid::parse_str(id).ok()?;
    Some((model, id))
}

/// Extract the tailer's view from an outbox payload (the `bus_envelope`
/// shape). `None` = not a bus-shaped payload or a channel-key violation —
/// the caller drops it.
pub fn extract_event(id: Uuid, occurred_at: chrono::DateTime<chrono::Utc>, payload: &serde_json::Value) -> Option<TailedEvent> {
    let channel = payload.get("channel")?.as_str()?;
    if !valid_channel_key(channel) {
        tracing::warn!(outbox_id = %id, channel = %channel, "BUS-B2: dropping outbox event with invalid channel key");
        return None;
    }
    let message = payload.get("message")?;
    let message_type = message.get("type")?.as_str()?.to_string();
    let inner = message.get("payload")?.clone();
    Some(TailedEvent {
        id,
        channel: channel.to_string(),
        message_type,
        payload: inner,
        occurred_at,
    })
}

/// One poll pass, factored out for tests: read the window, dedup, publish.
/// Returns the number of events published this pass.
pub async fn poll_once(
    pool: &sqlx::PgPool,
    registry: &RealtimeRegistry,
    seen: &mut HashMap<Uuid, chrono::DateTime<chrono::Utc>>,
    window_seconds: i64,
) -> Result<usize, sqlx::Error> {
    // Prune the dedup map to the same horizon the query reads — entries
    // older than the window can never be re-fetched.
    let horizon = chrono::Utc::now() - chrono::Duration::seconds(window_seconds);
    seen.retain(|_, ts| *ts > horizon);

    let rows = sqlx::query(
        r#"
        SELECT id, occurred_at, payload
        FROM messaging.outbox_events
        WHERE occurred_at > now() - make_interval(secs => $1)
        ORDER BY occurred_at, id
        "#,
    )
    .bind(window_seconds)
    .fetch_all(pool)
    .await?;

    let mut published = 0;
    for row in rows {
        let id: Uuid = row.get("id");
        let occurred_at: chrono::DateTime<chrono::Utc> = row.get("occurred_at");
        if seen.contains_key(&id) {
            continue;
        }
        seen.insert(id, occurred_at);
        if let Some(event) = extract_event(id, occurred_at, &row.get::<serde_json::Value, _>("payload")) {
            registry.publish(event);
            published += 1;
        }
    }
    Ok(published)
}

/// Spawn the replica's tailer task. The returned [`tokio::task::JoinHandle`]
/// is the shutdown lever — abort it (or let runtime drop) on SIGTERM.
pub fn spawn(pool: sqlx::PgPool, registry: Arc<RealtimeRegistry>, config: TailerConfig) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let mut seen: HashMap<Uuid, chrono::DateTime<chrono::Utc>> = HashMap::new();
        let interval = Duration::from_millis(config.poll_ms.max(50));
        tracing::info!(target: "mail::realtime_tailer", window_seconds = config.window_seconds, poll_ms = config.poll_ms, "outbox tailer started");
        loop {
            match poll_once(&pool, &registry, &mut seen, config.window_seconds).await {
                Ok(_) => {}
                Err(e) => {
                    // Read-side failure only: log and keep the cadence. The
                    // ring goes stale, clients reconnect with their
                    // watermark (or clamp) — never a silent skip.
                    tracing::error!(target: "mail::realtime_tailer", error = %e, "outbox poll failed");
                }
            }
            tokio::time::sleep(interval).await;
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bus_b2_channel_key_grammar() {
        for good in [
            "res.partner_00000000-0000-0000-0000-000000000000",
            "discuss.channel_00000000-0000-0000-0000-000000000000",
            "crm.lead_00000000-0000-0000-0000-000000000000",
            "mail.guest_00000000-0000-0000-0000-000000000000",
            "simple-key.name_1",
        ] {
            assert!(valid_channel_key(good), "{good}");
        }
        for bad in ["", "has space_x", "wild*card", "sla/sh", "unicode_é"] {
            assert!(!valid_channel_key(bad), "{bad}");
        }
    }

    #[test]
    fn record_key_parses_on_last_underscore() {
        let id = Uuid::new_v4();
        let record_key = format!("crm.lead_{id}");
        let (model, parsed) = parse_record_key(&record_key).unwrap();
        assert_eq!(model, "crm.lead");
        assert_eq!(parsed, id);

        // Non-record shapes still parse as (model, id) — res.partner IS a
        // model; the ACL layer decides what that means.
        let partner_key = format!("res.partner_{id}");
        let (model, _) = parse_record_key(&partner_key).unwrap();
        assert_eq!(model, "res.partner");

        // No uuid tail → not a record key.
        assert!(parse_record_key("discuss.channel_general").is_none());
    }

    #[test]
    fn extract_drops_invalid_channel_and_non_bus_shapes() {
        let id = Uuid::new_v4();
        let now = chrono::Utc::now();

        let good = serde_json::json!({
            "channel": format!("res.partner_{id}"),
            "message": { "type": "mail.message/insert", "payload": { "id": 1 } }
        });
        let ev = extract_event(id, now, &good).unwrap();
        assert_eq!(ev.message_type, "mail.message/insert");
        assert_eq!(ev.payload["id"], 1);

        // Channel key violation → dropped (None), not forwarded.
        let bad_key = serde_json::json!({
            "channel": "evil key",
            "message": { "type": "t", "payload": {} }
        });
        assert!(extract_event(id, now, &bad_key).is_none());

        // Not bus-shaped → dropped.
        assert!(extract_event(id, now, &serde_json::json!({ "nope": 1 })).is_none());
        assert!(extract_event(
            id,
            now,
            &serde_json::json!({ "channel": format!("res.partner_{id}"), "message": { "payload": {} } })
        )
        .is_none());
    }
}
