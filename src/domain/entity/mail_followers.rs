use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use uuid::Uuid;
use super::AuditMetadata;

/// Strongly-typed ID for MailFollowers
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct MailFollowersId(pub Uuid);

impl MailFollowersId {
    pub fn new(id: Uuid) -> Self { Self(id) }
    pub fn generate() -> Self { Self(Uuid::new_v4()) }
    pub fn into_inner(self) -> Uuid { self.0 }
}

impl std::fmt::Display for MailFollowersId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::str::FromStr for MailFollowersId {
    type Err = uuid::Error;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(Self(Uuid::parse_str(s)?))
    }
}

impl From<Uuid> for MailFollowersId {
    fn from(id: Uuid) -> Self { Self(id) }
}

impl From<MailFollowersId> for Uuid {
    fn from(id: MailFollowersId) -> Self { id.0 }
}

impl AsRef<Uuid> for MailFollowersId {
    fn as_ref(&self) -> &Uuid { &self.0 }
}

impl std::ops::Deref for MailFollowersId {
    type Target = Uuid;
    fn deref(&self) -> &Self::Target { &self.0 }
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct MailFollowers {
    pub id: Uuid,
    pub res_model: String,
    pub res_id: Uuid,
    pub partner_id: Option<Uuid>,
    pub channel_id: Option<Uuid>,
    pub subtype_ids: Option<serde_json::Value>,
    #[serde(default)]
    #[sqlx(json)]
    pub metadata: AuditMetadata,
}

impl MailFollowers {
    /// Create a builder for MailFollowers
    pub fn builder() -> MailFollowersBuilder {
        <MailFollowersBuilder as Default>::default()
    }

    /// Create a new MailFollowers with required fields
    pub fn new(res_model: String, res_id: Uuid) -> Self {
        Self {
            id: Uuid::new_v4(),
            res_model,
            res_id,
            partner_id: None,
            channel_id: None,
            subtype_ids: None,
            metadata: AuditMetadata::default(),
        }
    }

    /// Get the entity's unique identifier
    pub fn id(&self) -> &Uuid {
        &self.id
    }

    /// Get a strongly-typed ID for this entity
    pub fn typed_id(&self) -> MailFollowersId {
        MailFollowersId(self.id)
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

    /// Set the partner_id field (chainable)
    pub fn with_partner_id(mut self, value: Uuid) -> Self {
        self.partner_id = Some(value);
        self
    }

    /// Set the channel_id field (chainable)
    pub fn with_channel_id(mut self, value: Uuid) -> Self {
        self.channel_id = Some(value);
        self
    }

    /// Set the subtype_ids field (chainable)
    pub fn with_subtype_ids(mut self, value: serde_json::Value) -> Self {
        self.subtype_ids = Some(value);
        self
    }

    // ==========================================================
    // Partial Update
    // ==========================================================

    /// Apply partial updates from a map of field name to JSON value
    pub fn apply_patch(&mut self, fields: std::collections::HashMap<String, serde_json::Value>) {
        for (key, value) in fields {
            match key.as_str() {
                "res_model" => {
                    if let Ok(v) = serde_json::from_value(value) { self.res_model = v; }
                }
                "res_id" => {
                    if let Ok(v) = serde_json::from_value(value) { self.res_id = v; }
                }
                "partner_id" => {
                    if let Ok(v) = serde_json::from_value(value) { self.partner_id = v; }
                }
                "channel_id" => {
                    if let Ok(v) = serde_json::from_value(value) { self.channel_id = v; }
                }
                "subtype_ids" => {
                    if let Ok(v) = serde_json::from_value(value) { self.subtype_ids = v; }
                }
                _ => {} // ignore unknown fields
            }
        }
    }

    // <<< CUSTOM METHODS START >>>
    // <<< CUSTOM METHODS END >>>
}

impl super::Entity for MailFollowers {
    type Id = Uuid;

    fn entity_id(&self) -> &Self::Id {
        &self.id
    }

    fn entity_type() -> &'static str {
        "MailFollowers"
    }
}

impl backbone_core::PersistentEntity for MailFollowers {
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

impl backbone_orm::EntityRepoMeta for MailFollowers {
    fn column_types() -> std::collections::HashMap<String, String> {
        let mut m = std::collections::HashMap::new();
        m.insert("id".to_string(), "uuid".to_string());
        m.insert("res_id".to_string(), "uuid".to_string());
        m.insert("partner_id".to_string(), "uuid".to_string());
        m.insert("channel_id".to_string(), "uuid".to_string());
        m
    }
    fn search_fields() -> &'static [&'static str] {
        &["res_model"]
    }
}

/// Builder for MailFollowers entity
///
/// Provides a fluent API for constructing MailFollowers instances.
/// System fields (id, metadata, timestamps) are auto-initialized.
#[derive(Debug, Clone, Default)]
pub struct MailFollowersBuilder {
    res_model: Option<String>,
    res_id: Option<Uuid>,
    partner_id: Option<Uuid>,
    channel_id: Option<Uuid>,
    subtype_ids: Option<serde_json::Value>,
}

impl MailFollowersBuilder {
    /// Set the res_model field (required)
    pub fn res_model(mut self, value: String) -> Self {
        self.res_model = Some(value);
        self
    }

    /// Set the res_id field (required)
    pub fn res_id(mut self, value: Uuid) -> Self {
        self.res_id = Some(value);
        self
    }

    /// Set the partner_id field (optional)
    pub fn partner_id(mut self, value: Uuid) -> Self {
        self.partner_id = Some(value);
        self
    }

    /// Set the channel_id field (optional)
    pub fn channel_id(mut self, value: Uuid) -> Self {
        self.channel_id = Some(value);
        self
    }

    /// Set the subtype_ids field (optional)
    pub fn subtype_ids(mut self, value: serde_json::Value) -> Self {
        self.subtype_ids = Some(value);
        self
    }

    /// Build the MailFollowers entity
    ///
    /// Returns Err if any required field without a default is missing.
    pub fn build(self) -> Result<MailFollowers, String> {
        let res_model = self.res_model.ok_or_else(|| "res_model is required".to_string())?;
        let res_id = self.res_id.ok_or_else(|| "res_id is required".to_string())?;

        Ok(MailFollowers {
            id: Uuid::new_v4(),
            res_model,
            res_id,
            partner_id: self.partner_id,
            channel_id: self.channel_id,
            subtype_ids: self.subtype_ids,
            metadata: AuditMetadata::default(),
        })
    }
}
