//! Repository for alias resolution (hand-written; user-owned).
//!
//! Holds the SQL for [`crate::application::service::AliasWriteService`]: the
//! `(alias_name, COALESCE(alias_domain_id, 0))` resolution semantics of MAIL-M33.
//! Odoo protects "only-NULL-domain collides with only-NULL-domain" via a raw-SQL
//! unique index whose COALESCE expression treats NULL as a first-class value; the
//! resolver must use the SAME semantics (`IS NOT DISTINCT FROM`), or a NULL-domain
//! alias would resolve against a non-NULL-domain row and vice versa.

use sqlx::{PgConnection, Row};
use uuid::Uuid;

/// The resolved alias, as the inbound router needs it (MAIL-M33).
pub struct AliasRow {
    pub id: Uuid,
    pub alias_name: String,
    pub alias_domain_id: Option<Uuid>,
    pub alias_contact: String,
    pub alias_model_id: Option<Uuid>,
    pub alias_parent_model_id: Option<Uuid>,
    pub alias_parent_thread_id: Option<Uuid>,
    pub alias_user_id: Option<Uuid>,
    pub alias_defaults: Option<String>,
    pub alias_force_thread_id: Option<Uuid>,
    pub alias_reply_to_address: Option<String>,
}

/// Hand-written alias resolution SQL.
pub struct AliasResolutionRepository;

impl AliasResolutionRepository {
    pub fn new() -> Self {
        Self
    }
}

impl Default for AliasResolutionRepository {
    fn default() -> Self {
        Self::new()
    }
}

impl AliasResolutionRepository {
    /// Resolve `(alias_name, alias_domain_id)` with COALESCE-unique semantics: a
    /// NULL domain matches only a NULL domain (`IS NOT DISTINCT FROM` — the exact
    /// semantics of the `(alias_name, COALESCE(alias_domain_id, 0))` unique index).
    /// `Ok(None)` = no such alias.
    pub async fn resolve(
        conn: &mut PgConnection,
        alias_name: &str,
        alias_domain_id: Option<Uuid>,
    ) -> Result<Option<AliasRow>, sqlx::Error> {
        let row = sqlx::query(
            r#"SELECT id, alias_name, alias_domain_id, alias_contact::text AS alias_contact,
                      alias_model_id, alias_parent_model_id, alias_parent_thread_id,
                      alias_user_id, alias_defaults, alias_force_thread_id, alias_reply_to_address
               FROM messaging.mail_aliases
               WHERE alias_name = $1
                 AND alias_domain_id IS NOT DISTINCT FROM $2
                 AND (metadata->>'deleted_at') IS NULL
               LIMIT 1"#,
        )
        .bind(alias_name)
        .bind(alias_domain_id)
        .fetch_optional(&mut *conn)
        .await?;
        Ok(row.map(|r| AliasRow {
            id: r.get("id"),
            alias_name: r.get("alias_name"),
            alias_domain_id: r.get("alias_domain_id"),
            alias_contact: r.get("alias_contact"),
            alias_model_id: r.get("alias_model_id"),
            alias_parent_model_id: r.get("alias_parent_model_id"),
            alias_parent_thread_id: r.get("alias_parent_thread_id"),
            alias_user_id: r.get("alias_user_id"),
            alias_defaults: r.get("alias_defaults"),
            alias_force_thread_id: r.get("alias_force_thread_id"),
            alias_reply_to_address: r.get("alias_reply_to_address"),
        }))
    }
}
