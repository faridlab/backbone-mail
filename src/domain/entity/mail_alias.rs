use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use uuid::Uuid;

use super::MailAliasContact;
use super::AuditMetadata;

/// Strongly-typed ID for MailAlias
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct MailAliasId(pub Uuid);

impl MailAliasId {
    pub fn new(id: Uuid) -> Self { Self(id) }
    pub fn generate() -> Self { Self(Uuid::new_v4()) }
    pub fn into_inner(self) -> Uuid { self.0 }
}

impl std::fmt::Display for MailAliasId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::str::FromStr for MailAliasId {
    type Err = uuid::Error;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(Self(Uuid::parse_str(s)?))
    }
}

impl From<Uuid> for MailAliasId {
    fn from(id: Uuid) -> Self { Self(id) }
}

impl From<MailAliasId> for Uuid {
    fn from(id: MailAliasId) -> Self { id.0 }
}

impl AsRef<Uuid> for MailAliasId {
    fn as_ref(&self) -> &Uuid { &self.0 }
}

impl std::ops::Deref for MailAliasId {
    type Target = Uuid;
    fn deref(&self) -> &Self::Target { &self.0 }
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct MailAlias {
    pub id: Uuid,
    pub alias_name: Option<String>,
    pub alias_domain_id: Option<Uuid>,
    pub alias_contact: MailAliasContact,
    pub alias_model_id: Option<Uuid>,
    pub alias_parent_model_id: Option<Uuid>,
    pub alias_parent_thread_id: Option<Uuid>,
    pub alias_user_id: Option<Uuid>,
    pub alias_defaults: Option<String>,
    pub alias_force_thread_id: Option<Uuid>,
    pub alias_reply_to_address: Option<String>,
    #[serde(default)]
    #[sqlx(json)]
    pub metadata: AuditMetadata,
}

impl MailAlias {
    /// Create a builder for MailAlias
    pub fn builder() -> MailAliasBuilder {
        MailAliasBuilder::default()
    }

    /// Create a new MailAlias with required fields
    pub fn new(alias_contact: MailAliasContact) -> Self {
        Self {
            id: Uuid::new_v4(),
            alias_name: None,
            alias_domain_id: None,
            alias_contact,
            alias_model_id: None,
            alias_parent_model_id: None,
            alias_parent_thread_id: None,
            alias_user_id: None,
            alias_defaults: None,
            alias_force_thread_id: None,
            alias_reply_to_address: None,
            metadata: AuditMetadata::default(),
        }
    }

    /// Get the entity's unique identifier
    pub fn id(&self) -> &Uuid {
        &self.id
    }

    /// Get a strongly-typed ID for this entity
    pub fn typed_id(&self) -> MailAliasId {
        MailAliasId(self.id)
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

    /// Set the alias_name field (chainable)
    pub fn with_alias_name(mut self, value: String) -> Self {
        self.alias_name = Some(value);
        self
    }

    /// Set the alias_domain_id field (chainable)
    pub fn with_alias_domain_id(mut self, value: Uuid) -> Self {
        self.alias_domain_id = Some(value);
        self
    }

    /// Set the alias_model_id field (chainable)
    pub fn with_alias_model_id(mut self, value: Uuid) -> Self {
        self.alias_model_id = Some(value);
        self
    }

    /// Set the alias_parent_model_id field (chainable)
    pub fn with_alias_parent_model_id(mut self, value: Uuid) -> Self {
        self.alias_parent_model_id = Some(value);
        self
    }

    /// Set the alias_parent_thread_id field (chainable)
    pub fn with_alias_parent_thread_id(mut self, value: Uuid) -> Self {
        self.alias_parent_thread_id = Some(value);
        self
    }

    /// Set the alias_user_id field (chainable)
    pub fn with_alias_user_id(mut self, value: Uuid) -> Self {
        self.alias_user_id = Some(value);
        self
    }

    /// Set the alias_defaults field (chainable)
    pub fn with_alias_defaults(mut self, value: String) -> Self {
        self.alias_defaults = Some(value);
        self
    }

    /// Set the alias_force_thread_id field (chainable)
    pub fn with_alias_force_thread_id(mut self, value: Uuid) -> Self {
        self.alias_force_thread_id = Some(value);
        self
    }

    /// Set the alias_reply_to_address field (chainable)
    pub fn with_alias_reply_to_address(mut self, value: String) -> Self {
        self.alias_reply_to_address = Some(value);
        self
    }

    // ==========================================================
    // Partial Update
    // ==========================================================

    /// Apply partial updates from a map of field name to JSON value
    pub fn apply_patch(&mut self, fields: std::collections::HashMap<String, serde_json::Value>) {
        for (key, value) in fields {
            match key.as_str() {
                "alias_name" => {
                    if let Ok(v) = serde_json::from_value(value) { self.alias_name = v; }
                }
                "alias_domain_id" => {
                    if let Ok(v) = serde_json::from_value(value) { self.alias_domain_id = v; }
                }
                "alias_contact" => {
                    if let Ok(v) = serde_json::from_value(value) { self.alias_contact = v; }
                }
                "alias_model_id" => {
                    if let Ok(v) = serde_json::from_value(value) { self.alias_model_id = v; }
                }
                "alias_parent_model_id" => {
                    if let Ok(v) = serde_json::from_value(value) { self.alias_parent_model_id = v; }
                }
                "alias_parent_thread_id" => {
                    if let Ok(v) = serde_json::from_value(value) { self.alias_parent_thread_id = v; }
                }
                "alias_user_id" => {
                    if let Ok(v) = serde_json::from_value(value) { self.alias_user_id = v; }
                }
                "alias_defaults" => {
                    if let Ok(v) = serde_json::from_value(value) { self.alias_defaults = v; }
                }
                "alias_force_thread_id" => {
                    if let Ok(v) = serde_json::from_value(value) { self.alias_force_thread_id = v; }
                }
                "alias_reply_to_address" => {
                    if let Ok(v) = serde_json::from_value(value) { self.alias_reply_to_address = v; }
                }
                _ => {} // ignore unknown fields
            }
        }
    }

    // <<< CUSTOM METHODS START >>>
    // <<< CUSTOM METHODS END >>>
}

impl super::Entity for MailAlias {
    type Id = Uuid;

    fn entity_id(&self) -> &Self::Id {
        &self.id
    }

    fn entity_type() -> &'static str {
        "MailAlias"
    }
}

impl backbone_core::PersistentEntity for MailAlias {
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

impl backbone_orm::EntityRepoMeta for MailAlias {
    fn column_types() -> std::collections::HashMap<String, String> {
        let mut m = std::collections::HashMap::new();
        m.insert("id".to_string(), "uuid".to_string());
        m.insert("alias_domain_id".to_string(), "uuid".to_string());
        m.insert("alias_model_id".to_string(), "uuid".to_string());
        m.insert("alias_parent_model_id".to_string(), "uuid".to_string());
        m.insert("alias_parent_thread_id".to_string(), "uuid".to_string());
        m.insert("alias_user_id".to_string(), "uuid".to_string());
        m.insert("alias_force_thread_id".to_string(), "uuid".to_string());
        m.insert("alias_contact".to_string(), "mail_alias_contact".to_string());
        m
    }
    fn search_fields() -> &'static [&'static str] {
        &[]
    }
}

/// Builder for MailAlias entity
///
/// Provides a fluent API for constructing MailAlias instances.
/// System fields (id, metadata, timestamps) are auto-initialized.
#[derive(Debug, Clone, Default)]
pub struct MailAliasBuilder {
    alias_name: Option<String>,
    alias_domain_id: Option<Uuid>,
    alias_contact: Option<MailAliasContact>,
    alias_model_id: Option<Uuid>,
    alias_parent_model_id: Option<Uuid>,
    alias_parent_thread_id: Option<Uuid>,
    alias_user_id: Option<Uuid>,
    alias_defaults: Option<String>,
    alias_force_thread_id: Option<Uuid>,
    alias_reply_to_address: Option<String>,
}

impl MailAliasBuilder {
    /// Set the alias_name field (optional)
    pub fn alias_name(mut self, value: String) -> Self {
        self.alias_name = Some(value);
        self
    }

    /// Set the alias_domain_id field (optional)
    pub fn alias_domain_id(mut self, value: Uuid) -> Self {
        self.alias_domain_id = Some(value);
        self
    }

    /// Set the alias_contact field (default: `MailAliasContact::default()`)
    pub fn alias_contact(mut self, value: MailAliasContact) -> Self {
        self.alias_contact = Some(value);
        self
    }

    /// Set the alias_model_id field (optional)
    pub fn alias_model_id(mut self, value: Uuid) -> Self {
        self.alias_model_id = Some(value);
        self
    }

    /// Set the alias_parent_model_id field (optional)
    pub fn alias_parent_model_id(mut self, value: Uuid) -> Self {
        self.alias_parent_model_id = Some(value);
        self
    }

    /// Set the alias_parent_thread_id field (optional)
    pub fn alias_parent_thread_id(mut self, value: Uuid) -> Self {
        self.alias_parent_thread_id = Some(value);
        self
    }

    /// Set the alias_user_id field (optional)
    pub fn alias_user_id(mut self, value: Uuid) -> Self {
        self.alias_user_id = Some(value);
        self
    }

    /// Set the alias_defaults field (optional)
    pub fn alias_defaults(mut self, value: String) -> Self {
        self.alias_defaults = Some(value);
        self
    }

    /// Set the alias_force_thread_id field (optional)
    pub fn alias_force_thread_id(mut self, value: Uuid) -> Self {
        self.alias_force_thread_id = Some(value);
        self
    }

    /// Set the alias_reply_to_address field (optional)
    pub fn alias_reply_to_address(mut self, value: String) -> Self {
        self.alias_reply_to_address = Some(value);
        self
    }

    /// Build the MailAlias entity
    ///
    /// Returns Err if any required field without a default is missing.
    pub fn build(self) -> Result<MailAlias, String> {

        Ok(MailAlias {
            id: Uuid::new_v4(),
            alias_name: self.alias_name,
            alias_domain_id: self.alias_domain_id,
            alias_contact: self.alias_contact.unwrap_or(MailAliasContact::default()),
            alias_model_id: self.alias_model_id,
            alias_parent_model_id: self.alias_parent_model_id,
            alias_parent_thread_id: self.alias_parent_thread_id,
            alias_user_id: self.alias_user_id,
            alias_defaults: self.alias_defaults,
            alias_force_thread_id: self.alias_force_thread_id,
            alias_reply_to_address: self.alias_reply_to_address,
            metadata: AuditMetadata::default(),
        })
    }
}
