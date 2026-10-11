// This directory's extension (hand-written; ADR-0031). The generated mod.rs (lib.rs for
// src/) pulls it in with include!, so its lines resolve against this directory.

// Gateway transport adapters (ADR-0024 seam) — feature-gated so the module
// builds transport-free unless the composing service opts in.
#[cfg(any(feature = "gateway-smtp", feature = "gateway-sms-http"))]
pub mod gateway;
