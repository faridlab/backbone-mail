use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use uuid::Uuid;
use super::AuditMetadata;

/// Strongly-typed ID for SmsTemplate
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct SmsTemplateId(pub Uuid);

impl SmsTemplateId {
    pub fn new(id: Uuid) -> Self { Self(id) }
    pub fn generate() -> Self { Self(Uuid::new_v4()) }
    pub fn into_inner(self) -> Uuid { self.0 }
}

impl std::fmt::Display for SmsTemplateId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::str::FromStr for SmsTemplateId {
    type Err = uuid::Error;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(Self(Uuid::parse_str(s)?))
    }
}

impl From<Uuid> for SmsTemplateId {
    fn from(id: Uuid) -> Self { Self(id) }
}

impl From<SmsTemplateId> for Uuid {
    fn from(id: SmsTemplateId) -> Self { id.0 }
}

impl AsRef<Uuid> for SmsTemplateId {
    fn as_ref(&self) -> &Uuid { &self.0 }
}

impl std::ops::Deref for SmsTemplateId {
    type Target = Uuid;
    fn deref(&self) -> &Self::Target { &self.0 }
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct SmsTemplate {
    pub id: Uuid,
    pub name: String,
    pub body: String,
    pub model_id: Option<Uuid>,
    pub lang: Option<String>,
    pub condition: Option<String>,
    pub active: bool,
    #[serde(default)]
    #[sqlx(json)]
    pub metadata: AuditMetadata,
}

impl SmsTemplate {
    /// Create a builder for SmsTemplate
    pub fn builder() -> SmsTemplateBuilder {
        SmsTemplateBuilder::default()
    }

    /// Create a new SmsTemplate with required fields
    pub fn new(name: String, body: String, active: bool) -> Self {
        Self {
            id: Uuid::new_v4(),
            name,
            body,
            model_id: None,
            lang: None,
            condition: None,
            active,
            metadata: AuditMetadata::default(),
        }
    }

    /// Get the entity's unique identifier
    pub fn id(&self) -> &Uuid {
        &self.id
    }

    /// Get a strongly-typed ID for this entity
    pub fn typed_id(&self) -> SmsTemplateId {
        SmsTemplateId(self.id)
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

    /// Set the model_id field (chainable)
    pub fn with_model_id(mut self, value: Uuid) -> Self {
        self.model_id = Some(value);
        self
    }

    /// Set the lang field (chainable)
    pub fn with_lang(mut self, value: String) -> Self {
        self.lang = Some(value);
        self
    }

    /// Set the condition field (chainable)
    pub fn with_condition(mut self, value: String) -> Self {
        self.condition = Some(value);
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
                "body" => {
                    if let Ok(v) = serde_json::from_value(value) { self.body = v; }
                }
                "model_id" => {
                    if let Ok(v) = serde_json::from_value(value) { self.model_id = v; }
                }
                "lang" => {
                    if let Ok(v) = serde_json::from_value(value) { self.lang = v; }
                }
                "condition" => {
                    if let Ok(v) = serde_json::from_value(value) { self.condition = v; }
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

impl super::Entity for SmsTemplate {
    type Id = Uuid;

    fn entity_id(&self) -> &Self::Id {
        &self.id
    }

    fn entity_type() -> &'static str {
        "SmsTemplate"
    }
}

impl backbone_core::PersistentEntity for SmsTemplate {
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

impl backbone_orm::EntityRepoMeta for SmsTemplate {
    fn column_types() -> std::collections::HashMap<String, String> {
        let mut m = std::collections::HashMap::new();
        m.insert("id".to_string(), "uuid".to_string());
        m.insert("model_id".to_string(), "uuid".to_string());
        m
    }
    fn search_fields() -> &'static [&'static str] {
        &["name", "body"]
    }
}

/// Builder for SmsTemplate entity
///
/// Provides a fluent API for constructing SmsTemplate instances.
/// System fields (id, metadata, timestamps) are auto-initialized.
#[derive(Debug, Clone, Default)]
pub struct SmsTemplateBuilder {
    name: Option<String>,
    body: Option<String>,
    model_id: Option<Uuid>,
    lang: Option<String>,
    condition: Option<String>,
    active: Option<bool>,
}

impl SmsTemplateBuilder {
    /// Set the name field (required)
    pub fn name(mut self, value: String) -> Self {
        self.name = Some(value);
        self
    }

    /// Set the body field (required)
    pub fn body(mut self, value: String) -> Self {
        self.body = Some(value);
        self
    }

    /// Set the model_id field (optional)
    pub fn model_id(mut self, value: Uuid) -> Self {
        self.model_id = Some(value);
        self
    }

    /// Set the lang field (optional)
    pub fn lang(mut self, value: String) -> Self {
        self.lang = Some(value);
        self
    }

    /// Set the condition field (optional)
    pub fn condition(mut self, value: String) -> Self {
        self.condition = Some(value);
        self
    }

    /// Set the active field (default: `true`)
    pub fn active(mut self, value: bool) -> Self {
        self.active = Some(value);
        self
    }

    /// Build the SmsTemplate entity
    ///
    /// Returns Err if any required field without a default is missing.
    pub fn build(self) -> Result<SmsTemplate, String> {
        let name = self.name.ok_or_else(|| "name is required".to_string())?;
        let body = self.body.ok_or_else(|| "body is required".to_string())?;

        Ok(SmsTemplate {
            id: Uuid::new_v4(),
            name,
            body,
            model_id: self.model_id,
            lang: self.lang,
            condition: self.condition,
            active: self.active.unwrap_or(true),
            metadata: AuditMetadata::default(),
        })
    }
}
