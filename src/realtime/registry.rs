//! The realtime broadcast registry (hand-written; user-owned).
//!
//! One process-local fan-out point between the outbox tailer and the SSE
//! streams. The tailer pushes every in-window outbox event here; each
//! connection subscribes and applies its OWN allowlist (BUS-B2) before a
//! frame reaches the wire — see [`super::sse`] for why per-connection
//! filtering (rather than per-key routing) is the correct first shape:
//! record-shaped channels are only decidable per-identity through the
//! MAIL-B1 resolver, so a key-routed registry could not deliver them
//! without the same per-subscriber check anyway.
//!
//! The replay ring holds the tail of recent events (bounded count). It is
//! the `Last-Event-ID` watermark backing store: reconnects inside the
//! window replay; anything older, absent, or forged-ahead clamps to the
//! window start (Odoo's reset-0 semantics — never a skip-forward oracle).

use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

use tokio::sync::broadcast;
use uuid::Uuid;

/// One tailed outbox event, already unwrapped from its bus envelope.
#[derive(Debug, Clone)]
pub struct TailedEvent {
    pub id: Uuid,
    /// The bus channel key (BUS-B2-validated by the tailer).
    pub channel: String,
    /// `bus.bus` message type (e.g. `mail.message/insert`).
    pub message_type: String,
    /// The message payload (already the inner `payload` object).
    pub payload: serde_json::Value,
    pub occurred_at: chrono::DateTime<chrono::Utc>,
}

/// Ring + broadcast capacity. Generous: a lagged receiver just misses
/// frames and reconnects with its watermark (the correct failure mode).
const CHANNEL_CAPACITY: usize = 4096;
const RING_CAPACITY: usize = 4096;

pub struct RealtimeRegistry {
    sender: broadcast::Sender<Arc<TailedEvent>>,
    ring: Mutex<VecDeque<Arc<TailedEvent>>>,
    /// Live connections per identity key (`p:<uuid>` / `g:<uuid>`) — the
    /// per-identity connection cap bookkeeping.
    connections: Mutex<std::collections::HashMap<String, usize>>,
}

impl RealtimeRegistry {
    pub fn new() -> Self {
        let (sender, _) = broadcast::channel(CHANNEL_CAPACITY);
        Self {
            sender,
            ring: Mutex::new(VecDeque::new()),
            connections: Mutex::new(std::collections::HashMap::new()),
        }
    }

    /// The tailer's only write: remember in the ring, fan out to live
    /// subscribers. Never touches the database.
    pub fn publish(&self, event: TailedEvent) {
        let event = Arc::new(event);
        {
            let mut ring = self.ring.lock().unwrap_or_else(|e| e.into_inner());
            ring.push_back(Arc::clone(&event));
            while ring.len() > RING_CAPACITY {
                ring.pop_front();
            }
        }
        let _ = self.sender.send(event); // no subscribers is fine
    }

    /// A fresh subscription to the live feed.
    pub fn subscribe(&self) -> broadcast::Receiver<Arc<TailedEvent>> {
        self.sender.subscribe()
    }

    /// Replay for a `Last-Event-ID` watermark. Found → the events AFTER it
    /// (in ring order); absent (reconnect too old, first connect, or a
    /// forged-ahead id) → the whole ring, i.e. clamped to the window start.
    pub fn replay_after(&self, last_id: Option<Uuid>) -> Vec<Arc<TailedEvent>> {
        let ring = self.ring.lock().unwrap_or_else(|e| e.into_inner());
        let start = match last_id.and_then(|id| ring.iter().position(|e| e.id == id)) {
            Some(pos) => pos + 1,
            None => 0,
        };
        ring.iter().skip(start).cloned().collect()
    }

    /// Reserve a connection slot for an identity. `Ok(())` under the cap,
    /// `Err(current)` when the identity already holds `cap` streams.
    pub fn reserve_connection(&self, identity_key: &str, cap: usize) -> Result<(), usize> {
        let mut conns = self.connections.lock().unwrap_or_else(|e| e.into_inner());
        let current = *conns.get(identity_key).unwrap_or(&0);
        if current >= cap {
            return Err(current);
        }
        conns.insert(identity_key.to_string(), current + 1);
        Ok(())
    }

    /// Release a connection slot (guard's Drop).
    pub fn release_connection(&self, identity_key: &str) {
        let mut conns = self.connections.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(n) = conns.get_mut(identity_key) {
            *n = n.saturating_sub(1);
            if *n == 0 {
                conns.remove(identity_key);
            }
        }
    }
}

impl Default for RealtimeRegistry {
    fn default() -> Self {
        Self::new()
    }
}

/// The per-identity key used for connection accounting (mirrors the
/// session-proof identity key).
pub fn identity_key(identity: &crate::application::service::chatter_acl::MessagingIdentity) -> String {
    match identity {
        crate::application::service::chatter_acl::MessagingIdentity::User { partner_id } => {
            format!("p:{partner_id}")
        }
        crate::application::service::chatter_acl::MessagingIdentity::Guest { guest_id } => {
            format!("g:{guest_id}")
        }
    }
}

/// RAII connection-slot guard — releasing on drop keeps the cap honest even
/// when a client vanishes mid-stream.
pub struct ConnectionGuard {
    registry: Arc<RealtimeRegistry>,
    identity_key: String,
}

impl ConnectionGuard {
    pub fn new(registry: Arc<RealtimeRegistry>, identity: &crate::application::service::chatter_acl::MessagingIdentity) -> Self {
        Self { registry, identity_key: identity_key(identity) }
    }
}

impl Drop for ConnectionGuard {
    fn drop(&mut self) {
        self.registry.release_connection(&self.identity_key);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ev(id: Uuid, channel: &str) -> TailedEvent {
        TailedEvent {
            id,
            channel: channel.into(),
            message_type: "mail.message/insert".into(),
            payload: serde_json::json!({ "n": id.to_string() }),
            occurred_at: chrono::Utc::now(),
        }
    }

    #[test]
    fn replay_after_watermark_and_clamp() {
        let r = RealtimeRegistry::new();
        let ids: Vec<Uuid> = (0..3).map(|_| Uuid::new_v4()).collect();
        for id in &ids {
            r.publish(ev(*id, "res.partner_x"));
        }
        // Found → strictly after.
        let after = r.replay_after(Some(ids[0]));
        assert_eq!(after.len(), 2);
        assert_eq!(after[0].id, ids[1]);
        // Absent / forged-ahead → whole ring (window-start clamp).
        assert_eq!(r.replay_after(None).len(), 3);
        assert_eq!(r.replay_after(Some(Uuid::new_v4())).len(), 3);
    }

    #[test]
    fn ring_is_bounded() {
        let r = RealtimeRegistry::new();
        for _ in 0..(RING_CAPACITY + 50) {
            r.publish(ev(Uuid::new_v4(), "c"));
        }
        assert_eq!(r.replay_after(None).len(), RING_CAPACITY);
    }

    #[test]
    fn connection_cap_enforced_and_released() {
        let r = RealtimeRegistry::new();
        assert!(r.reserve_connection("p:a", 2).is_ok());
        assert!(r.reserve_connection("p:a", 2).is_ok());
        assert_eq!(r.reserve_connection("p:a", 2).unwrap_err(), 2);
        r.release_connection("p:a");
        assert!(r.reserve_connection("p:a", 2).is_ok());
    }
}
