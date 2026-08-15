use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use uuid::Uuid;

use super::SmtpAuthentication;
use super::SmtpEncryption;
use super::AuditMetadata;

/// Strongly-typed ID for MailServer
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct MailServerId(pub Uuid);

impl MailServerId {
    pub fn new(id: Uuid) -> Self { Self(id) }
    pub fn generate() -> Self { Self(Uuid::new_v4()) }
    pub fn into_inner(self) -> Uuid { self.0 }
}

impl std::fmt::Display for MailServerId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::str::FromStr for MailServerId {
    type Err = uuid::Error;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(Self(Uuid::parse_str(s)?))
    }
}

impl From<Uuid> for MailServerId {
    fn from(id: Uuid) -> Self { Self(id) }
}

impl From<MailServerId> for Uuid {
    fn from(id: MailServerId) -> Self { id.0 }
}

impl AsRef<Uuid> for MailServerId {
    fn as_ref(&self) -> &Uuid { &self.0 }
}

impl std::ops::Deref for MailServerId {
    type Target = Uuid;
    fn deref(&self) -> &Self::Target { &self.0 }
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct MailServer {
    pub id: Uuid,
    pub name: String,
    pub from_filter: Option<String>,
    pub smtp_host: String,
    pub smtp_port: i32,
    pub smtp_authentication: SmtpAuthentication,
    pub smtp_user: Option<String>,
    pub smtp_pass_ref: Option<String>,
    pub smtp_encryption: SmtpEncryption,
    pub smtp_debug: bool,
    pub sequence: i32,
    pub active: bool,
    #[serde(default)]
    #[sqlx(json)]
    pub metadata: AuditMetadata,
}

impl MailServer {
    /// Create a builder for MailServer
    pub fn builder() -> MailServerBuilder {
        <MailServerBuilder as Default>::default()
    }

    /// Create a new MailServer with required fields
    pub fn new(name: String, smtp_host: String, smtp_port: i32, smtp_authentication: SmtpAuthentication, smtp_encryption: SmtpEncryption, smtp_debug: bool, sequence: i32, active: bool) -> Self {
        Self {
            id: Uuid::new_v4(),
            name,
            from_filter: None,
            smtp_host,
            smtp_port,
            smtp_authentication,
            smtp_user: None,
            smtp_pass_ref: None,
            smtp_encryption,
            smtp_debug,
            sequence,
            active,
            metadata: AuditMetadata::default(),
        }
    }

    /// Get the entity's unique identifier
    pub fn id(&self) -> &Uuid {
        &self.id
    }

    /// Get a strongly-typed ID for this entity
    pub fn typed_id(&self) -> MailServerId {
        MailServerId(self.id)
    }

    /// Get when this entity was created
    pub fn created_at(&self) -> Option<&DateTime<Utc>> {
        self.metadata.created_at.as_ref()
    }

    /// Get when this entity was last updated
    pub fn updated_at(&self) -> Option<&DateTime<Utc>> {
        self.metadata.updated_at.as_ref()
    }

    /// Check if this entity is soft deleted
    pub fn is_deleted(&self) -> bool {
        self.metadata.deleted_at.is_some()
    }

    /// Check if this entity is active (not deleted)
    pub fn is_active(&self) -> bool {
        self.metadata.deleted_at.is_none()
    }

    /// Get when this entity was deleted
    pub fn deleted_at(&self) -> Option<&DateTime<Utc>> {
        self.metadata.deleted_at.as_ref()
    }

    /// Get who created this entity
    pub fn created_by(&self) -> Option<&Uuid> {
        self.metadata.created_by.as_ref()
    }

    /// Get who last updated this entity
    pub fn updated_by(&self) -> Option<&Uuid> {
        self.metadata.updated_by.as_ref()
    }

    /// Get who deleted this entity
    pub fn deleted_by(&self) -> Option<&Uuid> {
        self.metadata.deleted_by.as_ref()
    }


    // ==========================================================
    // Fluent Setters (with_* for optional fields)
    // ==========================================================

    /// Set the from_filter field (chainable)
    pub fn with_from_filter(mut self, value: String) -> Self {
        self.from_filter = Some(value);
        self
    }

    /// Set the smtp_user field (chainable)
    pub fn with_smtp_user(mut self, value: String) -> Self {
        self.smtp_user = Some(value);
        self
    }

    /// Set the smtp_pass_ref field (chainable)
    pub fn with_smtp_pass_ref(mut self, value: String) -> Self {
        self.smtp_pass_ref = Some(value);
        self
    }

    // ==========================================================
    // Partial Update
    // ==========================================================

    /// Apply partial updates from a map of field name to JSON value
    pub fn apply_patch(&mut self, fields: std::collections::HashMap<String, serde_json::Value>) {
        for (key, value) in fields {
            match key.as_str() {
                "name" => {
                    if let Ok(v) = serde_json::from_value(value) { self.name = v; }
                }
                "from_filter" => {
                    if let Ok(v) = serde_json::from_value(value) { self.from_filter = v; }
                }
                "smtp_host" => {
                    if let Ok(v) = serde_json::from_value(value) { self.smtp_host = v; }
                }
                "smtp_port" => {
                    if let Ok(v) = serde_json::from_value(value) { self.smtp_port = v; }
                }
                "smtp_authentication" => {
                    if let Ok(v) = serde_json::from_value(value) { self.smtp_authentication = v; }
                }
                "smtp_user" => {
                    if let Ok(v) = serde_json::from_value(value) { self.smtp_user = v; }
                }
                "smtp_pass_ref" => {
                    if let Ok(v) = serde_json::from_value(value) { self.smtp_pass_ref = v; }
                }
                "smtp_encryption" => {
                    if let Ok(v) = serde_json::from_value(value) { self.smtp_encryption = v; }
                }
                "smtp_debug" => {
                    if let Ok(v) = serde_json::from_value(value) { self.smtp_debug = v; }
                }
                "sequence" => {
                    if let Ok(v) = serde_json::from_value(value) { self.sequence = v; }
                }
                "active" => {
                    if let Ok(v) = serde_json::from_value(value) { self.active = v; }
                }
                _ => {} // ignore unknown fields
            }
        }
    }

    // <<< CUSTOM METHODS START >>>
    // <<< CUSTOM METHODS END >>>
}

impl super::Entity for MailServer {
    type Id = Uuid;

    fn entity_id(&self) -> &Self::Id {
        &self.id
    }

    fn entity_type() -> &'static str {
        "MailServer"
    }
}

impl backbone_core::PersistentEntity for MailServer {
    fn entity_id(&self) -> String {
        self.id.to_string()
    }
    fn set_entity_id(&mut self, id: String) {
        if let Ok(uuid) = uuid::Uuid::parse_str(&id) {
            self.id = uuid;
        }
    }
    fn created_at(&self) -> Option<chrono::DateTime<chrono::Utc>> {
        self.metadata.created_at
    }
    fn set_created_at(&mut self, ts: chrono::DateTime<chrono::Utc>) {
        self.metadata.created_at = Some(ts);
    }
    fn updated_at(&self) -> Option<chrono::DateTime<chrono::Utc>> {
        self.metadata.updated_at
    }
    fn set_updated_at(&mut self, ts: chrono::DateTime<chrono::Utc>) {
        self.metadata.updated_at = Some(ts);
    }
    fn deleted_at(&self) -> Option<chrono::DateTime<chrono::Utc>> {
        self.metadata.deleted_at
    }
    fn set_deleted_at(&mut self, ts: Option<chrono::DateTime<chrono::Utc>>) {
        self.metadata.deleted_at = ts;
    }
}

impl backbone_orm::EntityRepoMeta for MailServer {
    fn column_types() -> std::collections::HashMap<String, String> {
        let mut m = std::collections::HashMap::new();
        m.insert("id".to_string(), "uuid".to_string());
        m.insert("smtp_authentication".to_string(), "smtp_authentication".to_string());
        m.insert("smtp_encryption".to_string(), "smtp_encryption".to_string());
        m
    }
    fn search_fields() -> &'static [&'static str] {
        &["name", "smtp_host"]
    }
}

/// Builder for MailServer entity
///
/// Provides a fluent API for constructing MailServer instances.
/// System fields (id, metadata, timestamps) are auto-initialized.
#[derive(Debug, Clone, Default)]
pub struct MailServerBuilder {
    name: Option<String>,
    from_filter: Option<String>,
    smtp_host: Option<String>,
    smtp_port: Option<i32>,
    smtp_authentication: Option<SmtpAuthentication>,
    smtp_user: Option<String>,
    smtp_pass_ref: Option<String>,
    smtp_encryption: Option<SmtpEncryption>,
    smtp_debug: Option<bool>,
    sequence: Option<i32>,
    active: Option<bool>,
}

impl MailServerBuilder {
    /// Set the name field (required)
    pub fn name(mut self, value: String) -> Self {
        self.name = Some(value);
        self
    }

    /// Set the from_filter field (optional)
    pub fn from_filter(mut self, value: String) -> Self {
        self.from_filter = Some(value);
        self
    }

    /// Set the smtp_host field (required)
    pub fn smtp_host(mut self, value: String) -> Self {
        self.smtp_host = Some(value);
        self
    }

    /// Set the smtp_port field (default: `25`)
    pub fn smtp_port(mut self, value: i32) -> Self {
        self.smtp_port = Some(value);
        self
    }

    /// Set the smtp_authentication field (default: `SmtpAuthentication::default()`)
    pub fn smtp_authentication(mut self, value: SmtpAuthentication) -> Self {
        self.smtp_authentication = Some(value);
        self
    }

    /// Set the smtp_user field (optional)
    pub fn smtp_user(mut self, value: String) -> Self {
        self.smtp_user = Some(value);
        self
    }

    /// Set the smtp_pass_ref field (optional)
    pub fn smtp_pass_ref(mut self, value: String) -> Self {
        self.smtp_pass_ref = Some(value);
        self
    }

    /// Set the smtp_encryption field (default: `SmtpEncryption::default()`)
    pub fn smtp_encryption(mut self, value: SmtpEncryption) -> Self {
        self.smtp_encryption = Some(value);
        self
    }

    /// Set the smtp_debug field (default: `false`)
    pub fn smtp_debug(mut self, value: bool) -> Self {
        self.smtp_debug = Some(value);
        self
    }

    /// Set the sequence field (default: `10`)
    pub fn sequence(mut self, value: i32) -> Self {
        self.sequence = Some(value);
        self
    }

    /// Set the active field (default: `true`)
    pub fn active(mut self, value: bool) -> Self {
        self.active = Some(value);
        self
    }

    /// Build the MailServer entity
    ///
    /// Returns Err if any required field without a default is missing.
    pub fn build(self) -> Result<MailServer, String> {
        let name = self.name.ok_or_else(|| "name is required".to_string())?;
        let smtp_host = self.smtp_host.ok_or_else(|| "smtp_host is required".to_string())?;

        Ok(MailServer {
            id: Uuid::new_v4(),
            name,
            from_filter: self.from_filter,
            smtp_host,
            smtp_port: self.smtp_port.unwrap_or(25),
            smtp_authentication: self.smtp_authentication.unwrap_or_default(),
            smtp_user: self.smtp_user,
            smtp_pass_ref: self.smtp_pass_ref,
            smtp_encryption: self.smtp_encryption.unwrap_or_default(),
            smtp_debug: self.smtp_debug.unwrap_or(false),
            sequence: self.sequence.unwrap_or(10),
            active: self.active.unwrap_or(true),
            metadata: AuditMetadata::default(),
        })
    }
}
