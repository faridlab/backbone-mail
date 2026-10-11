// This directory's extension (hand-written; ADR-0031). The generated mod.rs (lib.rs for
// src/) pulls it in with include!, so its lines resolve against this directory.

// The bus-derived platform constants (ADR-0014 sentinel + ADR-0017 channel keys and
// the in-tx outbox stage helper). Hand-authored; declared user-owned in
// metaphor.codegen.yaml.
pub mod constants;
pub use constants::{
    bus_envelope, discuss_channel, guest_channel, partner_channel, record_channel,
    stage_bus_event, MESSAGING_PLATFORM_COMPANY_ID, OUTBOX_SCHEMA,
};
