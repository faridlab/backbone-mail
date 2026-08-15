//! The chatter ACL seam (MAIL-B1) — who may read/post on a parent document's
//! chatter (hand-written; user-owned).
//!
//! `mail.message` is polymorphic (`model`, `res_id`): the SAME table carries
//! chatter for every host document. Odoo enforces parent-document access with
//! `ir.rules` that JOIN `mail.message` back to each host's own ACLs at query
//! time. The port does NOT port ir.rules — MAIL-B1 replaces the rule-shaped
//! WHERE with a PROCEDURAL walk: every read/write path calls a
//! [`ThreadAccessResolver`] registered by the host service.
//!
//! Deny-by-default: the module ships [`DenyHostDocs`] — channel chatter and a
//! partner's own channel work out of the box, but parent-document chatter is
//! DENIED until a host registers a resolver via
//! `MessagingModule::set_thread_acl(...)` (the swappable [`ThreadAclSlot`]).
//! A host that forgets to register gets a closed surface, never an open one.
//!
//! Odoo also special-cases two non-document threads, ported as always-allow:
//!  - `res.partner` — a partner's chatter is the partner's own wall; the
//!    identity's partner row is readable by itself.
//!  - `discuss.channel` — handled by channel-membership checks in the channel
//!    services, not here (those paths never consult the resolver).

use std::sync::Arc;

use async_trait::async_trait;
use uuid::Uuid;

use crate::domain::event::constants::partner_channel;

/// Who is asking — the two identity sources of the messaging stack (M43: a
/// guest is a real participant; anonymous is nobody and fails every check).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MessagingIdentity {
    User { partner_id: Uuid },
    Guest { guest_id: Uuid },
}

impl MessagingIdentity {
    /// The bus channel-key this identity's inbox events are addressed to
    /// (users converge on their partner's channel — see `partner_channel`).
    pub fn channel(&self) -> String {
        match self {
            MessagingIdentity::User { partner_id } => partner_channel(*partner_id),
            MessagingIdentity::Guest { guest_id } => {
                crate::domain::event::constants::guest_channel(*guest_id)
            }
        }
    }

    /// The partner id, when the identity has one (guests don't).
    pub fn partner_id(&self) -> Option<Uuid> {
        match self {
            MessagingIdentity::User { partner_id } => Some(*partner_id),
            MessagingIdentity::Guest { .. } => None,
        }
    }

    /// The guest id, when the identity is a guest (users aren't).
    pub fn guest_id(&self) -> Option<Uuid> {
        match self {
            MessagingIdentity::Guest { guest_id } => Some(*guest_id),
            MessagingIdentity::User { .. } => None,
        }
    }
}

/// The MAIL-B1 seam. Host services implement this and register it on the
/// module builder; every chatter read/write path consults it BEFORE touching
/// `mail_messages`.
///
/// The `pool` argument lets a resolver do its own host-table lookup (the
/// framework's company-scope helpers, an ACL table, a status check) without
/// the messaging module knowing anything about host schemas.
#[async_trait]
pub trait ThreadAccessResolver: Send + Sync {
    /// May this identity read the chatter of `(model, res_id)`?
    async fn can_read(
        &self,
        pool: &sqlx::PgPool,
        identity: &MessagingIdentity,
        model: &str,
        res_id: Uuid,
    ) -> bool;

    /// May this identity post (or edit/schedule a post) on `(model, res_id)`?
    /// Typically stricter than read (Odoo: read = host read ACL; post = write
    /// or follower, per host policy).
    async fn can_post(
        &self,
        pool: &sqlx::PgPool,
        identity: &MessagingIdentity,
        model: &str,
        res_id: Uuid,
    ) -> bool;
}

/// The deny-by-default resolver: parent-document chatter is closed until a
/// host registers a real one. The only allows are the two Odoo
/// non-document specials — a partner's OWN wall (`res.partner_<self>`) is
/// readable/postable by that partner.
pub struct DenyHostDocs;

#[async_trait]
impl ThreadAccessResolver for DenyHostDocs {
    async fn can_read(
        &self,
        _pool: &sqlx::PgPool,
        identity: &MessagingIdentity,
        model: &str,
        res_id: Uuid,
    ) -> bool {
        is_own_partner_wall(identity, model, res_id)
    }

    async fn can_post(
        &self,
        _pool: &sqlx::PgPool,
        identity: &MessagingIdentity,
        model: &str,
        res_id: Uuid,
    ) -> bool {
        is_own_partner_wall(identity, model, res_id)
    }
}

/// `res.partner` chatter on YOUR OWN partner row is the Odoo partner-wall
/// special case — allowed for any authenticated identity, no host needed.
/// Guests never match (they have no partner).
fn is_own_partner_wall(identity: &MessagingIdentity, model: &str, res_id: Uuid) -> bool {
    model == "res.partner" && identity.partner_id() == Some(res_id)
}

/// A static resolver the host app can build from config/composition when it
/// doesn't want to write a full trait impl — e.g. an allowlist of
/// `(model, res_id)` pairs, or "same partner wall only" (the DenyHostDocs
/// behavior with a closure escape hatch for the document models).
pub struct StaticThreadAccess {
    /// The models whose chatter is open to every authenticated identity.
    /// ONLY for genuinely public hosts — a wrong entry here is a data leak.
    pub open_models: Vec<String>,
}

#[async_trait]
impl ThreadAccessResolver for StaticThreadAccess {
    async fn can_read(
        &self,
        _pool: &sqlx::PgPool,
        identity: &MessagingIdentity,
        model: &str,
        res_id: Uuid,
    ) -> bool {
        is_own_partner_wall(identity, model, res_id)
            || (self.open_models.iter().any(|m| m == model) && identity.partner_id().is_some())
    }

    async fn can_post(
        &self,
        _pool: &sqlx::PgPool,
        identity: &MessagingIdentity,
        model: &str,
        res_id: Uuid,
    ) -> bool {
        is_own_partner_wall(identity, model, res_id)
            || (self.open_models.iter().any(|m| m == model) && identity.partner_id().is_some())
    }
}

/// A shared, swappable resolver slot — how a host registers its ACL without
/// the generated module builder needing a new field. Services are built over
/// the slot (defaulting to [`DenyHostDocs`]); `MessagingModule::set_thread_acl`
/// installs the host's resolver and every service sees it on the NEXT call.
/// Reads are cheap (an `RwLock` read); installs happen once at boot.
#[derive(Clone)]
pub struct ThreadAclSlot {
    inner: std::sync::Arc<std::sync::RwLock<Arc<dyn ThreadAccessResolver>>>,
}

impl ThreadAclSlot {
    /// Install (replace) the active resolver.
    pub fn install(&self, resolver: Arc<dyn ThreadAccessResolver>) {
        *self.inner.write().unwrap() = resolver;
    }

    /// The currently installed resolver.
    pub fn current(&self) -> Arc<dyn ThreadAccessResolver> {
        self.inner.read().unwrap().clone()
    }
}

impl Default for ThreadAclSlot {
    fn default() -> Self {
        Self { inner: std::sync::Arc::new(std::sync::RwLock::new(Arc::new(DenyHostDocs))) }
    }
}

#[async_trait::async_trait]
impl ThreadAccessResolver for ThreadAclSlot {
    async fn can_read(
        &self,
        pool: &sqlx::PgPool,
        identity: &MessagingIdentity,
        model: &str,
        res_id: Uuid,
    ) -> bool {
        self.current().can_read(pool, identity, model, res_id).await
    }

    async fn can_post(
        &self,
        pool: &sqlx::PgPool,
        identity: &MessagingIdentity,
        model: &str,
        res_id: Uuid,
    ) -> bool {
        self.current().can_post(pool, identity, model, res_id).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tid() -> Uuid {
        Uuid::new_v4()
    }

    #[tokio::test]
    async fn deny_host_docs_denies_document_chatter() {
        // MAIL-B1: a crm.lead thread is closed under the default resolver —
        // the host must register, or nothing reads.
        let id = MessagingIdentity::User { partner_id: tid() };
        let r = DenyHostDocs.can_read(&unreachable_pool(), &id, "crm.lead", tid()).await;
        assert!(!r, "document chatter must be deny-by-default");
    }

    #[tokio::test]
    async fn own_partner_wall_is_open() {
        let p = tid();
        let id = MessagingIdentity::User { partner_id: p };
        assert!(DenyHostDocs.can_read(&unreachable_pool(), &id, "res.partner", p).await);
        assert!(DenyHostDocs.can_post(&unreachable_pool(), &id, "res.partner", p).await);
        // someone ELSE's wall is closed.
        assert!(!DenyHostDocs.can_read(&unreachable_pool(), &id, "res.partner", tid()).await);
    }

    #[tokio::test]
    async fn guests_never_match_partner_wall() {
        let id = MessagingIdentity::Guest { guest_id: tid() };
        assert!(!DenyHostDocs.can_read(&unreachable_pool(), &id, "res.partner", tid()).await);
    }

    #[tokio::test]
    async fn static_open_models_allows_authenticated_users_only() {
        let s = StaticThreadAccess { open_models: vec!["project.project".into()] };
        let u = MessagingIdentity::User { partner_id: tid() };
        let g = MessagingIdentity::Guest { guest_id: tid() };
        assert!(s.can_read(&unreachable_pool(), &u, "project.project", tid()).await);
        assert!(!s.can_read(&unreachable_pool(), &g, "project.project", tid()).await);
        // an unlisted model stays closed
        assert!(!s.can_read(&unreachable_pool(), &u, "crm.lead", tid()).await);
    }

    /// The default resolvers must never touch the pool — deny/allow is decided
    /// from identity alone. Build a pool handle we can't actually connect with;
    /// if it were used the test would hang/error rather than pass.
    fn unreachable_pool() -> sqlx::PgPool {
        sqlx::PgPool::connect_lazy("postgres://nobody@127.0.0.1:1/none")
            .unwrap_or_else(|_| panic!("lazy pool handle must always construct"))
    }
}
