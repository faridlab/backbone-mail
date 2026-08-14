use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use uuid::Uuid;
use super::AuditMetadata;

/// Strongly-typed ID for MailMessageReaction
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct MailMessageReactionId(pub Uuid);

impl MailMessageReactionId {
    pub fn new(id: Uuid) -> Self { Self(id) }
    pub fn generate() -> Self { Self(Uuid::new_v4()) }
    pub fn into_inner(self) -> Uuid { self.0 }
}

impl std::fmt::Display for MailMessageReactionId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::str::FromStr for MailMessageReactionId {
    type Err = uuid::Error;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(Self(Uuid::parse_str(s)?))
    }
}

impl From<Uuid> for MailMessageReactionId {
    fn from(id: Uuid) -> Self { Self(id) }
}

impl From<MailMessageReactionId> for Uuid {
    fn from(id: MailMessageReactionId) -> Self { id.0 }
}

impl AsRef<Uuid> for MailMessageReactionId {
    fn as_ref(&self) -> &Uuid { &self.0 }
}

impl std::ops::Deref for MailMessageReactionId {
    type Target = Uuid;
    fn deref(&self) -> &Self::Target { &self.0 }
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct MailMessageReaction {
    pub id: Uuid,
    pub message_id: Uuid,
    pub content: String,
    pub partner_id: Option<Uuid>,
    pub guest_id: Option<Uuid>,
    #[serde(default)]
    #[sqlx(json)]
    pub metadata: AuditMetadata,
}

impl MailMessageReaction {
    /// Create a builder for MailMessageReaction
    pub fn builder() -> MailMessageReactionBuilder {
        MailMessageReactionBuilder::default()
    }

    /// Create a new MailMessageReaction with required fields
    pub fn new(message_id: Uuid, content: String) -> Self {
        Self {
            id: Uuid::new_v4(),
            message_id,
            content,
            partner_id: None,
            guest_id: None,
            metadata: AuditMetadata::default(),
        }
    }

    /// Get the entity's unique identifier
    pub fn id(&self) -> &Uuid {
        &self.id
    }

    /// Get a strongly-typed ID for this entity
    pub fn typed_id(&self) -> MailMessageReactionId {
        MailMessageReactionId(self.id)
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

    /// Set the guest_id field (chainable)
    pub fn with_guest_id(mut self, value: Uuid) -> Self {
        self.guest_id = Some(value);
        self
    }

    // ==========================================================
    // Partial Update
    // ==========================================================

    /// Apply partial updates from a map of field name to JSON value
    pub fn apply_patch(&mut self, fields: std::collections::HashMap<String, serde_json::Value>) {
        for (key, value) in fields {
            match key.as_str() {
                "message_id" => {
                    if let Ok(v) = serde_json::from_value(value) { self.message_id = v; }
                }
                "content" => {
                    if let Ok(v) = serde_json::from_value(value) { self.content = v; }
                }
                "partner_id" => {
                    if let Ok(v) = serde_json::from_value(value) { self.partner_id = v; }
                }
                "guest_id" => {
                    if let Ok(v) = serde_json::from_value(value) { self.guest_id = v; }
                }
                _ => {} // ignore unknown fields
            }
        }
    }

    // <<< CUSTOM METHODS START >>>
    // <<< CUSTOM METHODS END >>>
}

impl super::Entity for MailMessageReaction {
    type Id = Uuid;

    fn entity_id(&self) -> &Self::Id {
        &self.id
    }

    fn entity_type() -> &'static str {
        "MailMessageReaction"
    }
}

impl backbone_core::PersistentEntity for MailMessageReaction {
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

impl backbone_orm::EntityRepoMeta for MailMessageReaction {
    fn column_types() -> std::collections::HashMap<String, String> {
        let mut m = std::collections::HashMap::new();
        m.insert("id".to_string(), "uuid".to_string());
        m.insert("message_id".to_string(), "uuid".to_string());
        m.insert("partner_id".to_string(), "uuid".to_string());
        m.insert("guest_id".to_string(), "uuid".to_string());
        m
    }
    fn search_fields() -> &'static [&'static str] {
        &["content"]
    }
}

/// Builder for MailMessageReaction entity
///
/// Provides a fluent API for constructing MailMessageReaction instances.
/// System fields (id, metadata, timestamps) are auto-initialized.
#[derive(Debug, Clone, Default)]
pub struct MailMessageReactionBuilder {
    message_id: Option<Uuid>,
    content: Option<String>,
    partner_id: Option<Uuid>,
    guest_id: Option<Uuid>,
}

impl MailMessageReactionBuilder {
    /// Set the message_id field (required)
    pub fn message_id(mut self, value: Uuid) -> Self {
        self.message_id = Some(value);
        self
    }

    /// Set the content field (required)
    pub fn content(mut self, value: String) -> Self {
        self.content = Some(value);
        self
    }

    /// Set the partner_id field (optional)
    pub fn partner_id(mut self, value: Uuid) -> Self {
        self.partner_id = Some(value);
        self
    }

    /// Set the guest_id field (optional)
    pub fn guest_id(mut self, value: Uuid) -> Self {
        self.guest_id = Some(value);
        self
    }

    /// Build the MailMessageReaction entity
    ///
    /// Returns Err if any required field without a default is missing.
    pub fn build(self) -> Result<MailMessageReaction, String> {
        let message_id = self.message_id.ok_or_else(|| "message_id is required".to_string())?;
        let content = self.content.ok_or_else(|| "content is required".to_string())?;

        Ok(MailMessageReaction {
            id: Uuid::new_v4(),
            message_id,
            content,
            partner_id: self.partner_id,
            guest_id: self.guest_id,
            metadata: AuditMetadata::default(),
        })
    }
}
