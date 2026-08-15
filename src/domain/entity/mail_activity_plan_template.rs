use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use uuid::Uuid;

use super::MailActivityDelayUnit;
use super::AuditMetadata;

/// Strongly-typed ID for MailActivityPlanTemplate
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct MailActivityPlanTemplateId(pub Uuid);

impl MailActivityPlanTemplateId {
    pub fn new(id: Uuid) -> Self { Self(id) }
    pub fn generate() -> Self { Self(Uuid::new_v4()) }
    pub fn into_inner(self) -> Uuid { self.0 }
}

impl std::fmt::Display for MailActivityPlanTemplateId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::str::FromStr for MailActivityPlanTemplateId {
    type Err = uuid::Error;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(Self(Uuid::parse_str(s)?))
    }
}

impl From<Uuid> for MailActivityPlanTemplateId {
    fn from(id: Uuid) -> Self { Self(id) }
}

impl From<MailActivityPlanTemplateId> for Uuid {
    fn from(id: MailActivityPlanTemplateId) -> Self { id.0 }
}

impl AsRef<Uuid> for MailActivityPlanTemplateId {
    fn as_ref(&self) -> &Uuid { &self.0 }
}

impl std::ops::Deref for MailActivityPlanTemplateId {
    type Target = Uuid;
    fn deref(&self) -> &Self::Target { &self.0 }
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct MailActivityPlanTemplate {
    pub id: Uuid,
    pub plan_id: Uuid,
    pub activity_type_id: Uuid,
    pub summary: Option<String>,
    pub note: Option<String>,
    pub delay_count: Option<i32>,
    pub delay_unit: Option<MailActivityDelayUnit>,
    pub user_id: Option<Uuid>,
    #[serde(default)]
    #[sqlx(json)]
    pub metadata: AuditMetadata,
}

impl MailActivityPlanTemplate {
    /// Create a builder for MailActivityPlanTemplate
    pub fn builder() -> MailActivityPlanTemplateBuilder {
        <MailActivityPlanTemplateBuilder as Default>::default()
    }

    /// Create a new MailActivityPlanTemplate with required fields
    pub fn new(plan_id: Uuid, activity_type_id: Uuid) -> Self {
        Self {
            id: Uuid::new_v4(),
            plan_id,
            activity_type_id,
            summary: None,
            note: None,
            delay_count: None,
            delay_unit: None,
            user_id: None,
            metadata: AuditMetadata::default(),
        }
    }

    /// Get the entity's unique identifier
    pub fn id(&self) -> &Uuid {
        &self.id
    }

    /// Get a strongly-typed ID for this entity
    pub fn typed_id(&self) -> MailActivityPlanTemplateId {
        MailActivityPlanTemplateId(self.id)
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

    /// Set the summary field (chainable)
    pub fn with_summary(mut self, value: String) -> Self {
        self.summary = Some(value);
        self
    }

    /// Set the note field (chainable)
    pub fn with_note(mut self, value: String) -> Self {
        self.note = Some(value);
        self
    }

    /// Set the delay_count field (chainable)
    pub fn with_delay_count(mut self, value: i32) -> Self {
        self.delay_count = Some(value);
        self
    }

    /// Set the delay_unit field (chainable)
    pub fn with_delay_unit(mut self, value: MailActivityDelayUnit) -> Self {
        self.delay_unit = Some(value);
        self
    }

    /// Set the user_id field (chainable)
    pub fn with_user_id(mut self, value: Uuid) -> Self {
        self.user_id = Some(value);
        self
    }

    // ==========================================================
    // Partial Update
    // ==========================================================

    /// Apply partial updates from a map of field name to JSON value
    pub fn apply_patch(&mut self, fields: std::collections::HashMap<String, serde_json::Value>) {
        for (key, value) in fields {
            match key.as_str() {
                "plan_id" => {
                    if let Ok(v) = serde_json::from_value(value) { self.plan_id = v; }
                }
                "activity_type_id" => {
                    if let Ok(v) = serde_json::from_value(value) { self.activity_type_id = v; }
                }
                "summary" => {
                    if let Ok(v) = serde_json::from_value(value) { self.summary = v; }
                }
                "note" => {
                    if let Ok(v) = serde_json::from_value(value) { self.note = v; }
                }
                "delay_count" => {
                    if let Ok(v) = serde_json::from_value(value) { self.delay_count = v; }
                }
                "delay_unit" => {
                    if let Ok(v) = serde_json::from_value(value) { self.delay_unit = v; }
                }
                "user_id" => {
                    if let Ok(v) = serde_json::from_value(value) { self.user_id = v; }
                }
                _ => {} // ignore unknown fields
            }
        }
    }

    // <<< CUSTOM METHODS START >>>
    // <<< CUSTOM METHODS END >>>
}

impl super::Entity for MailActivityPlanTemplate {
    type Id = Uuid;

    fn entity_id(&self) -> &Self::Id {
        &self.id
    }

    fn entity_type() -> &'static str {
        "MailActivityPlanTemplate"
    }
}

impl backbone_core::PersistentEntity for MailActivityPlanTemplate {
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

impl backbone_orm::EntityRepoMeta for MailActivityPlanTemplate {
    fn column_types() -> std::collections::HashMap<String, String> {
        let mut m = std::collections::HashMap::new();
        m.insert("id".to_string(), "uuid".to_string());
        m.insert("plan_id".to_string(), "uuid".to_string());
        m.insert("activity_type_id".to_string(), "uuid".to_string());
        m.insert("user_id".to_string(), "uuid".to_string());
        m.insert("delay_unit".to_string(), "mail_activity_delay_unit".to_string());
        m
    }
    fn search_fields() -> &'static [&'static str] {
        &[]
    }
}

/// Builder for MailActivityPlanTemplate entity
///
/// Provides a fluent API for constructing MailActivityPlanTemplate instances.
/// System fields (id, metadata, timestamps) are auto-initialized.
#[derive(Debug, Clone, Default)]
pub struct MailActivityPlanTemplateBuilder {
    plan_id: Option<Uuid>,
    activity_type_id: Option<Uuid>,
    summary: Option<String>,
    note: Option<String>,
    delay_count: Option<i32>,
    delay_unit: Option<MailActivityDelayUnit>,
    user_id: Option<Uuid>,
}

impl MailActivityPlanTemplateBuilder {
    /// Set the plan_id field (required)
    pub fn plan_id(mut self, value: Uuid) -> Self {
        self.plan_id = Some(value);
        self
    }

    /// Set the activity_type_id field (required)
    pub fn activity_type_id(mut self, value: Uuid) -> Self {
        self.activity_type_id = Some(value);
        self
    }

    /// Set the summary field (optional)
    pub fn summary(mut self, value: String) -> Self {
        self.summary = Some(value);
        self
    }

    /// Set the note field (optional)
    pub fn note(mut self, value: String) -> Self {
        self.note = Some(value);
        self
    }

    /// Set the delay_count field (optional)
    pub fn delay_count(mut self, value: i32) -> Self {
        self.delay_count = Some(value);
        self
    }

    /// Set the delay_unit field (optional)
    pub fn delay_unit(mut self, value: MailActivityDelayUnit) -> Self {
        self.delay_unit = Some(value);
        self
    }

    /// Set the user_id field (optional)
    pub fn user_id(mut self, value: Uuid) -> Self {
        self.user_id = Some(value);
        self
    }

    /// Build the MailActivityPlanTemplate entity
    ///
    /// Returns Err if any required field without a default is missing.
    pub fn build(self) -> Result<MailActivityPlanTemplate, String> {
        let plan_id = self.plan_id.ok_or_else(|| "plan_id is required".to_string())?;
        let activity_type_id = self.activity_type_id.ok_or_else(|| "activity_type_id is required".to_string())?;

        Ok(MailActivityPlanTemplate {
            id: Uuid::new_v4(),
            plan_id,
            activity_type_id,
            summary: self.summary,
            note: self.note,
            delay_count: self.delay_count,
            delay_unit: self.delay_unit,
            user_id: self.user_id,
            metadata: AuditMetadata::default(),
        })
    }
}
