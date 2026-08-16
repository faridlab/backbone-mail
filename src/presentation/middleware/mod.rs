//! Presentation middleware (hand-written; user-owned).

pub mod guest_context;
pub mod user_scope;

pub use guest_context::{
    forbidden, guest_context, unauthorized, AuthPartnerId, IsAdmin, WireIdentity,
};
pub use user_scope::{user_scope, UserScope};
