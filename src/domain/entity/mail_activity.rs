use chrono::{DateTime, Utc, NaiveDate};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use uuid::Uuid;

use super::MailActivityState;
use super::AuditMetadata;

/// Strongly-typed ID for MailActivity
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct MailActivityId(pub Uuid);

impl MailActivityId {
    pub fn new(id: Uuid) -> Self { Self(id) }
    pub fn generate() -> Self { Self(Uuid::new_v4()) }
    pub fn into_inner(self) -> Uuid { self.0 }
}

impl std::fmt::Display for MailActivityId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::str::FromStr for MailActivityId {
    type Err = uuid::Error;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(Self(Uuid::parse_str(s)?))
    }
}

impl From<Uuid> for MailActivityId {
    fn from(id: Uuid) -> Self { Self(id) }
}

impl From<MailActivityId> for Uuid {
    fn from(id: MailActivityId) -> Self { id.0 }
}

impl AsRef<Uuid> for MailActivityId {
    fn as_ref(&self) -> &Uuid { &self.0 }
}

impl std::ops::Deref for MailActivityId {
    type Target = Uuid;
    fn deref(&self) -> &Self::Target { &self.0 }
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct MailActivity {
    pub id: Uuid,
    pub res_model: String,
    pub res_id: Uuid,
    pub res_model_id: Option<Uuid>,
    pub activity_type_id: Option<Uuid>,
    pub summary: Option<String>,
    pub note: Option<String>,
    pub date_deadline: NaiveDate,
    pub user_id: Uuid,
    pub requested_user_id: Option<Uuid>,
    pub state: MailActivityState,
    pub active: bool,
    pub has_recommended_activities: bool,
    pub chained_next_activity: Option<serde_json::Value>,
    pub calendar_event_id: Option<Uuid>,
    #[serde(default)]
    #[sqlx(json)]
    pub metadata: AuditMetadata,
}

impl MailActivity {
    /// Create a builder for MailActivity
    pub fn builder() -> MailActivityBuilder {
        MailActivityBuilder::default()
    }

    /// Create a new MailActivity with required fields
    pub fn new(res_model: String, res_id: Uuid, date_deadline: NaiveDate, user_id: Uuid, state: MailActivityState, active: bool, has_recommended_activities: bool) -> Self {
        Self {
            id: Uuid::new_v4(),
            res_model,
            res_id,
            res_model_id: None,
            activity_type_id: None,
            summary: None,
            note: None,
            date_deadline,
            user_id,
            requested_user_id: None,
            state,
            active,
            has_recommended_activities,
            chained_next_activity: None,
            calendar_event_id: None,
            metadata: AuditMetadata::default(),
        }
    }

    /// Get the entity's unique identifier
    pub fn id(&self) -> &Uuid {
        &self.id
    }

    /// Get a strongly-typed ID for this entity
    pub fn typed_id(&self) -> MailActivityId {
        MailActivityId(self.id)
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

    /// Set the res_model_id field (chainable)
    pub fn with_res_model_id(mut self, value: Uuid) -> Self {
        self.res_model_id = Some(value);
        self
    }

    /// Set the activity_type_id field (chainable)
    pub fn with_activity_type_id(mut self, value: Uuid) -> Self {
        self.activity_type_id = Some(value);
        self
    }

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

    /// Set the requested_user_id field (chainable)
    pub fn with_requested_user_id(mut self, value: Uuid) -> Self {
        self.requested_user_id = Some(value);
        self
    }

    /// Set the chained_next_activity field (chainable)
    pub fn with_chained_next_activity(mut self, value: serde_json::Value) -> Self {
        self.chained_next_activity = Some(value);
        self
    }

    /// Set the calendar_event_id field (chainable)
    pub fn with_calendar_event_id(mut self, value: Uuid) -> Self {
        self.calendar_event_id = Some(value);
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
                "res_model_id" => {
                    if let Ok(v) = serde_json::from_value(value) { self.res_model_id = v; }
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
                "date_deadline" => {
                    if let Ok(v) = serde_json::from_value(value) { self.date_deadline = v; }
                }
                "user_id" => {
                    if let Ok(v) = serde_json::from_value(value) { self.user_id = v; }
                }
                "requested_user_id" => {
                    if let Ok(v) = serde_json::from_value(value) { self.requested_user_id = v; }
                }
                "state" => {
                    if let Ok(v) = serde_json::from_value(value) { self.state = v; }
                }
                "active" => {
                    if let Ok(v) = serde_json::from_value(value) { self.active = v; }
                }
                "has_recommended_activities" => {
                    if let Ok(v) = serde_json::from_value(value) { self.has_recommended_activities = v; }
                }
                "chained_next_activity" => {
                    if let Ok(v) = serde_json::from_value(value) { self.chained_next_activity = v; }
                }
                "calendar_event_id" => {
                    if let Ok(v) = serde_json::from_value(value) { self.calendar_event_id = v; }
                }
                _ => {} // ignore unknown fields
            }
        }
    }

    // <<< CUSTOM METHODS START >>>
    // <<< CUSTOM METHODS END >>>
}

impl super::Entity for MailActivity {
    type Id = Uuid;

    fn entity_id(&self) -> &Self::Id {
        &self.id
    }

    fn entity_type() -> &'static str {
        "MailActivity"
    }
}

impl backbone_core::PersistentEntity for MailActivity {
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

impl backbone_orm::EntityRepoMeta for MailActivity {
    fn column_types() -> std::collections::HashMap<String, String> {
        let mut m = std::collections::HashMap::new();
        m.insert("id".to_string(), "uuid".to_string());
        m.insert("res_id".to_string(), "uuid".to_string());
        m.insert("res_model_id".to_string(), "uuid".to_string());
        m.insert("activity_type_id".to_string(), "uuid".to_string());
        m.insert("user_id".to_string(), "uuid".to_string());
        m.insert("requested_user_id".to_string(), "uuid".to_string());
        m.insert("calendar_event_id".to_string(), "uuid".to_string());
        m.insert("state".to_string(), "mail_activity_state".to_string());
        m
    }
    fn search_fields() -> &'static [&'static str] {
        &["res_model"]
    }
}

/// Builder for MailActivity entity
///
/// Provides a fluent API for constructing MailActivity instances.
/// System fields (id, metadata, timestamps) are auto-initialized.
#[derive(Debug, Clone, Default)]
pub struct MailActivityBuilder {
    res_model: Option<String>,
    res_id: Option<Uuid>,
    res_model_id: Option<Uuid>,
    activity_type_id: Option<Uuid>,
    summary: Option<String>,
    note: Option<String>,
    date_deadline: Option<NaiveDate>,
    user_id: Option<Uuid>,
    requested_user_id: Option<Uuid>,
    state: Option<MailActivityState>,
    active: Option<bool>,
    has_recommended_activities: Option<bool>,
    chained_next_activity: Option<serde_json::Value>,
    calendar_event_id: Option<Uuid>,
}

impl MailActivityBuilder {
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

    /// Set the res_model_id field (optional)
    pub fn res_model_id(mut self, value: Uuid) -> Self {
        self.res_model_id = Some(value);
        self
    }

    /// Set the activity_type_id field (optional)
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

    /// Set the date_deadline field (required)
    pub fn date_deadline(mut self, value: NaiveDate) -> Self {
        self.date_deadline = Some(value);
        self
    }

    /// Set the user_id field (required)
    pub fn user_id(mut self, value: Uuid) -> Self {
        self.user_id = Some(value);
        self
    }

    /// Set the requested_user_id field (optional)
    pub fn requested_user_id(mut self, value: Uuid) -> Self {
        self.requested_user_id = Some(value);
        self
    }

    /// Set the state field (default: `MailActivityState::default()`)
    pub fn state(mut self, value: MailActivityState) -> Self {
        self.state = Some(value);
        self
    }

    /// Set the active field (default: `true`)
    pub fn active(mut self, value: bool) -> Self {
        self.active = Some(value);
        self
    }

    /// Set the has_recommended_activities field (default: `false`)
    pub fn has_recommended_activities(mut self, value: bool) -> Self {
        self.has_recommended_activities = Some(value);
        self
    }

    /// Set the chained_next_activity field (optional)
    pub fn chained_next_activity(mut self, value: serde_json::Value) -> Self {
        self.chained_next_activity = Some(value);
        self
    }

    /// Set the calendar_event_id field (optional)
    pub fn calendar_event_id(mut self, value: Uuid) -> Self {
        self.calendar_event_id = Some(value);
        self
    }

    /// Build the MailActivity entity
    ///
    /// Returns Err if any required field without a default is missing.
    pub fn build(self) -> Result<MailActivity, String> {
        let res_model = self.res_model.ok_or_else(|| "res_model is required".to_string())?;
        let res_id = self.res_id.ok_or_else(|| "res_id is required".to_string())?;
        let date_deadline = self.date_deadline.ok_or_else(|| "date_deadline is required".to_string())?;
        let user_id = self.user_id.ok_or_else(|| "user_id is required".to_string())?;

        Ok(MailActivity {
            id: Uuid::new_v4(),
            res_model,
            res_id,
            res_model_id: self.res_model_id,
            activity_type_id: self.activity_type_id,
            summary: self.summary,
            note: self.note,
            date_deadline,
            user_id,
            requested_user_id: self.requested_user_id,
            state: self.state.unwrap_or(MailActivityState::default()),
            active: self.active.unwrap_or(true),
            has_recommended_activities: self.has_recommended_activities.unwrap_or(false),
            chained_next_activity: self.chained_next_activity,
            calendar_event_id: self.calendar_event_id,
            metadata: AuditMetadata::default(),
        })
    }
}
