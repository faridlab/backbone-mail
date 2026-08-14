use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use uuid::Uuid;

use super::SmsState;
use super::SmsFailureType;
use super::AuditMetadata;

use crate::domain::state_machine::{SmsStateMachine, SmsState, StateMachineError};

/// Strongly-typed ID for Sms
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct SmsId(pub Uuid);

impl SmsId {
    pub fn new(id: Uuid) -> Self { Self(id) }
    pub fn generate() -> Self { Self(Uuid::new_v4()) }
    pub fn into_inner(self) -> Uuid { self.0 }
}

impl std::fmt::Display for SmsId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::str::FromStr for SmsId {
    type Err = uuid::Error;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(Self(Uuid::parse_str(s)?))
    }
}

impl From<Uuid> for SmsId {
    fn from(id: Uuid) -> Self { Self(id) }
}

impl From<SmsId> for Uuid {
    fn from(id: SmsId) -> Self { id.0 }
}

impl AsRef<Uuid> for SmsId {
    fn as_ref(&self) -> &Uuid { &self.0 }
}

impl std::ops::Deref for SmsId {
    type Target = Uuid;
    fn deref(&self) -> &Self::Target { &self.0 }
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct Sms {
    pub id: Uuid,
    pub uuid: String,
    pub number: String,
    pub body: String,
    pub(crate) state: SmsState,
    pub failure_type: Option<SmsFailureType>,
    pub error_message: Option<String>,
    pub mail_message_id: Option<Uuid>,
    pub iap_status_code: Option<i32>,
    #[serde(default)]
    #[sqlx(json)]
    pub metadata: AuditMetadata,
}

impl Sms {
    /// Create a builder for Sms
    pub fn builder() -> SmsBuilder {
        SmsBuilder::default()
    }

    /// Create a new Sms with required fields
    pub fn new(uuid: String, number: String, body: String, state: SmsState) -> Self {
        Self {
            id: Uuid::new_v4(),
            uuid,
            number,
            body,
            state,
            failure_type: None,
            error_message: None,
            mail_message_id: None,
            iap_status_code: None,
            metadata: AuditMetadata::default(),
        }
    }

    /// Get the entity's unique identifier
    pub fn id(&self) -> &Uuid {
        &self.id
    }

    /// Get a strongly-typed ID for this entity
    pub fn typed_id(&self) -> SmsId {
        SmsId(self.id)
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
    pub fn with_failure_type(mut self, value: SmsFailureType) -> Self {
        self.failure_type = Some(value);
        self
    }

    /// Set the error_message field (chainable)
    pub fn with_error_message(mut self, value: String) -> Self {
        self.error_message = Some(value);
        self
    }

    /// Set the mail_message_id field (chainable)
    pub fn with_mail_message_id(mut self, value: Uuid) -> Self {
        self.mail_message_id = Some(value);
        self
    }

    /// Set the iap_status_code field (chainable)
    pub fn with_iap_status_code(mut self, value: i32) -> Self {
        self.iap_status_code = Some(value);
        self
    }

    // ==========================================================
    // State Machine
    // ==========================================================

    /// Transition to a new state via the state state machine.
    ///
    /// Returns `Err` if the transition is not permitted from the current state.
    /// Use this method instead of assigning `self.state` directly.
    pub fn transition_to(&mut self, new_state: SmsState) -> Result<(), StateMachineError> {
        let current = self.state.to_string().parse::<SmsState>()?;
        let mut sm = SmsStateMachine::from_state(current);
        sm.transition_to_state(new_state)?;
        self.state = new_state.to_string().parse::<SmsState>()
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
                "uuid" => {
                    if let Ok(v) = serde_json::from_value(value) { self.uuid = v; }
                }
                "number" => {
                    if let Ok(v) = serde_json::from_value(value) { self.number = v; }
                }
                "body" => {
                    if let Ok(v) = serde_json::from_value(value) { self.body = v; }
                }
                "failure_type" => {
                    if let Ok(v) = serde_json::from_value(value) { self.failure_type = v; }
                }
                "error_message" => {
                    if let Ok(v) = serde_json::from_value(value) { self.error_message = v; }
                }
                "mail_message_id" => {
                    if let Ok(v) = serde_json::from_value(value) { self.mail_message_id = v; }
                }
                "iap_status_code" => {
                    if let Ok(v) = serde_json::from_value(value) { self.iap_status_code = v; }
                }
                _ => {} // ignore unknown fields
            }
        }
    }

    // <<< CUSTOM METHODS START >>>
    // <<< CUSTOM METHODS END >>>
}

impl super::Entity for Sms {
    type Id = Uuid;

    fn entity_id(&self) -> &Self::Id {
        &self.id
    }

    fn entity_type() -> &'static str {
        "Sms"
    }
}

impl backbone_core::PersistentEntity for Sms {
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

impl backbone_orm::EntityRepoMeta for Sms {
    fn column_types() -> std::collections::HashMap<String, String> {
        let mut m = std::collections::HashMap::new();
        m.insert("id".to_string(), "uuid".to_string());
        m.insert("mail_message_id".to_string(), "uuid".to_string());
        m.insert("state".to_string(), "sms_state".to_string());
        m.insert("failure_type".to_string(), "sms_failure_type".to_string());
        m
    }
    fn search_fields() -> &'static [&'static str] {
        &["uuid", "number", "body"]
    }
}

/// Builder for Sms entity
///
/// Provides a fluent API for constructing Sms instances.
/// System fields (id, metadata, timestamps) are auto-initialized.
#[derive(Debug, Clone, Default)]
pub struct SmsBuilder {
    uuid: Option<String>,
    number: Option<String>,
    body: Option<String>,
    state: Option<SmsState>,
    failure_type: Option<SmsFailureType>,
    error_message: Option<String>,
    mail_message_id: Option<Uuid>,
    iap_status_code: Option<i32>,
}

impl SmsBuilder {
    /// Set the uuid field (required)
    pub fn uuid(mut self, value: String) -> Self {
        self.uuid = Some(value);
        self
    }

    /// Set the number field (required)
    pub fn number(mut self, value: String) -> Self {
        self.number = Some(value);
        self
    }

    /// Set the body field (required)
    pub fn body(mut self, value: String) -> Self {
        self.body = Some(value);
        self
    }

    /// Set the state field (default: `SmsState::default()`)
    pub fn state(mut self, value: SmsState) -> Self {
        self.state = Some(value);
        self
    }

    /// Set the failure_type field (optional)
    pub fn failure_type(mut self, value: SmsFailureType) -> Self {
        self.failure_type = Some(value);
        self
    }

    /// Set the error_message field (optional)
    pub fn error_message(mut self, value: String) -> Self {
        self.error_message = Some(value);
        self
    }

    /// Set the mail_message_id field (optional)
    pub fn mail_message_id(mut self, value: Uuid) -> Self {
        self.mail_message_id = Some(value);
        self
    }

    /// Set the iap_status_code field (optional)
    pub fn iap_status_code(mut self, value: i32) -> Self {
        self.iap_status_code = Some(value);
        self
    }

    /// Build the Sms entity
    ///
    /// Returns Err if any required field without a default is missing.
    pub fn build(self) -> Result<Sms, String> {
        let uuid = self.uuid.ok_or_else(|| "uuid is required".to_string())?;
        let number = self.number.ok_or_else(|| "number is required".to_string())?;
        let body = self.body.ok_or_else(|| "body is required".to_string())?;

        Ok(Sms {
            id: Uuid::new_v4(),
            uuid,
            number,
            body,
            state: self.state.unwrap_or(SmsState::default()),
            failure_type: self.failure_type,
            error_message: self.error_message,
            mail_message_id: self.mail_message_id,
            iap_status_code: self.iap_status_code,
            metadata: AuditMetadata::default(),
        })
    }
}
