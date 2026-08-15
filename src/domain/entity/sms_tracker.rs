use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use uuid::Uuid;

use super::MailNotificationStatus;
use super::NotificationFailureType;
use super::AuditMetadata;

/// Strongly-typed ID for SmsTracker
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct SmsTrackerId(pub Uuid);

impl SmsTrackerId {
    pub fn new(id: Uuid) -> Self { Self(id) }
    pub fn generate() -> Self { Self(Uuid::new_v4()) }
    pub fn into_inner(self) -> Uuid { self.0 }
}

impl std::fmt::Display for SmsTrackerId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::str::FromStr for SmsTrackerId {
    type Err = uuid::Error;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(Self(Uuid::parse_str(s)?))
    }
}

impl From<Uuid> for SmsTrackerId {
    fn from(id: Uuid) -> Self { Self(id) }
}

impl From<SmsTrackerId> for Uuid {
    fn from(id: SmsTrackerId) -> Self { id.0 }
}

impl AsRef<Uuid> for SmsTrackerId {
    fn as_ref(&self) -> &Uuid { &self.0 }
}

impl std::ops::Deref for SmsTrackerId {
    type Target = Uuid;
    fn deref(&self) -> &Self::Target { &self.0 }
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct SmsTracker {
    pub id: Uuid,
    pub sms_uuid: String,
    pub message_id: Option<Uuid>,
    pub notification_id: Option<Uuid>,
    pub state: MailNotificationStatus,
    pub failure_type: Option<NotificationFailureType>,
    pub failure_reason: Option<String>,
    pub recipient: Option<String>,
    #[serde(default)]
    #[sqlx(json)]
    pub metadata: AuditMetadata,
}

impl SmsTracker {
    /// Create a builder for SmsTracker
    pub fn builder() -> SmsTrackerBuilder {
        <SmsTrackerBuilder as Default>::default()
    }

    /// Create a new SmsTracker with required fields
    pub fn new(sms_uuid: String, state: MailNotificationStatus) -> Self {
        Self {
            id: Uuid::new_v4(),
            sms_uuid,
            message_id: None,
            notification_id: None,
            state,
            failure_type: None,
            failure_reason: None,
            recipient: None,
            metadata: AuditMetadata::default(),
        }
    }

    /// Get the entity's unique identifier
    pub fn id(&self) -> &Uuid {
        &self.id
    }

    /// Get a strongly-typed ID for this entity
    pub fn typed_id(&self) -> SmsTrackerId {
        SmsTrackerId(self.id)
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

    /// Set the message_id field (chainable)
    pub fn with_message_id(mut self, value: Uuid) -> Self {
        self.message_id = Some(value);
        self
    }

    /// Set the notification_id field (chainable)
    pub fn with_notification_id(mut self, value: Uuid) -> Self {
        self.notification_id = Some(value);
        self
    }

    /// Set the failure_type field (chainable)
    pub fn with_failure_type(mut self, value: NotificationFailureType) -> Self {
        self.failure_type = Some(value);
        self
    }

    /// Set the failure_reason field (chainable)
    pub fn with_failure_reason(mut self, value: String) -> Self {
        self.failure_reason = Some(value);
        self
    }

    /// Set the recipient field (chainable)
    pub fn with_recipient(mut self, value: String) -> Self {
        self.recipient = Some(value);
        self
    }

    // ==========================================================
    // Partial Update
    // ==========================================================

    /// Apply partial updates from a map of field name to JSON value
    pub fn apply_patch(&mut self, fields: std::collections::HashMap<String, serde_json::Value>) {
        for (key, value) in fields {
            match key.as_str() {
                "sms_uuid" => {
                    if let Ok(v) = serde_json::from_value(value) { self.sms_uuid = v; }
                }
                "message_id" => {
                    if let Ok(v) = serde_json::from_value(value) { self.message_id = v; }
                }
                "notification_id" => {
                    if let Ok(v) = serde_json::from_value(value) { self.notification_id = v; }
                }
                "state" => {
                    if let Ok(v) = serde_json::from_value(value) { self.state = v; }
                }
                "failure_type" => {
                    if let Ok(v) = serde_json::from_value(value) { self.failure_type = v; }
                }
                "failure_reason" => {
                    if let Ok(v) = serde_json::from_value(value) { self.failure_reason = v; }
                }
                "recipient" => {
                    if let Ok(v) = serde_json::from_value(value) { self.recipient = v; }
                }
                _ => {} // ignore unknown fields
            }
        }
    }

    // <<< CUSTOM METHODS START >>>
    // <<< CUSTOM METHODS END >>>
}

impl super::Entity for SmsTracker {
    type Id = Uuid;

    fn entity_id(&self) -> &Self::Id {
        &self.id
    }

    fn entity_type() -> &'static str {
        "SmsTracker"
    }
}

impl backbone_core::PersistentEntity for SmsTracker {
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

impl backbone_orm::EntityRepoMeta for SmsTracker {
    fn column_types() -> std::collections::HashMap<String, String> {
        let mut m = std::collections::HashMap::new();
        m.insert("id".to_string(), "uuid".to_string());
        m.insert("message_id".to_string(), "uuid".to_string());
        m.insert("notification_id".to_string(), "uuid".to_string());
        m.insert("state".to_string(), "mail_notification_status".to_string());
        m.insert("failure_type".to_string(), "notification_failure_type".to_string());
        m
    }
    fn search_fields() -> &'static [&'static str] {
        &["sms_uuid"]
    }
}

/// Builder for SmsTracker entity
///
/// Provides a fluent API for constructing SmsTracker instances.
/// System fields (id, metadata, timestamps) are auto-initialized.
#[derive(Debug, Clone, Default)]
pub struct SmsTrackerBuilder {
    sms_uuid: Option<String>,
    message_id: Option<Uuid>,
    notification_id: Option<Uuid>,
    state: Option<MailNotificationStatus>,
    failure_type: Option<NotificationFailureType>,
    failure_reason: Option<String>,
    recipient: Option<String>,
}

impl SmsTrackerBuilder {
    /// Set the sms_uuid field (required)
    pub fn sms_uuid(mut self, value: String) -> Self {
        self.sms_uuid = Some(value);
        self
    }

    /// Set the message_id field (optional)
    pub fn message_id(mut self, value: Uuid) -> Self {
        self.message_id = Some(value);
        self
    }

    /// Set the notification_id field (optional)
    pub fn notification_id(mut self, value: Uuid) -> Self {
        self.notification_id = Some(value);
        self
    }

    /// Set the state field (default: `MailNotificationStatus::default()`)
    pub fn state(mut self, value: MailNotificationStatus) -> Self {
        self.state = Some(value);
        self
    }

    /// Set the failure_type field (optional)
    pub fn failure_type(mut self, value: NotificationFailureType) -> Self {
        self.failure_type = Some(value);
        self
    }

    /// Set the failure_reason field (optional)
    pub fn failure_reason(mut self, value: String) -> Self {
        self.failure_reason = Some(value);
        self
    }

    /// Set the recipient field (optional)
    pub fn recipient(mut self, value: String) -> Self {
        self.recipient = Some(value);
        self
    }

    /// Build the SmsTracker entity
    ///
    /// Returns Err if any required field without a default is missing.
    pub fn build(self) -> Result<SmsTracker, String> {
        let sms_uuid = self.sms_uuid.ok_or_else(|| "sms_uuid is required".to_string())?;

        Ok(SmsTracker {
            id: Uuid::new_v4(),
            sms_uuid,
            message_id: self.message_id,
            notification_id: self.notification_id,
            state: self.state.unwrap_or_default(),
            failure_type: self.failure_type,
            failure_reason: self.failure_reason,
            recipient: self.recipient,
            metadata: AuditMetadata::default(),
        })
    }
}
