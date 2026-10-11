// This directory's extension (hand-written; ADR-0031). The generated mod.rs (lib.rs for
// src/) pulls it in with include!, so its lines resolve against this directory.

// Increment-2 hand-written route groups (user-owned; survive regen).
pub mod thread_routes;
pub mod channel_routes;
pub mod guest_routes;
pub mod presence_routes;
pub mod attachment_routes;
pub mod webhook_routes;
pub mod public_routes;
// Increment 3 — the inbound email webhook (MAIL-M27; token consteq, bare mount).
pub mod inbound_routes;
