//! Inbound-gateway SQL (hand-written; user-owned) — the reads and writes of the
//! one-tx inbound pipeline (MAIL-M27/M28; MAIL-B2/B6).
//!
//! Everything here runs on the CALLER'S transaction: `MailInboundService` opens
//! one tx per message and every write (dedup probe, allowlist match, routing,
//! the message insert, the server bookkeeping) rides it — a failure rolls the
//! whole message back, which is the MAIL-B2 fix (Odoo's fetchmail commits the
//! whole batch in one tx, so one poisoned message loses its whole batch).
//!
//! The token check is deliberately NOT here: it happens BEFORE the tx opens
//! (fail-closed, zero writes before auth — the /sms/status playbook).

use chrono::{DateTime, Utc};
use sqlx::{PgConnection, Row};
use uuid::Uuid;

/// The fetchmail server row as the inbound pipeline sees it (auth + routing
/// config). `token_hash` is SHA-256 hex of the bearer token.
pub struct InboundServerRow {
    pub id: Uuid,
    pub active: bool,
    pub state: String,
    pub token_hash: String,
    pub default_thread_model: Option<String>,
}

/// Hand-written inbound-gateway SQL.
pub struct InboundRepository;

impl InboundRepository {
    /// Load a server for auth. `Ok(None)` = unknown id — the route must answer
    /// the SAME 401 as a bad token (no server-existence oracle).
    pub async fn find_server(
        conn: &mut PgConnection,
        server_id: Uuid,
    ) -> Result<Option<InboundServerRow>, sqlx::Error> {
        let row = sqlx::query(
            r#"SELECT id, active, state::text AS state, token_hash, default_thread_model
               FROM messaging.fetchmail_servers
               WHERE id = $1 AND (metadata->>'deleted_at') IS NULL"#,
        )
        .bind(server_id)
        .fetch_optional(&mut *conn)
        .await?;
        Ok(row.map(|r| InboundServerRow {
            id: r.get("id"),
            active: r.get("active"),
            state: r.get("state"),
            token_hash: r.get("token_hash"),
            default_thread_model: r.get("default_thread_model"),
        }))
    }

    /// MAIL-M28 allowlist match on the envelope sender. Patterns: exact address
    /// (`a@b.c`), `@domain` (any local part), bare domain (`b.c`). A row with a
    /// null `fetchmail_server_id` applies to ALL servers; a row scoped to
    /// another server does not apply. Case-insensitive on both sides.
    pub async fn sender_allowed(
        conn: &mut PgConnection,
        server_id: Uuid,
        envelope_from: &str,
    ) -> Result<bool, sqlx::Error> {
        let from = envelope_from.trim().to_ascii_lowercase();
        let domain = from.rsplit('@').next().unwrap_or_default().to_string();
        let hit = sqlx::query_scalar::<_, i64>(
            r#"SELECT COUNT(*)
               FROM messaging.mail_gateway_allowed
               WHERE active
                 AND (metadata->>'deleted_at') IS NULL
                 AND (fetchmail_server_id IS NULL OR fetchmail_server_id = $1)
                 AND lower(pattern) IN ($2, $3, '@' || $3)"#,
        )
        .bind(server_id)
        .bind(&from)
        .bind(&domain)
        .fetch_one(&mut *conn)
        .await?;
        Ok(hit > 0)
    }

    /// MAIL-B6 dedup probe: does a message with this RFC id already exist?
    /// (The UNIQUE partial index is the race backstop — the service also folds
    /// a 23505 on insert into the duplicate outcome.)
    pub async fn find_by_message_id(
        conn: &mut PgConnection,
        message_id: &str,
    ) -> Result<Option<Uuid>, sqlx::Error> {
        sqlx::query_scalar::<_, Uuid>(
            r#"SELECT id FROM messaging.mail_messages
               WHERE message_id = $1
               LIMIT 1"#,
        )
        .bind(message_id)
        .fetch_optional(&mut *conn)
        .await
    }

    /// Reply-collation: find the thread an inbound reply continues. Matches the
    /// parent by its RFC `message_id` (= the child's `In-Reply-To`) and copies
    /// the parent's `(model, res_id)` — the routing-without-registry trick
    /// (Odoo walks mail.message.message_id the same way in `message_route`).
    pub async fn find_parent_thread(
        conn: &mut PgConnection,
        parent_message_id: &str,
    ) -> Result<Option<(Option<String>, Option<Uuid>)>, sqlx::Error> {
        let row = sqlx::query(
            r#"SELECT model, res_id FROM messaging.mail_messages
               WHERE message_id = $1 AND model IS NOT NULL
               LIMIT 1"#,
        )
        .bind(parent_message_id)
        .fetch_optional(&mut *conn)
        .await?;
        Ok(row.map(|r| (r.get::<Option<String>, _>("model"), r.get::<Option<Uuid>, _>("res_id"))))
    }

    /// Learn a thread's model STRING from an existing message on it. The alias
    /// registry columns are uuid refs with no name (the ir.model registry is
    /// framework, not ported — alias.model.yaml PORT DECISIONS), so alias-routed
    /// threads recover the model string by collation: any chatter row already
    /// on the thread says which model owns it.
    pub async fn learn_thread_model(
        conn: &mut PgConnection,
        res_id: Uuid,
    ) -> Result<Option<String>, sqlx::Error> {
        sqlx::query_scalar::<_, Option<String>>(
            r#"SELECT model FROM messaging.mail_messages
               WHERE res_id = $1 AND model IS NOT NULL
               LIMIT 1"#,
        )
        .bind(res_id)
        .fetch_optional(&mut *conn)
        .await
        .map(|r| r.flatten())
    }

    /// Resolve the alias-domain id for an inbound domain (the `to` address's
    /// half of M33 routing). One row per name is the operational invariant
    /// (mail_alias_domain is single-tenant — schema note).
    pub async fn find_domain_id_by_name(
        conn: &mut PgConnection,
        domain: &str,
    ) -> Result<Option<Uuid>, sqlx::Error> {
        sqlx::query_scalar::<_, Uuid>(
            r#"SELECT id FROM messaging.mail_alias_domains
               WHERE lower(name) = lower($1)
                 AND (metadata->>'deleted_at') IS NULL
               LIMIT 1"#,
        )
        .bind(domain)
        .fetch_optional(&mut *conn)
        .await
    }

    /// Bookkeeping on success (Odoo `fetchmail.server.date`): last_fetch_at.
    pub async fn touch_server_success(
        conn: &mut PgConnection,
        server_id: Uuid,
        at: DateTime<Utc>,
    ) -> Result<(), sqlx::Error> {
        sqlx::query(
            r#"UPDATE messaging.fetchmail_servers
               SET last_fetch_at = $2, error_at = NULL, error_message = NULL
               WHERE id = $1"#,
        )
        .bind(server_id)
        .bind(at)
        .execute(&mut *conn)
        .await?;
        Ok(())
    }

    /// Bookkeeping on failure — bounded, overwritten (a status field, not a log).
    #[allow(clippy::too_many_arguments)]
    pub async fn touch_server_failure(
        conn: &mut PgConnection,
        server_id: Uuid,
        at: DateTime<Utc>,
        message: &str,
    ) -> Result<(), sqlx::Error> {
        sqlx::query(
            r#"UPDATE messaging.fetchmail_servers
               SET error_at = $2, error_message = left($3, 500)
               WHERE id = $1"#,
        )
        .bind(server_id)
        .bind(at)
        .bind(message)
        .execute(&mut *conn)
        .await?;
        Ok(())
    }
}
