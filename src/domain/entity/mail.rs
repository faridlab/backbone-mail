use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use uuid::Uuid;

use super::MailState;
use super::MailFailureType;
use super::AuditMetadata;

use crate::domain::state_machine::{MailHooksStateMachine, MailHooksState, StateMachineError};

/// Strongly-typed ID for Mail
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct MailId(pub Uuid);

impl MailId {
    pub fn new(id: Uuid) -> Self { Self(id) }
    pub fn generate() -> Self { Self(Uuid::new_v4()) }
    pub fn into_inner(self) -> Uuid { self.0 }
}

impl std::fmt::Display for MailId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::str::FromStr for MailId {
    type Err = uuid::Error;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(Self(Uuid::parse_str(s)?))
    }
}

impl From<Uuid> for MailId {
    fn from(id: Uuid) -> Self { Self(id) }
}

impl From<MailId> for Uuid {
    fn from(id: MailId) -> Self { id.0 }
}

impl AsRef<Uuid> for MailId {
    fn as_ref(&self) -> &Uuid { &self.0 }
}

impl std::ops::Deref for MailId {
    type Target = Uuid;
    fn deref(&self) -> &Self::Target { &self.0 }
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct Mail {
    pub id: Uuid,
    pub mail_message_id: Uuid,
    pub(crate) state: MailState,
    pub failure_type: Option<MailFailureType>,
    pub scheduled_date: Option<DateTime<Utc>>,
    pub auto_delete: bool,
    pub failure_reason: Option<String>,
    pub email_to: Option<String>,
    pub email_cc: Option<String>,
    pub reply_to: Option<String>,
    #[serde(default)]
    #[sqlx(json)]
    pub metadata: AuditMetadata,
}

impl Mail {
    /// Create a builder for Mail
    pub fn builder() -> MailBuilder {
        <MailBuilder as Default>::default()
    }

    /// Create a new Mail with required fields
    pub fn new(mail_message_id: Uuid, state: MailState, auto_delete: bool) -> Self {
        Self {
            id: Uuid::new_v4(),
            mail_message_id,
            state,
            failure_type: None,
            scheduled_date: None,
            auto_delete,
            failure_reason: None,
            email_to: None,
            email_cc: None,
            reply_to: None,
            metadata: AuditMetadata::default(),
        }
    }

    /// Get the entity's unique identifier
    pub fn id(&self) -> &Uuid {
        &self.id
    }

    /// Get a strongly-typed ID for this entity
    pub fn typed_id(&self) -> MailId {
        MailId(self.id)
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

    /// Set the failure_type field (chainable)
    pub fn with_failure_type(mut self, value: MailFailureType) -> Self {
        self.failure_type = Some(value);
        self
    }

    /// Set the scheduled_date field (chainable)
    pub fn with_scheduled_date(mut self, value: DateTime<Utc>) -> Self {
        self.scheduled_date = Some(value);
        self
    }

    /// Set the failure_reason field (chainable)
    pub fn with_failure_reason(mut self, value: String) -> Self {
        self.failure_reason = Some(value);
        self
    }

    /// Set the email_to field (chainable)
    pub fn with_email_to(mut self, value: String) -> Self {
        self.email_to = Some(value);
        self
    }

    /// Set the email_cc field (chainable)
    pub fn with_email_cc(mut self, value: String) -> Self {
        self.email_cc = Some(value);
        self
    }

    /// Set the reply_to field (chainable)
    pub fn with_reply_to(mut self, value: String) -> Self {
        self.reply_to = Some(value);
        self
    }

    // ==========================================================
    // State Machine
    // ==========================================================

    /// Transition to a new state via the state state machine.
    ///
    /// Returns `Err` if the transition is not permitted from the current state.
    /// Use this method instead of assigning `self.state` directly.
    pub fn transition_to(&mut self, new_state: MailHooksState) -> Result<(), StateMachineError> {
        let current = self.state.to_string().parse::<MailHooksState>()?;
        let mut sm = MailHooksStateMachine::from_state(current);
        sm.transition_to_state(new_state)?;
        self.state = new_state.to_string().parse::<MailState>()
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
                "failure_type" => {
                    if let Ok(v) = serde_json::from_value(value) { self.failure_type = v; }
                }
                "scheduled_date" => {
                    if let Ok(v) = serde_json::from_value(value) { self.scheduled_date = v; }
                }
                "auto_delete" => {
                    if let Ok(v) = serde_json::from_value(value) { self.auto_delete = v; }
                }
                "failure_reason" => {
                    if let Ok(v) = serde_json::from_value(value) { self.failure_reason = v; }
                }
                "email_to" => {
                    if let Ok(v) = serde_json::from_value(value) { self.email_to = v; }
                }
                "email_cc" => {
                    if let Ok(v) = serde_json::from_value(value) { self.email_cc = v; }
                }
                "reply_to" => {
                    if let Ok(v) = serde_json::from_value(value) { self.reply_to = v; }
                }
                _ => {} // ignore unknown fields
            }
        }
    }

    // <<< CUSTOM METHODS START >>>
    // <<< CUSTOM METHODS END >>>
}

impl super::Entity for Mail {
    type Id = Uuid;

    fn entity_id(&self) -> &Self::Id {
        &self.id
    }

    fn entity_type() -> &'static str {
        "Mail"
    }
}

impl backbone_core::PersistentEntity for Mail {
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

impl backbone_orm::EntityRepoMeta for Mail {
    fn column_types() -> std::collections::HashMap<String, String> {
        let mut m = std::collections::HashMap::new();
        m.insert("id".to_string(), "uuid".to_string());
        m.insert("mail_message_id".to_string(), "uuid".to_string());
        m.insert("state".to_string(), "mail_state".to_string());
        m.insert("failure_type".to_string(), "mail_failure_type".to_string());
        m
    }
    fn search_fields() -> &'static [&'static str] {
        &[]
    }
}

/// Builder for Mail entity
///
/// Provides a fluent API for constructing Mail instances.
/// System fields (id, metadata, timestamps) are auto-initialized.
#[derive(Debug, Clone, Default)]
pub struct MailBuilder {
    mail_message_id: Option<Uuid>,
    state: Option<MailState>,
    failure_type: Option<MailFailureType>,
    scheduled_date: Option<DateTime<Utc>>,
    auto_delete: Option<bool>,
    failure_reason: Option<String>,
    email_to: Option<String>,
    email_cc: Option<String>,
    reply_to: Option<String>,
}

impl MailBuilder {
    /// Set the mail_message_id field (required)
    pub fn mail_message_id(mut self, value: Uuid) -> Self {
        self.mail_message_id = Some(value);
        self
    }

    /// Set the state field (default: `MailState::default()`)
    pub fn state(mut self, value: MailState) -> Self {
        self.state = Some(value);
        self
    }

    /// Set the failure_type field (optional)
    pub fn failure_type(mut self, value: MailFailureType) -> Self {
        self.failure_type = Some(value);
        self
    }

    /// Set the scheduled_date field (optional)
    pub fn scheduled_date(mut self, value: DateTime<Utc>) -> Self {
        self.scheduled_date = Some(value);
        self
    }

    /// Set the auto_delete field (default: `false`)
    pub fn auto_delete(mut self, value: bool) -> Self {
        self.auto_delete = Some(value);
        self
    }

    /// Set the failure_reason field (optional)
    pub fn failure_reason(mut self, value: String) -> Self {
        self.failure_reason = Some(value);
        self
    }

    /// Set the email_to field (optional)
    pub fn email_to(mut self, value: String) -> Self {
        self.email_to = Some(value);
        self
    }

    /// Set the email_cc field (optional)
    pub fn email_cc(mut self, value: String) -> Self {
        self.email_cc = Some(value);
        self
    }

    /// Set the reply_to field (optional)
    pub fn reply_to(mut self, value: String) -> Self {
        self.reply_to = Some(value);
        self
    }

    /// Build the Mail entity
    ///
    /// Returns Err if any required field without a default is missing.
    pub fn build(self) -> Result<Mail, String> {
        let mail_message_id = self.mail_message_id.ok_or_else(|| "mail_message_id is required".to_string())?;

        Ok(Mail {
            id: Uuid::new_v4(),
            mail_message_id,
            state: self.state.unwrap_or_default(),
            failure_type: self.failure_type,
            scheduled_date: self.scheduled_date,
            auto_delete: self.auto_delete.unwrap_or(false),
            failure_reason: self.failure_reason,
            email_to: self.email_to,
            email_cc: self.email_cc,
            reply_to: self.reply_to,
            metadata: AuditMetadata::default(),
        })
    }
}
