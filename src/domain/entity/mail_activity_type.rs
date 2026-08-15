use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use uuid::Uuid;

use super::MailActivityCategory;
use super::MailActivityChainingType;
use super::MailActivityDelayUnit;
use super::MailActivityDelayFrom;
use super::AuditMetadata;

use crate::domain::state_machine::{MailActivityTypeHooksStateMachine, MailActivityTypeHooksState, StateMachineError};

/// Strongly-typed ID for MailActivityType
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct MailActivityTypeId(pub Uuid);

impl MailActivityTypeId {
    pub fn new(id: Uuid) -> Self { Self(id) }
    pub fn generate() -> Self { Self(Uuid::new_v4()) }
    pub fn into_inner(self) -> Uuid { self.0 }
}

impl std::fmt::Display for MailActivityTypeId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::str::FromStr for MailActivityTypeId {
    type Err = uuid::Error;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(Self(Uuid::parse_str(s)?))
    }
}

impl From<Uuid> for MailActivityTypeId {
    fn from(id: Uuid) -> Self { Self(id) }
}

impl From<MailActivityTypeId> for Uuid {
    fn from(id: MailActivityTypeId) -> Self { id.0 }
}

impl AsRef<Uuid> for MailActivityTypeId {
    fn as_ref(&self) -> &Uuid { &self.0 }
}

impl std::ops::Deref for MailActivityTypeId {
    type Target = Uuid;
    fn deref(&self) -> &Self::Target { &self.0 }
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct MailActivityType {
    pub id: Uuid,
    pub name: String,
    pub summary: Option<String>,
    pub note: Option<String>,
    pub category: Option<MailActivityCategory>,
    pub(crate) chaining_type: MailActivityChainingType,
    pub delay_count: i32,
    pub delay_unit: MailActivityDelayUnit,
    pub delay_from: MailActivityDelayFrom,
    pub default_user_id: Option<Uuid>,
    pub default_note: Option<String>,
    pub icon: Option<String>,
    pub res_model: Option<String>,
    pub res_model_id: Option<Uuid>,
    pub active: bool,
    #[serde(default)]
    #[sqlx(json)]
    pub metadata: AuditMetadata,
}

impl MailActivityType {
    /// Create a builder for MailActivityType
    pub fn builder() -> MailActivityTypeBuilder {
        <MailActivityTypeBuilder as Default>::default()
    }

    /// Create a new MailActivityType with required fields
    pub fn new(name: String, chaining_type: MailActivityChainingType, delay_count: i32, delay_unit: MailActivityDelayUnit, delay_from: MailActivityDelayFrom, active: bool) -> Self {
        Self {
            id: Uuid::new_v4(),
            name,
            summary: None,
            note: None,
            category: None,
            chaining_type,
            delay_count,
            delay_unit,
            delay_from,
            default_user_id: None,
            default_note: None,
            icon: None,
            res_model: None,
            res_model_id: None,
            active,
            metadata: AuditMetadata::default(),
        }
    }

    /// Get the entity's unique identifier
    pub fn id(&self) -> &Uuid {
        &self.id
    }

    /// Get a strongly-typed ID for this entity
    pub fn typed_id(&self) -> MailActivityTypeId {
        MailActivityTypeId(self.id)
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

    /// Set the category field (chainable)
    pub fn with_category(mut self, value: MailActivityCategory) -> Self {
        self.category = Some(value);
        self
    }

    /// Set the default_user_id field (chainable)
    pub fn with_default_user_id(mut self, value: Uuid) -> Self {
        self.default_user_id = Some(value);
        self
    }

    /// Set the default_note field (chainable)
    pub fn with_default_note(mut self, value: String) -> Self {
        self.default_note = Some(value);
        self
    }

    /// Set the icon field (chainable)
    pub fn with_icon(mut self, value: String) -> Self {
        self.icon = Some(value);
        self
    }

    /// Set the res_model field (chainable)
    pub fn with_res_model(mut self, value: String) -> Self {
        self.res_model = Some(value);
        self
    }

    /// Set the res_model_id field (chainable)
    pub fn with_res_model_id(mut self, value: Uuid) -> Self {
        self.res_model_id = Some(value);
        self
    }

    // ==========================================================
    // State Machine
    // ==========================================================

    /// Transition to a new state via the chaining_type state machine.
    ///
    /// Returns `Err` if the transition is not permitted from the current state.
    /// Use this method instead of assigning `self.chaining_type` directly.
    pub fn transition_to(&mut self, new_state: MailActivityTypeHooksState) -> Result<(), StateMachineError> {
        let current = self.chaining_type.to_string().parse::<MailActivityTypeHooksState>()?;
        let mut sm = MailActivityTypeHooksStateMachine::from_state(current);
        sm.transition_to_state(new_state)?;
        self.chaining_type = new_state.to_string().parse::<MailActivityChainingType>()
            .map_err(|e| StateMachineError::InvalidState(e.to_string()))?;
        Ok(())
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
                "summary" => {
                    if let Ok(v) = serde_json::from_value(value) { self.summary = v; }
                }
                "note" => {
                    if let Ok(v) = serde_json::from_value(value) { self.note = v; }
                }
                "category" => {
                    if let Ok(v) = serde_json::from_value(value) { self.category = v; }
                }
                "delay_count" => {
                    if let Ok(v) = serde_json::from_value(value) { self.delay_count = v; }
                }
                "delay_unit" => {
                    if let Ok(v) = serde_json::from_value(value) { self.delay_unit = v; }
                }
                "delay_from" => {
                    if let Ok(v) = serde_json::from_value(value) { self.delay_from = v; }
                }
                "default_user_id" => {
                    if let Ok(v) = serde_json::from_value(value) { self.default_user_id = v; }
                }
                "default_note" => {
                    if let Ok(v) = serde_json::from_value(value) { self.default_note = v; }
                }
                "icon" => {
                    if let Ok(v) = serde_json::from_value(value) { self.icon = v; }
                }
                "res_model" => {
                    if let Ok(v) = serde_json::from_value(value) { self.res_model = v; }
                }
                "res_model_id" => {
                    if let Ok(v) = serde_json::from_value(value) { self.res_model_id = v; }
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

impl super::Entity for MailActivityType {
    type Id = Uuid;

    fn entity_id(&self) -> &Self::Id {
        &self.id
    }

    fn entity_type() -> &'static str {
        "MailActivityType"
    }
}

impl backbone_core::PersistentEntity for MailActivityType {
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

impl backbone_orm::EntityRepoMeta for MailActivityType {
    fn column_types() -> std::collections::HashMap<String, String> {
        let mut m = std::collections::HashMap::new();
        m.insert("id".to_string(), "uuid".to_string());
        m.insert("default_user_id".to_string(), "uuid".to_string());
        m.insert("res_model_id".to_string(), "uuid".to_string());
        m.insert("category".to_string(), "mail_activity_category".to_string());
        m.insert("chaining_type".to_string(), "mail_activity_chaining_type".to_string());
        m.insert("delay_unit".to_string(), "mail_activity_delay_unit".to_string());
        m.insert("delay_from".to_string(), "mail_activity_delay_from".to_string());
        m
    }
    fn search_fields() -> &'static [&'static str] {
        &["name"]
    }
}

/// Builder for MailActivityType entity
///
/// Provides a fluent API for constructing MailActivityType instances.
/// System fields (id, metadata, timestamps) are auto-initialized.
#[derive(Debug, Clone, Default)]
pub struct MailActivityTypeBuilder {
    name: Option<String>,
    summary: Option<String>,
    note: Option<String>,
    category: Option<MailActivityCategory>,
    chaining_type: Option<MailActivityChainingType>,
    delay_count: Option<i32>,
    delay_unit: Option<MailActivityDelayUnit>,
    delay_from: Option<MailActivityDelayFrom>,
    default_user_id: Option<Uuid>,
    default_note: Option<String>,
    icon: Option<String>,
    res_model: Option<String>,
    res_model_id: Option<Uuid>,
    active: Option<bool>,
}

impl MailActivityTypeBuilder {
    /// Set the name field (required)
    pub fn name(mut self, value: String) -> Self {
        self.name = Some(value);
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

    /// Set the category field (optional)
    pub fn category(mut self, value: MailActivityCategory) -> Self {
        self.category = Some(value);
        self
    }

    /// Set the chaining_type field (default: `MailActivityChainingType::default()`)
    pub fn chaining_type(mut self, value: MailActivityChainingType) -> Self {
        self.chaining_type = Some(value);
        self
    }

    /// Set the delay_count field (default: `0`)
    pub fn delay_count(mut self, value: i32) -> Self {
        self.delay_count = Some(value);
        self
    }

    /// Set the delay_unit field (default: `MailActivityDelayUnit::default()`)
    pub fn delay_unit(mut self, value: MailActivityDelayUnit) -> Self {
        self.delay_unit = Some(value);
        self
    }

    /// Set the delay_from field (default: `MailActivityDelayFrom::default()`)
    pub fn delay_from(mut self, value: MailActivityDelayFrom) -> Self {
        self.delay_from = Some(value);
        self
    }

    /// Set the default_user_id field (optional)
    pub fn default_user_id(mut self, value: Uuid) -> Self {
        self.default_user_id = Some(value);
        self
    }

    /// Set the default_note field (optional)
    pub fn default_note(mut self, value: String) -> Self {
        self.default_note = Some(value);
        self
    }

    /// Set the icon field (optional)
    pub fn icon(mut self, value: String) -> Self {
        self.icon = Some(value);
        self
    }

    /// Set the res_model field (optional)
    pub fn res_model(mut self, value: String) -> Self {
        self.res_model = Some(value);
        self
    }

    /// Set the res_model_id field (optional)
    pub fn res_model_id(mut self, value: Uuid) -> Self {
        self.res_model_id = Some(value);
        self
    }

    /// Set the active field (default: `true`)
    pub fn active(mut self, value: bool) -> Self {
        self.active = Some(value);
        self
    }

    /// Build the MailActivityType entity
    ///
    /// Returns Err if any required field without a default is missing.
    pub fn build(self) -> Result<MailActivityType, String> {
        let name = self.name.ok_or_else(|| "name is required".to_string())?;

        Ok(MailActivityType {
            id: Uuid::new_v4(),
            name,
            summary: self.summary,
            note: self.note,
            category: self.category,
            chaining_type: self.chaining_type.unwrap_or_default(),
            delay_count: self.delay_count.unwrap_or(0),
            delay_unit: self.delay_unit.unwrap_or_default(),
            delay_from: self.delay_from.unwrap_or_default(),
            default_user_id: self.default_user_id,
            default_note: self.default_note,
            icon: self.icon,
            res_model: self.res_model,
            res_model_id: self.res_model_id,
            active: self.active.unwrap_or(true),
            metadata: AuditMetadata::default(),
        })
    }
}
