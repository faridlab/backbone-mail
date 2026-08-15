//! Realtime delivery (hand-written; user-owned).
//!
//! Stage-3 member: the SSE-session proof ([`session`]) and the shared
//! [`SessionSecret`] the host app installs as a router extension. Stage-4
//! adds the outbox tailer, the broadcast registry, and the stream endpoint.

pub mod registry;
pub mod session;
pub mod sse;
pub mod tailer;

use std::sync::Arc;

/// The HMAC secret behind SSE session proofs. Installed ONCE by the host app
/// as a router extension (`Extension(SessionSecret(...))`) on the presence
/// and stream route groups — never per-request, never in a URL.
#[derive(Clone)]
pub struct SessionSecret(pub Arc<Vec<u8>>);

pub use registry::RealtimeRegistry;
