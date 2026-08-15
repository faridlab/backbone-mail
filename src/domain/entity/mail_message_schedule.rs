use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use uuid::Uuid;
use super::AuditMetadata;

/// Strongly-typed ID for MailMessageSchedule
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct MailMessageScheduleId(pub Uuid);

impl MailMessageScheduleId {
    pub fn new(id: Uuid) -> Self { Self(id) }
    pub fn generate() -> Self { Self(Uuid::new_v4()) }
    pub fn into_inner(self) -> Uuid { self.0 }
}

impl std::fmt::Display for MailMessageScheduleId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::str::FromStr for MailMessageScheduleId {
    type Err = uuid::Error;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(Self(Uuid::parse_str(s)?))
    }
}

impl From<Uuid> for MailMessageScheduleId {
    fn from(id: Uuid) -> Self { Self(id) }
}

impl From<MailMessageScheduleId> for Uuid {
    fn from(id: MailMessageScheduleId) -> Self { id.0 }
}

impl AsRef<Uuid> for MailMessageScheduleId {
    fn as_ref(&self) -> &Uuid { &self.0 }
}

impl std::ops::Deref for MailMessageScheduleId {
    type Target = Uuid;
    fn deref(&self) -> &Self::Target { &self.0 }
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct MailMessageSchedule {
    pub id: Uuid,
    pub mail_message_id: Uuid,
    pub notification_parameters: Option<String>,
    pub scheduled_datetime: DateTime<Utc>,
    #[serde(default)]
    #[sqlx(json)]
    pub metadata: AuditMetadata,
}

impl MailMessageSchedule {
    /// Create a builder for MailMessageSchedule
    pub fn builder() -> MailMessageScheduleBuilder {
        <MailMessageScheduleBuilder as Default>::default()
    }

    /// Create a new MailMessageSchedule with required fields
    pub fn new(mail_message_id: Uuid, scheduled_datetime: DateTime<Utc>) -> Self {
        Self {
            id: Uuid::new_v4(),
            mail_message_id,
            notification_parameters: None,
            scheduled_datetime,
            metadata: AuditMetadata::default(),
        }
    }

    /// Get the entity's unique identifier
    pub fn id(&self) -> &Uuid {
        &self.id
    }

    /// Get a strongly-typed ID for this entity
    pub fn typed_id(&self) -> MailMessageScheduleId {
        MailMessageScheduleId(self.id)
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

    /// Set the notification_parameters field (chainable)
    pub fn with_notification_parameters(mut self, value: String) -> Self {
        self.notification_parameters = Some(value);
        self
    }

    // ==========================================================
    // Partial Update
    // ==========================================================

    /// Apply partial updates from a map of field name to JSON value
    pub fn apply_patch(&mut self, fields: std::collections::HashMap<String, serde_json::Value>) {
        for (key, value) in fields {
            match key.as_str() {
                "mail_message_id" => {
                    if let Ok(v) = serde_json::from_value(value) { self.mail_message_id = v; }
                }
                "notification_parameters" => {
                    if let Ok(v) = serde_json::from_value(value) { self.notification_parameters = v; }
                }
                "scheduled_datetime" => {
                    if let Ok(v) = serde_json::from_value(value) { self.scheduled_datetime = v; }
                }
                _ => {} // ignore unknown fields
            }
        }
    }

    // <<< CUSTOM METHODS START >>>
    // <<< CUSTOM METHODS END >>>
}

impl super::Entity for MailMessageSchedule {
    type Id = Uuid;

    fn entity_id(&self) -> &Self::Id {
        &self.id
    }

    fn entity_type() -> &'static str {
        "MailMessageSchedule"
    }
}

impl backbone_core::PersistentEntity for MailMessageSchedule {
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

impl backbone_orm::EntityRepoMeta for MailMessageSchedule {
    fn column_types() -> std::collections::HashMap<String, String> {
        let mut m = std::collections::HashMap::new();
        m.insert("id".to_string(), "uuid".to_string());
        m.insert("mail_message_id".to_string(), "uuid".to_string());
        m
    }
    fn search_fields() -> &'static [&'static str] {
        &[]
    }
}

/// Builder for MailMessageSchedule entity
///
/// Provides a fluent API for constructing MailMessageSchedule instances.
/// System fields (id, metadata, timestamps) are auto-initialized.
#[derive(Debug, Clone, Default)]
pub struct MailMessageScheduleBuilder {
    mail_message_id: Option<Uuid>,
    notification_parameters: Option<String>,
    scheduled_datetime: Option<DateTime<Utc>>,
}

impl MailMessageScheduleBuilder {
    /// Set the mail_message_id field (required)
    pub fn mail_message_id(mut self, value: Uuid) -> Self {
        self.mail_message_id = Some(value);
        self
    }

    /// Set the notification_parameters field (optional)
    pub fn notification_parameters(mut self, value: String) -> Self {
        self.notification_parameters = Some(value);
        self
    }

    /// Set the scheduled_datetime field (required)
    pub fn scheduled_datetime(mut self, value: DateTime<Utc>) -> Self {
        self.scheduled_datetime = Some(value);
        self
    }

    /// Build the MailMessageSchedule entity
    ///
    /// Returns Err if any required field without a default is missing.
    pub fn build(self) -> Result<MailMessageSchedule, String> {
        let mail_message_id = self.mail_message_id.ok_or_else(|| "mail_message_id is required".to_string())?;
        let scheduled_datetime = self.scheduled_datetime.ok_or_else(|| "scheduled_datetime is required".to_string())?;

        Ok(MailMessageSchedule {
            id: Uuid::new_v4(),
            mail_message_id,
            notification_parameters: self.notification_parameters,
            scheduled_datetime,
            metadata: AuditMetadata::default(),
        })
    }
}
