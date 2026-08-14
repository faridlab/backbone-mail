use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use uuid::Uuid;
use super::AuditMetadata;

/// Strongly-typed ID for MailGuest
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct MailGuestId(pub Uuid);

impl MailGuestId {
    pub fn new(id: Uuid) -> Self { Self(id) }
    pub fn generate() -> Self { Self(Uuid::new_v4()) }
    pub fn into_inner(self) -> Uuid { self.0 }
}

impl std::fmt::Display for MailGuestId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::str::FromStr for MailGuestId {
    type Err = uuid::Error;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(Self(Uuid::parse_str(s)?))
    }
}

impl From<Uuid> for MailGuestId {
    fn from(id: Uuid) -> Self { Self(id) }
}

impl From<MailGuestId> for Uuid {
    fn from(id: MailGuestId) -> Self { id.0 }
}

impl AsRef<Uuid> for MailGuestId {
    fn as_ref(&self) -> &Uuid { &self.0 }
}

impl std::ops::Deref for MailGuestId {
    type Target = Uuid;
    fn deref(&self) -> &Self::Target { &self.0 }
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct MailGuest {
    pub id: Uuid,
    pub name: String,
    pub country_id: Option<Uuid>,
    pub last_connection_dt: Option<DateTime<Utc>>,
    pub active: bool,
    #[serde(default)]
    #[sqlx(json)]
    pub metadata: AuditMetadata,
}

impl MailGuest {
    /// Create a builder for MailGuest
    pub fn builder() -> MailGuestBuilder {
        MailGuestBuilder::default()
    }

    /// Create a new MailGuest with required fields
    pub fn new(name: String, active: bool) -> Self {
        Self {
            id: Uuid::new_v4(),
            name,
            country_id: None,
            last_connection_dt: None,
            active,
            metadata: AuditMetadata::default(),
        }
    }

    /// Get the entity's unique identifier
    pub fn id(&self) -> &Uuid {
        &self.id
    }

    /// Get a strongly-typed ID for this entity
    pub fn typed_id(&self) -> MailGuestId {
        MailGuestId(self.id)
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

    /// Set the country_id field (chainable)
    pub fn with_country_id(mut self, value: Uuid) -> Self {
        self.country_id = Some(value);
        self
    }

    /// Set the last_connection_dt field (chainable)
    pub fn with_last_connection_dt(mut self, value: DateTime<Utc>) -> Self {
        self.last_connection_dt = Some(value);
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
                "country_id" => {
                    if let Ok(v) = serde_json::from_value(value) { self.country_id = v; }
                }
                "last_connection_dt" => {
                    if let Ok(v) = serde_json::from_value(value) { self.last_connection_dt = v; }
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

impl super::Entity for MailGuest {
    type Id = Uuid;

    fn entity_id(&self) -> &Self::Id {
        &self.id
    }

    fn entity_type() -> &'static str {
        "MailGuest"
    }
}

impl backbone_core::PersistentEntity for MailGuest {
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

impl backbone_orm::EntityRepoMeta for MailGuest {
    fn column_types() -> std::collections::HashMap<String, String> {
        let mut m = std::collections::HashMap::new();
        m.insert("id".to_string(), "uuid".to_string());
        m.insert("country_id".to_string(), "uuid".to_string());
        m
    }
    fn search_fields() -> &'static [&'static str] {
        &["name"]
    }
}

/// Builder for MailGuest entity
///
/// Provides a fluent API for constructing MailGuest instances.
/// System fields (id, metadata, timestamps) are auto-initialized.
#[derive(Debug, Clone, Default)]
pub struct MailGuestBuilder {
    name: Option<String>,
    country_id: Option<Uuid>,
    last_connection_dt: Option<DateTime<Utc>>,
    active: Option<bool>,
}

impl MailGuestBuilder {
    /// Set the name field (required)
    pub fn name(mut self, value: String) -> Self {
        self.name = Some(value);
        self
    }

    /// Set the country_id field (optional)
    pub fn country_id(mut self, value: Uuid) -> Self {
        self.country_id = Some(value);
        self
    }

    /// Set the last_connection_dt field (optional)
    pub fn last_connection_dt(mut self, value: DateTime<Utc>) -> Self {
        self.last_connection_dt = Some(value);
        self
    }

    /// Set the active field (default: `true`)
    pub fn active(mut self, value: bool) -> Self {
        self.active = Some(value);
        self
    }

    /// Build the MailGuest entity
    ///
    /// Returns Err if any required field without a default is missing.
    pub fn build(self) -> Result<MailGuest, String> {
        let name = self.name.ok_or_else(|| "name is required".to_string())?;

        Ok(MailGuest {
            id: Uuid::new_v4(),
            name,
            country_id: self.country_id,
            last_connection_dt: self.last_connection_dt,
            active: self.active.unwrap_or(true),
            metadata: AuditMetadata::default(),
        })
    }
}
