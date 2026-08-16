//! Gateway transport adapters (ADR-0024 seam) — the module-side impls of the
//! outbound ports. The module owns the queue rows and state machines; these
//! adapters own the sockets. Both are feature-gated so the crate stays
//! transport-free unless a composing service opts in:
//!
//! - `gateway-smtp`     → [`smtp_mail_api::SmtpMailApi`] over `backbone-email`'s
//!   lettre service (per-row server selection via the MAIL-M26 ladder)
//! - `gateway-sms-http` → [`sms_http_api::HttpSmsApi`], the generic IAP-shaped
//!   HTTP JSON SMS provider
//!
//! Promoted from backbone-messaging-app (increment 3) so no composing service
//! re-implements them; the app-specific outbox relay stays in the app.

#[cfg(feature = "gateway-smtp")]
pub mod smtp_mail_api;
#[cfg(feature = "gateway-smtp")]
pub use smtp_mail_api::SmtpMailApi;

#[cfg(feature = "gateway-sms-http")]
pub mod sms_http_api;
#[cfg(feature = "gateway-sms-http")]
pub use sms_http_api::HttpSmsApi;
