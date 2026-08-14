use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use uuid::Uuid;
use super::AuditMetadata;

/// Strongly-typed ID for MailAliasDomain
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct MailAliasDomainId(pub Uuid);

impl MailAliasDomainId {
    pub fn new(id: Uuid) -> Self { Self(id) }
    pub fn generate() -> Self { Self(Uuid::new_v4()) }
    pub fn into_inner(self) -> Uuid { self.0 }
}

impl std::fmt::Display for MailAliasDomainId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::str::FromStr for MailAliasDomainId {
    type Err = uuid::Error;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(Self(Uuid::parse_str(s)?))
    }
}

impl From<Uuid> for MailAliasDomainId {
    fn from(id: Uuid) -> Self { Self(id) }
}

impl From<MailAliasDomainId> for Uuid {
    fn from(id: MailAliasDomainId) -> Self { id.0 }
}

impl AsRef<Uuid> for MailAliasDomainId {
    fn as_ref(&self) -> &Uuid { &self.0 }
}

impl std::ops::Deref for MailAliasDomainId {
    type Target = Uuid;
    fn deref(&self) -> &Self::Target { &self.0 }
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct MailAliasDomain {
    pub id: Uuid,
    pub name: String,
    pub catchall_alias: Option<String>,
    pub bounce_alias: Option<String>,
    pub bounce_alias_email: Option<String>,
    pub catchall_alias_email: Option<String>,
    pub use_mx: bool,
    pub mail_server_id: Option<Uuid>,
    pub from_filter: Option<String>,
    #[serde(default)]
    #[sqlx(json)]
    pub metadata: AuditMetadata,
}

impl MailAliasDomain {
    /// Create a builder for MailAliasDomain
    pub fn builder() -> MailAliasDomainBuilder {
        MailAliasDomainBuilder::default()
    }

    /// Create a new MailAliasDomain with required fields
    pub fn new(name: String, use_mx: bool) -> Self {
        Self {
            id: Uuid::new_v4(),
            name,
            catchall_alias: None,
            bounce_alias: None,
            bounce_alias_email: None,
            catchall_alias_email: None,
            use_mx,
            mail_server_id: None,
            from_filter: None,
            metadata: AuditMetadata::default(),
        }
    }

    /// Get the entity's unique identifier
    pub fn id(&self) -> &Uuid {
        &self.id
    }

    /// Get a strongly-typed ID for this entity
    pub fn typed_id(&self) -> MailAliasDomainId {
        MailAliasDomainId(self.id)
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

    /// Set the catchall_alias field (chainable)
    pub fn with_catchall_alias(mut self, value: String) -> Self {
        self.catchall_alias = Some(value);
        self
    }

    /// Set the bounce_alias field (chainable)
    pub fn with_bounce_alias(mut self, value: String) -> Self {
        self.bounce_alias = Some(value);
        self
    }

    /// Set the bounce_alias_email field (chainable)
    pub fn with_bounce_alias_email(mut self, value: String) -> Self {
        self.bounce_alias_email = Some(value);
        self
    }

    /// Set the catchall_alias_email field (chainable)
    pub fn with_catchall_alias_email(mut self, value: String) -> Self {
        self.catchall_alias_email = Some(value);
        self
    }

    /// Set the mail_server_id field (chainable)
    pub fn with_mail_server_id(mut self, value: Uuid) -> Self {
        self.mail_server_id = Some(value);
        self
    }

    /// Set the from_filter field (chainable)
    pub fn with_from_filter(mut self, value: String) -> Self {
        self.from_filter = Some(value);
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
                "catchall_alias" => {
                    if let Ok(v) = serde_json::from_value(value) { self.catchall_alias = v; }
                }
                "bounce_alias" => {
                    if let Ok(v) = serde_json::from_value(value) { self.bounce_alias = v; }
                }
                "bounce_alias_email" => {
                    if let Ok(v) = serde_json::from_value(value) { self.bounce_alias_email = v; }
                }
                "catchall_alias_email" => {
                    if let Ok(v) = serde_json::from_value(value) { self.catchall_alias_email = v; }
                }
                "use_mx" => {
                    if let Ok(v) = serde_json::from_value(value) { self.use_mx = v; }
                }
                "mail_server_id" => {
                    if let Ok(v) = serde_json::from_value(value) { self.mail_server_id = v; }
                }
                "from_filter" => {
                    if let Ok(v) = serde_json::from_value(value) { self.from_filter = v; }
                }
                _ => {} // ignore unknown fields
            }
        }
    }

    // <<< CUSTOM METHODS START >>>
    // <<< CUSTOM METHODS END >>>
}

impl super::Entity for MailAliasDomain {
    type Id = Uuid;

    fn entity_id(&self) -> &Self::Id {
        &self.id
    }

    fn entity_type() -> &'static str {
        "MailAliasDomain"
    }
}

impl backbone_core::PersistentEntity for MailAliasDomain {
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

impl backbone_orm::EntityRepoMeta for MailAliasDomain {
    fn column_types() -> std::collections::HashMap<String, String> {
        let mut m = std::collections::HashMap::new();
        m.insert("id".to_string(), "uuid".to_string());
        m.insert("mail_server_id".to_string(), "uuid".to_string());
        m
    }
    fn search_fields() -> &'static [&'static str] {
        &["name"]
    }
}

/// Builder for MailAliasDomain entity
///
/// Provides a fluent API for constructing MailAliasDomain instances.
/// System fields (id, metadata, timestamps) are auto-initialized.
#[derive(Debug, Clone, Default)]
pub struct MailAliasDomainBuilder {
    name: Option<String>,
    catchall_alias: Option<String>,
    bounce_alias: Option<String>,
    bounce_alias_email: Option<String>,
    catchall_alias_email: Option<String>,
    use_mx: Option<bool>,
    mail_server_id: Option<Uuid>,
    from_filter: Option<String>,
}

impl MailAliasDomainBuilder {
    /// Set the name field (required)
    pub fn name(mut self, value: String) -> Self {
        self.name = Some(value);
        self
    }

    /// Set the catchall_alias field (optional)
    pub fn catchall_alias(mut self, value: String) -> Self {
        self.catchall_alias = Some(value);
        self
    }

    /// Set the bounce_alias field (optional)
    pub fn bounce_alias(mut self, value: String) -> Self {
        self.bounce_alias = Some(value);
        self
    }

    /// Set the bounce_alias_email field (optional)
    pub fn bounce_alias_email(mut self, value: String) -> Self {
        self.bounce_alias_email = Some(value);
        self
    }

    /// Set the catchall_alias_email field (optional)
    pub fn catchall_alias_email(mut self, value: String) -> Self {
        self.catchall_alias_email = Some(value);
        self
    }

    /// Set the use_mx field (default: `false`)
    pub fn use_mx(mut self, value: bool) -> Self {
        self.use_mx = Some(value);
        self
    }

    /// Set the mail_server_id field (optional)
    pub fn mail_server_id(mut self, value: Uuid) -> Self {
        self.mail_server_id = Some(value);
        self
    }

    /// Set the from_filter field (optional)
    pub fn from_filter(mut self, value: String) -> Self {
        self.from_filter = Some(value);
        self
    }

    /// Build the MailAliasDomain entity
    ///
    /// Returns Err if any required field without a default is missing.
    pub fn build(self) -> Result<MailAliasDomain, String> {
        let name = self.name.ok_or_else(|| "name is required".to_string())?;

        Ok(MailAliasDomain {
            id: Uuid::new_v4(),
            name,
            catchall_alias: self.catchall_alias,
            bounce_alias: self.bounce_alias,
            bounce_alias_email: self.bounce_alias_email,
            catchall_alias_email: self.catchall_alias_email,
            use_mx: self.use_mx.unwrap_or(false),
            mail_server_id: self.mail_server_id,
            from_filter: self.from_filter,
            metadata: AuditMetadata::default(),
        })
    }
}
