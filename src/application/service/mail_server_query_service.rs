//! The outbound server-selection query service (hand-written; user-owned) —
//! MAIL-M26's read side.
//!
//! Wraps [`SmtpSelectionRepository::resolve_endpoint`] with the address
//! normalization Odoo does at the `_find_mail_server` call site: split on the
//! LAST `@` (local parts may contain quoted `@`), lowercase both halves, and
//! refuse an address with no domain (the ladder has nothing to match — the
//! caller turns that into a `mail_from_invalid` failure).
//!
//! The service never resolves `smtp_pass_ref` — that is the composing app's
//! job at send time (the module never reads env; ADR-0024).

use crate::infrastructure::persistence::smtp_selection_repository::{
    SmtpEndpoint, SmtpSelectionRepository,
};

#[derive(Debug, thiserror::Error)]
pub enum MailServerQueryError {
    #[error("db: {0}")]
    Db(#[from] sqlx::Error),
    #[error("invalid sender address: {0}")]
    Invalid(String),
}

pub struct MailServerQueryService {
    pool: sqlx::PgPool,
}

impl MailServerQueryService {
    pub fn new(pool: sqlx::PgPool) -> Self {
        Self { pool }
    }

    /// Walk the `_find_mail_server` ladder for one sending address.
    pub async fn resolve_endpoint(
        &self,
        from_email: &str,
    ) -> Result<Option<SmtpEndpoint>, MailServerQueryError> {
        let (local_part, domain) = split_address(from_email)
            .ok_or_else(|| MailServerQueryError::Invalid(from_email.to_string()))?;
        let conn = &mut self.pool.acquire().await?;
        SmtpSelectionRepository::resolve_endpoint(conn, &local_part, &domain)
            .await
            .map_err(Into::into)
    }
}

/// Split `local@domain` on the LAST `@`, lowercasing both halves. `None` when
/// there is no `@` (the ladder cannot match a domainless address).
pub fn split_address(address: &str) -> Option<(String, String)> {
    let idx = address.rfind('@')?;
    let local = address[..idx].trim().to_ascii_lowercase();
    let domain = address[idx + 1..].trim().to_ascii_lowercase();
    if local.is_empty() || domain.is_empty() {
        return None;
    }
    Some((local, domain))
}

#[cfg(test)]
mod tests {
    use super::split_address;

    #[test]
    fn splits_on_last_at_and_lowercases() {
        assert_eq!(
            split_address("\"a@b\"@Example.COM"),
            Some(("\"a@b\"".into(), "example.com".into()))
        );
    }

    #[test]
    fn domainless_address_is_none() {
        assert_eq!(split_address("no-domain"), None);
        assert_eq!(split_address("@example.com"), None);
        assert_eq!(split_address("user@"), None);
    }
}
