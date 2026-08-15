use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use uuid::Uuid;

use super::MailNotificationType;
use super::MailNotificationStatus;
use super::NotificationFailureType;
use super::AuditMetadata;

use crate::domain::state_machine::{MailNotificationHooksStateMachine, MailNotificationHooksState, StateMachineError};

/// Strongly-typed ID for MailNotification
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct MailNotificationId(pub Uuid);

impl MailNotificationId {
    pub fn new(id: Uuid) -> Self { Self(id) }
    pub fn generate() -> Self { Self(Uuid::new_v4()) }
    pub fn into_inner(self) -> Uuid { self.0 }
}

impl std::fmt::Display for MailNotificationId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::str::FromStr for MailNotificationId {
    type Err = uuid::Error;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(Self(Uuid::parse_str(s)?))
    }
}

impl From<Uuid> for MailNotificationId {
    fn from(id: Uuid) -> Self { Self(id) }
}

impl From<MailNotificationId> for Uuid {
    fn from(id: MailNotificationId) -> Self { id.0 }
}

impl AsRef<Uuid> for MailNotificationId {
    fn as_ref(&self) -> &Uuid { &self.0 }
}

impl std::ops::Deref for MailNotificationId {
    type Target = Uuid;
    fn deref(&self) -> &Self::Target { &self.0 }
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct MailNotification {
    pub id: Uuid,
    pub mail_message_id: Uuid,
    pub res_partner_id: Option<Uuid>,
    pub notification_type: MailNotificationType,
    pub(crate) notification_status: MailNotificationStatus,
    pub failure_type: Option<NotificationFailureType>,
    pub failure_reason: Option<String>,
    pub mail_mail_id_int: Option<Uuid>,
    pub is_read: bool,
    #[serde(default)]
    #[sqlx(json)]
    pub metadata: AuditMetadata,
}

impl MailNotification {
    /// Create a builder for MailNotification
    pub fn builder() -> MailNotificationBuilder {
        <MailNotificationBuilder as Default>::default()
    }

    /// Create a new MailNotification with required fields
    pub fn new(mail_message_id: Uuid, notification_type: MailNotificationType, notification_status: MailNotificationStatus, is_read: bool) -> Self {
        Self {
            id: Uuid::new_v4(),
            mail_message_id,
            res_partner_id: None,
            notification_type,
            notification_status,
            failure_type: None,
            failure_reason: None,
            mail_mail_id_int: None,
            is_read,
            metadata: AuditMetadata::default(),
        }
    }

    /// Get the entity's unique identifier
    pub fn id(&self) -> &Uuid {
        &self.id
    }

    /// Get a strongly-typed ID for this entity
    pub fn typed_id(&self) -> MailNotificationId {
        MailNotificationId(self.id)
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

    /// Set the res_partner_id field (chainable)
    pub fn with_res_partner_id(mut self, value: Uuid) -> Self {
        self.res_partner_id = Some(value);
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

    /// Set the mail_mail_id_int field (chainable)
    pub fn with_mail_mail_id_int(mut self, value: Uuid) -> Self {
        self.mail_mail_id_int = Some(value);
        self
    }

    // ==========================================================
    // State Machine
    // ==========================================================

    /// Transition to a new state via the notification_status state machine.
    ///
    /// Returns `Err` if the transition is not permitted from the current state.
    /// Use this method instead of assigning `self.notification_status` directly.
    pub fn transition_to(&mut self, new_state: MailNotificationHooksState) -> Result<(), StateMachineError> {
        let current = self.notification_status.to_string().parse::<MailNotificationHooksState>()?;
        let mut sm = MailNotificationHooksStateMachine::from_state(current);
        sm.transition_to_state(new_state)?;
        self.notification_status = new_state.to_string().parse::<MailNotificationStatus>()
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
                "mail_message_id" => {
                    if let Ok(v) = serde_json::from_value(value) { self.mail_message_id = v; }
                }
                "res_partner_id" => {
                    if let Ok(v) = serde_json::from_value(value) { self.res_partner_id = v; }
                }
                "notification_type" => {
                    if let Ok(v) = serde_json::from_value(value) { self.notification_type = v; }
                }
                "failure_type" => {
                    if let Ok(v) = serde_json::from_value(value) { self.failure_type = v; }
                }
                "failure_reason" => {
                    if let Ok(v) = serde_json::from_value(value) { self.failure_reason = v; }
                }
                "mail_mail_id_int" => {
                    if let Ok(v) = serde_json::from_value(value) { self.mail_mail_id_int = v; }
                }
                "is_read" => {
                    if let Ok(v) = serde_json::from_value(value) { self.is_read = v; }
                }
                _ => {} // ignore unknown fields
            }
        }
    }

    // <<< CUSTOM METHODS START >>>
    // <<< CUSTOM METHODS END >>>
}

impl super::Entity for MailNotification {
    type Id = Uuid;

    fn entity_id(&self) -> &Self::Id {
        &self.id
    }

    fn entity_type() -> &'static str {
        "MailNotification"
    }
}

impl backbone_core::PersistentEntity for MailNotification {
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

impl backbone_orm::EntityRepoMeta for MailNotification {
    fn column_types() -> std::collections::HashMap<String, String> {
        let mut m = std::collections::HashMap::new();
        m.insert("id".to_string(), "uuid".to_string());
        m.insert("mail_message_id".to_string(), "uuid".to_string());
        m.insert("res_partner_id".to_string(), "uuid".to_string());
        m.insert("notification_type".to_string(), "mail_notification_type".to_string());
        m.insert("notification_status".to_string(), "mail_notification_status".to_string());
        m.insert("failure_type".to_string(), "notification_failure_type".to_string());
        m
    }
    fn search_fields() -> &'static [&'static str] {
        &[]
    }
}

/// Builder for MailNotification entity
///
/// Provides a fluent API for constructing MailNotification instances.
/// System fields (id, metadata, timestamps) are auto-initialized.
#[derive(Debug, Clone, Default)]
pub struct MailNotificationBuilder {
    mail_message_id: Option<Uuid>,
    res_partner_id: Option<Uuid>,
    notification_type: Option<MailNotificationType>,
    notification_status: Option<MailNotificationStatus>,
    failure_type: Option<NotificationFailureType>,
    failure_reason: Option<String>,
    mail_mail_id_int: Option<Uuid>,
    is_read: Option<bool>,
}

impl MailNotificationBuilder {
    /// Set the mail_message_id field (required)
    pub fn mail_message_id(mut self, value: Uuid) -> Self {
        self.mail_message_id = Some(value);
        self
    }

    /// Set the res_partner_id field (optional)
    pub fn res_partner_id(mut self, value: Uuid) -> Self {
        self.res_partner_id = Some(value);
        self
    }

    /// Set the notification_type field (default: `MailNotificationType::default()`)
    pub fn notification_type(mut self, value: MailNotificationType) -> Self {
        self.notification_type = Some(value);
        self
    }

    /// Set the notification_status field (default: `MailNotificationStatus::default()`)
    pub fn notification_status(mut self, value: MailNotificationStatus) -> Self {
        self.notification_status = Some(value);
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

    /// Set the mail_mail_id_int field (optional)
    pub fn mail_mail_id_int(mut self, value: Uuid) -> Self {
        self.mail_mail_id_int = Some(value);
        self
    }

    /// Set the is_read field (default: `false`)
    pub fn is_read(mut self, value: bool) -> Self {
        self.is_read = Some(value);
        self
    }

    /// Build the MailNotification entity
    ///
    /// Returns Err if any required field without a default is missing.
    pub fn build(self) -> Result<MailNotification, String> {
        let mail_message_id = self.mail_message_id.ok_or_else(|| "mail_message_id is required".to_string())?;

        Ok(MailNotification {
            id: Uuid::new_v4(),
            mail_message_id,
            res_partner_id: self.res_partner_id,
            notification_type: self.notification_type.unwrap_or_default(),
            notification_status: self.notification_status.unwrap_or_default(),
            failure_type: self.failure_type,
            failure_reason: self.failure_reason,
            mail_mail_id_int: self.mail_mail_id_int,
            is_read: self.is_read.unwrap_or(false),
            metadata: AuditMetadata::default(),
        })
    }
}
