//! The alias resolution service (hand-written; user-owned).
//!
//! The port of Odoo's inbound alias lookup (MAIL-M33): `(alias_name,
//! COALESCE(alias_domain_id, 0))` uniqueness semantics, re-expressed as
//! `IS NOT DISTINCT FROM` in the resolver. The inbound ROUTER itself is increment 3
//! (MAIL-M26..M28 deferral); this verb is the resolution seam it will call.

use uuid::Uuid;

use crate::infrastructure::persistence::alias_resolution_repository::{
    AliasResolutionRepository, AliasRow,
};

#[derive(Debug, thiserror::Error)]
pub enum AliasError {
    #[error("db: {0}")]
    Db(#[from] sqlx::Error),
    #[error("invalid input: {0}")]
    Invalid(String),
}

pub struct AliasWriteService {
    pool: sqlx::PgPool,
}

impl AliasWriteService {
    pub fn new(pool: sqlx::PgPool) -> Self {
        Self { pool }
    }

    /// Resolve a local-part (+ optional domain) to its alias. A NULL domain matches
    /// ONLY a NULL-domain alias and vice versa — the COALESCE-unique semantics
    /// (MAIL-M33) preserved verbatim. `Ok(None)` = unknown address (the caller's
    /// bounce/catchall decision).
    pub async fn resolve(
        &self,
        alias_name: &str,
        alias_domain_id: Option<Uuid>,
    ) -> Result<Option<AliasRow>, AliasError> {
        let alias_name = alias_name.trim().to_ascii_lowercase();
        if alias_name.is_empty() {
            return Err(AliasError::Invalid("alias_name is required".into()));
        }
        let mut conn = self.pool.acquire().await?;
        Ok(AliasResolutionRepository::resolve(&mut conn, &alias_name, alias_domain_id).await?)
    }
}
