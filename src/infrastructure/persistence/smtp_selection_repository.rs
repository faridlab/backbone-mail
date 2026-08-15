//! SMTP server-selection SQL (hand-written; user-owned) — the port of Odoo's
//! `ir.mail_server._find_mail_server` ladder (MAIL-M26).
//!
//! Given a sending address, the ladder picks ONE server row:
//!
//!   rung 0 — a server whose from_filter LISTS the exact address
//!   rung 1 — a server whose from_filter LISTS the address's domain
//!            (`@domain` or bare `domain` entries — from_filter is a
//!            comma-separated list of addresses and/or domains)
//!   rung 2 — a server with NO from_filter (the wildcard)
//!   tie-break — lowest `sequence`, then id (deterministic; Odoo's implicit
//!            read-order becomes explicit here)
//!
//! `from_filter` matching is case-insensitive on both sides (mail addresses
//! normalize to lowercase). Archived servers (`active = false`) and soft-deleted
//! rows are outside the pool.
//!
//! The row returned carries NO secret — `smtp_pass_ref` is the ENV VAR NAME
//! (ADR-0024 interim), resolved by the composing app at send time.

use sqlx::{PgConnection, Row};
use uuid::Uuid;

/// A selected outbound server — the non-secret send config plus the
/// env-var REFERENCE for the password. `authentication == "certificate"` is
/// declared in the schema but UNSUPPORTED: the query returns it like any row,
/// and the APP's transport rejects it (needs a file-based secret store —
/// registered debt, port-notes §8).
#[derive(Debug, Clone)]
pub struct SmtpEndpoint {
    pub server_id: Uuid,
    pub name: String,
    pub from_filter: Option<String>,
    pub smtp_host: String,
    pub smtp_port: i32,
    /// `login` | `certificate` (see type doc).
    pub smtp_authentication: String,
    pub smtp_user: Option<String>,
    /// ENV VAR NAME holding the password — never the secret itself.
    pub smtp_pass_ref: Option<String>,
    /// `none` | `starttls` | `ssl` (strict variants only).
    pub smtp_encryption: String,
    pub smtp_debug: bool,
}

/// Hand-written selection-ladder SQL.
pub struct SmtpSelectionRepository;

impl SmtpSelectionRepository {
    /// Walk the ladder for one sending address. `local_part` and `domain` are
    /// the pre-split, lowercased halves of the address (the query service does
    /// the split; the SQL stays pure matching). `Ok(None)` = no server in the
    /// pool at all — the caller decides (Odoo falls back to system-wide SMTP
    /// config; the port treats "no server" as a `mail_server` failure).
    pub async fn resolve_endpoint(
        conn: &mut PgConnection,
        local_part: &str,
        domain: &str,
    ) -> Result<Option<SmtpEndpoint>, sqlx::Error> {
        let row = sqlx::query(
            r#"WITH entries AS (
                   -- Explode every server's from_filter into one trimmed, lowercased
                   -- entry per row so ANY() can match address vs domain cleanly.
                   SELECT s.id AS server_id, s.name, s.from_filter, s.smtp_host,
                          s.smtp_port, s.smtp_authentication::text AS smtp_authentication,
                          s.smtp_user, s.smtp_pass_ref, s.smtp_encryption::text AS smtp_encryption,
                          s.smtp_debug, s.sequence,
                          lower(trim(e.entry)) AS entry
                   FROM messaging.mail_servers s
                   LEFT JOIN LATERAL unnest(string_to_array(s.from_filter, ',')) AS e(entry)
                       ON s.from_filter IS NOT NULL
                   WHERE s.active
                     AND (s.metadata->>'deleted_at') IS NULL
               )
               SELECT server_id, name, from_filter, smtp_host, smtp_port, smtp_authentication,
                      smtp_user, smtp_pass_ref, smtp_encryption, smtp_debug
               FROM (
                   SELECT *,
                       CASE
                           -- rung 0: exact address listed
                           WHEN from_filter IS NOT NULL AND entry = $1 || '@' || $2 THEN 0
                           -- rung 1: domain listed ('@domain' or bare)
                           WHEN from_filter IS NOT NULL AND (entry = '@' || $2 OR entry = $2) THEN 1
                           -- rung 2: wildcard (no from_filter)
                           WHEN from_filter IS NULL THEN 2
                       END AS rung
                   FROM entries
               ) ranked
               WHERE rung IS NOT NULL
               ORDER BY rung, sequence, server_id
               LIMIT 1"#,
        )
        .bind(local_part)
        .bind(domain)
        .fetch_optional(&mut *conn)
        .await?;
        Ok(row.map(|r| SmtpEndpoint {
            server_id: r.get("server_id"),
            name: r.get("name"),
            from_filter: r.get("from_filter"),
            smtp_host: r.get("smtp_host"),
            smtp_port: r.get("smtp_port"),
            smtp_authentication: r.get("smtp_authentication"),
            smtp_user: r.get("smtp_user"),
            smtp_pass_ref: r.get("smtp_pass_ref"),
            smtp_encryption: r.get("smtp_encryption"),
            smtp_debug: r.get("smtp_debug"),
        }))
    }
}
