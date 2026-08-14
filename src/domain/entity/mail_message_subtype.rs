use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use uuid::Uuid;
use super::AuditMetadata;

/// Strongly-typed ID for MailMessageSubtype
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct MailMessageSubtypeId(pub Uuid);

impl MailMessageSubtypeId {
    pub fn new(id: Uuid) -> Self { Self(id) }
    pub fn generate() -> Self { Self(Uuid::new_v4()) }
    pub fn into_inner(self) -> Uuid { self.0 }
}

impl std::fmt::Display for MailMessageSubtypeId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::str::FromStr for MailMessageSubtypeId {
    type Err = uuid::Error;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(Self(Uuid::parse_str(s)?))
    }
}

impl From<Uuid> for MailMessageSubtypeId {
    fn from(id: Uuid) -> Self { Self(id) }
}

impl From<MailMessageSubtypeId> for Uuid {
    fn from(id: MailMessageSubtypeId) -> Self { id.0 }
}

impl AsRef<Uuid> for MailMessageSubtypeId {
    fn as_ref(&self) -> &Uuid { &self.0 }
}

impl std::ops::Deref for MailMessageSubtypeId {
    type Target = Uuid;
    fn deref(&self) -> &Self::Target { &self.0 }
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct MailMessageSubtype {
    pub id: Uuid,
    pub name: String,
    pub description: Option<String>,
    pub internal: bool,
    pub parent_id: Option<Uuid>,
    pub relation_field: Option<String>,
    pub res_model: Option<String>,
    pub default: bool,
    pub hidden: bool,
    pub tracked: bool,
    #[serde(default)]
    #[sqlx(json)]
    pub metadata: AuditMetadata,
}

impl MailMessageSubtype {
    /// Create a builder for MailMessageSubtype
    pub fn builder() -> MailMessageSubtypeBuilder {
        MailMessageSubtypeBuilder::default()
    }

    /// Create a new MailMessageSubtype with required fields
    pub fn new(name: String, internal: bool, default: bool, hidden: bool, tracked: bool) -> Self {
        Self {
            id: Uuid::new_v4(),
            name,
            description: None,
            internal,
            parent_id: None,
            relation_field: None,
            res_model: None,
            default,
            hidden,
            tracked,
            metadata: AuditMetadata::default(),
        }
    }

    /// Get the entity's unique identifier
    pub fn id(&self) -> &Uuid {
        &self.id
    }

    /// Get a strongly-typed ID for this entity
    pub fn typed_id(&self) -> MailMessageSubtypeId {
        MailMessageSubtypeId(self.id)
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

    /// Set the description field (chainable)
    pub fn with_description(mut self, value: String) -> Self {
        self.description = Some(value);
        self
    }

    /// Set the parent_id field (chainable)
    pub fn with_parent_id(mut self, value: Uuid) -> Self {
        self.parent_id = Some(value);
        self
    }

    /// Set the relation_field field (chainable)
    pub fn with_relation_field(mut self, value: String) -> Self {
        self.relation_field = Some(value);
        self
    }

    /// Set the res_model field (chainable)
    pub fn with_res_model(mut self, value: String) -> Self {
        self.res_model = Some(value);
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
                "description" => {
                    if let Ok(v) = serde_json::from_value(value) { self.description = v; }
                }
                "internal" => {
                    if let Ok(v) = serde_json::from_value(value) { self.internal = v; }
                }
                "parent_id" => {
                    if let Ok(v) = serde_json::from_value(value) { self.parent_id = v; }
                }
                "relation_field" => {
                    if let Ok(v) = serde_json::from_value(value) { self.relation_field = v; }
                }
                "res_model" => {
                    if let Ok(v) = serde_json::from_value(value) { self.res_model = v; }
                }
                "default" => {
                    if let Ok(v) = serde_json::from_value(value) { self.default = v; }
                }
                "hidden" => {
                    if let Ok(v) = serde_json::from_value(value) { self.hidden = v; }
                }
                "tracked" => {
                    if let Ok(v) = serde_json::from_value(value) { self.tracked = v; }
                }
                _ => {} // ignore unknown fields
            }
        }
    }

    // <<< CUSTOM METHODS START >>>
    // <<< CUSTOM METHODS END >>>
}

impl super::Entity for MailMessageSubtype {
    type Id = Uuid;

    fn entity_id(&self) -> &Self::Id {
        &self.id
    }

    fn entity_type() -> &'static str {
        "MailMessageSubtype"
    }
}

impl backbone_core::PersistentEntity for MailMessageSubtype {
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

impl backbone_orm::EntityRepoMeta for MailMessageSubtype {
    fn column_types() -> std::collections::HashMap<String, String> {
        let mut m = std::collections::HashMap::new();
        m.insert("id".to_string(), "uuid".to_string());
        m.insert("parent_id".to_string(), "uuid".to_string());
        m
    }
    fn search_fields() -> &'static [&'static str] {
        &["name"]
    }
}

/// Builder for MailMessageSubtype entity
///
/// Provides a fluent API for constructing MailMessageSubtype instances.
/// System fields (id, metadata, timestamps) are auto-initialized.
#[derive(Debug, Clone, Default)]
pub struct MailMessageSubtypeBuilder {
    name: Option<String>,
    description: Option<String>,
    internal: Option<bool>,
    parent_id: Option<Uuid>,
    relation_field: Option<String>,
    res_model: Option<String>,
    default: Option<bool>,
    hidden: Option<bool>,
    tracked: Option<bool>,
}

impl MailMessageSubtypeBuilder {
    /// Set the name field (required)
    pub fn name(mut self, value: String) -> Self {
        self.name = Some(value);
        self
    }

    /// Set the description field (optional)
    pub fn description(mut self, value: String) -> Self {
        self.description = Some(value);
        self
    }

    /// Set the internal field (default: `false`)
    pub fn internal(mut self, value: bool) -> Self {
        self.internal = Some(value);
        self
    }

    /// Set the parent_id field (optional)
    pub fn parent_id(mut self, value: Uuid) -> Self {
        self.parent_id = Some(value);
        self
    }

    /// Set the relation_field field (optional)
    pub fn relation_field(mut self, value: String) -> Self {
        self.relation_field = Some(value);
        self
    }

    /// Set the res_model field (optional)
    pub fn res_model(mut self, value: String) -> Self {
        self.res_model = Some(value);
        self
    }

    /// Set the default field (default: `false`)
    pub fn default(mut self, value: bool) -> Self {
        self.default = Some(value);
        self
    }

    /// Set the hidden field (default: `false`)
    pub fn hidden(mut self, value: bool) -> Self {
        self.hidden = Some(value);
        self
    }

    /// Set the tracked field (default: `false`)
    pub fn tracked(mut self, value: bool) -> Self {
        self.tracked = Some(value);
        self
    }

    /// Build the MailMessageSubtype entity
    ///
    /// Returns Err if any required field without a default is missing.
    pub fn build(self) -> Result<MailMessageSubtype, String> {
        let name = self.name.ok_or_else(|| "name is required".to_string())?;

        Ok(MailMessageSubtype {
            id: Uuid::new_v4(),
            name,
            description: self.description,
            internal: self.internal.unwrap_or(false),
            parent_id: self.parent_id,
            relation_field: self.relation_field,
            res_model: self.res_model,
            default: self.default.unwrap_or(false),
            hidden: self.hidden.unwrap_or(false),
            tracked: self.tracked.unwrap_or(false),
            metadata: AuditMetadata::default(),
        })
    }
}
