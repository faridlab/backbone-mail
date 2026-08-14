use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use uuid::Uuid;

use super::MailPresenceStatus;
use super::AuditMetadata;

use crate::domain::state_machine::{MailPresenceStateMachine, MailPresenceState, StateMachineError};

/// Strongly-typed ID for MailPresence
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct MailPresenceId(pub Uuid);

impl MailPresenceId {
    pub fn new(id: Uuid) -> Self { Self(id) }
    pub fn generate() -> Self { Self(Uuid::new_v4()) }
    pub fn into_inner(self) -> Uuid { self.0 }
}

impl std::fmt::Display for MailPresenceId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::str::FromStr for MailPresenceId {
    type Err = uuid::Error;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(Self(Uuid::parse_str(s)?))
    }
}

impl From<Uuid> for MailPresenceId {
    fn from(id: Uuid) -> Self { Self(id) }
}

impl From<MailPresenceId> for Uuid {
    fn from(id: MailPresenceId) -> Self { id.0 }
}

impl AsRef<Uuid> for MailPresenceId {
    fn as_ref(&self) -> &Uuid { &self.0 }
}

impl std::ops::Deref for MailPresenceId {
    type Target = Uuid;
    fn deref(&self) -> &Self::Target { &self.0 }
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct MailPresence {
    pub id: Uuid,
    pub user_id: Option<Uuid>,
    pub guest_id: Option<Uuid>,
    pub(crate) status: MailPresenceStatus,
    pub last_poll: Option<DateTime<Utc>>,
    #[serde(default)]
    #[sqlx(json)]
    pub metadata: AuditMetadata,
}

impl MailPresence {
    /// Create a builder for MailPresence
    pub fn builder() -> MailPresenceBuilder {
        MailPresenceBuilder::default()
    }

    /// Create a new MailPresence with required fields
    pub fn new(status: MailPresenceStatus) -> Self {
        Self {
            id: Uuid::new_v4(),
            user_id: None,
            guest_id: None,
            status,
            last_poll: None,
            metadata: AuditMetadata::default(),
        }
    }

    /// Get the entity's unique identifier
    pub fn id(&self) -> &Uuid {
        &self.id
    }

    /// Get a strongly-typed ID for this entity
    pub fn typed_id(&self) -> MailPresenceId {
        MailPresenceId(self.id)
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

    /// Get the current status
    pub fn status(&self) -> &MailPresenceStatus {
        &self.status
    }


    // ==========================================================
    // Fluent Setters (with_* for optional fields)
    // ==========================================================

    /// Set the user_id field (chainable)
    pub fn with_user_id(mut self, value: Uuid) -> Self {
        self.user_id = Some(value);
        self
    }

    /// Set the guest_id field (chainable)
    pub fn with_guest_id(mut self, value: Uuid) -> Self {
        self.guest_id = Some(value);
        self
    }

    /// Set the last_poll field (chainable)
    pub fn with_last_poll(mut self, value: DateTime<Utc>) -> Self {
        self.last_poll = Some(value);
        self
    }

    // ==========================================================
    // State Machine
    // ==========================================================

    /// Transition to a new state via the status state machine.
    ///
    /// Returns `Err` if the transition is not permitted from the current state.
    /// Use this method instead of assigning `self.status` directly.
    pub fn transition_to(&mut self, new_state: MailPresenceState) -> Result<(), StateMachineError> {
        let current = self.status.to_string().parse::<MailPresenceState>()?;
        let mut sm = MailPresenceStateMachine::from_state(current);
        sm.transition_to_state(new_state)?;
        self.status = new_state.to_string().parse::<MailPresenceStatus>()
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
                "user_id" => {
                    if let Ok(v) = serde_json::from_value(value) { self.user_id = v; }
                }
                "guest_id" => {
                    if let Ok(v) = serde_json::from_value(value) { self.guest_id = v; }
                }
                "last_poll" => {
                    if let Ok(v) = serde_json::from_value(value) { self.last_poll = v; }
                }
                _ => {} // ignore unknown fields
            }
        }
    }

    // <<< CUSTOM METHODS START >>>
    // <<< CUSTOM METHODS END >>>
}

impl super::Entity for MailPresence {
    type Id = Uuid;

    fn entity_id(&self) -> &Self::Id {
        &self.id
    }

    fn entity_type() -> &'static str {
        "MailPresence"
    }
}

impl backbone_core::PersistentEntity for MailPresence {
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

impl backbone_orm::EntityRepoMeta for MailPresence {
    fn column_types() -> std::collections::HashMap<String, String> {
        let mut m = std::collections::HashMap::new();
        m.insert("id".to_string(), "uuid".to_string());
        m.insert("user_id".to_string(), "uuid".to_string());
        m.insert("guest_id".to_string(), "uuid".to_string());
        m.insert("status".to_string(), "mail_presence_status".to_string());
        m
    }
    fn search_fields() -> &'static [&'static str] {
        &[]
    }
}

/// Builder for MailPresence entity
///
/// Provides a fluent API for constructing MailPresence instances.
/// System fields (id, metadata, timestamps) are auto-initialized.
#[derive(Debug, Clone, Default)]
pub struct MailPresenceBuilder {
    user_id: Option<Uuid>,
    guest_id: Option<Uuid>,
    status: Option<MailPresenceStatus>,
    last_poll: Option<DateTime<Utc>>,
}

impl MailPresenceBuilder {
    /// Set the user_id field (optional)
    pub fn user_id(mut self, value: Uuid) -> Self {
        self.user_id = Some(value);
        self
    }

    /// Set the guest_id field (optional)
    pub fn guest_id(mut self, value: Uuid) -> Self {
        self.guest_id = Some(value);
        self
    }

    /// Set the status field (default: `MailPresenceStatus::default()`)
    pub fn status(mut self, value: MailPresenceStatus) -> Self {
        self.status = Some(value);
        self
    }

    /// Set the last_poll field (optional)
    pub fn last_poll(mut self, value: DateTime<Utc>) -> Self {
        self.last_poll = Some(value);
        self
    }

    /// Build the MailPresence entity
    ///
    /// Returns Err if any required field without a default is missing.
    pub fn build(self) -> Result<MailPresence, String> {

        Ok(MailPresence {
            id: Uuid::new_v4(),
            user_id: self.user_id,
            guest_id: self.guest_id,
            status: self.status.unwrap_or(MailPresenceStatus::default()),
            last_poll: self.last_poll,
            metadata: AuditMetadata::default(),
        })
    }
}
