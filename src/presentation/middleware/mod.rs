//! Presentation middleware (hand-written; user-owned).

pub mod guest_context;

pub use guest_context::{
    forbidden, guest_context, unauthorized, AuthPartnerId, IsAdmin, WireIdentity,
};
