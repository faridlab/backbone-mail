use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use uuid::Uuid;

use super::DiscussChannelType;
use super::AuditMetadata;

use crate::domain::state_machine::{DiscussChannelStateMachine, DiscussChannelState, StateMachineError};

/// Strongly-typed ID for DiscussChannel
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct DiscussChannelId(pub Uuid);

impl DiscussChannelId {
    pub fn new(id: Uuid) -> Self { Self(id) }
    pub fn generate() -> Self { Self(Uuid::new_v4()) }
    pub fn into_inner(self) -> Uuid { self.0 }
}

impl std::fmt::Display for DiscussChannelId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::str::FromStr for DiscussChannelId {
    type Err = uuid::Error;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(Self(Uuid::parse_str(s)?))
    }
}

impl From<Uuid> for DiscussChannelId {
    fn from(id: Uuid) -> Self { Self(id) }
}

impl From<DiscussChannelId> for Uuid {
    fn from(id: DiscussChannelId) -> Self { id.0 }
}

impl AsRef<Uuid> for DiscussChannelId {
    fn as_ref(&self) -> &Uuid { &self.0 }
}

impl std::ops::Deref for DiscussChannelId {
    type Target = Uuid;
    fn deref(&self) -> &Self::Target { &self.0 }
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct DiscussChannel {
    pub id: Uuid,
    pub name: Option<String>,
    pub(crate) channel_type: DiscussChannelType,
    pub description: Option<String>,
    pub uuid: Option<String>,
    pub default_access_mode: Option<String>,
    pub avatar_128: Option<String>,
    pub email_send: bool,
    pub moderation: bool,
    pub group_public_id: Option<Uuid>,
    pub alias_id: Option<Uuid>,
    pub last_message_id: Option<Uuid>,
    #[serde(default)]
    #[sqlx(json)]
    pub metadata: AuditMetadata,
}

impl DiscussChannel {
    /// Create a builder for DiscussChannel
    pub fn builder() -> DiscussChannelBuilder {
        DiscussChannelBuilder::default()
    }

    /// Create a new DiscussChannel with required fields
    pub fn new(channel_type: DiscussChannelType, email_send: bool, moderation: bool) -> Self {
        Self {
            id: Uuid::new_v4(),
            name: None,
            channel_type,
            description: None,
            uuid: None,
            default_access_mode: None,
            avatar_128: None,
            email_send,
            moderation,
            group_public_id: None,
            alias_id: None,
            last_message_id: None,
            metadata: AuditMetadata::default(),
        }
    }

    /// Get the entity's unique identifier
    pub fn id(&self) -> &Uuid {
        &self.id
    }

    /// Get a strongly-typed ID for this entity
    pub fn typed_id(&self) -> DiscussChannelId {
        DiscussChannelId(self.id)
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

    /// Set the name field (chainable)
    pub fn with_name(mut self, value: String) -> Self {
        self.name = Some(value);
        self
    }

    /// Set the description field (chainable)
    pub fn with_description(mut self, value: String) -> Self {
        self.description = Some(value);
        self
    }

    /// Set the uuid field (chainable)
    pub fn with_uuid(mut self, value: String) -> Self {
        self.uuid = Some(value);
        self
    }

    /// Set the default_access_mode field (chainable)
    pub fn with_default_access_mode(mut self, value: String) -> Self {
        self.default_access_mode = Some(value);
        self
    }

    /// Set the avatar_128 field (chainable)
    pub fn with_avatar_128(mut self, value: String) -> Self {
        self.avatar_128 = Some(value);
        self
    }

    /// Set the group_public_id field (chainable)
    pub fn with_group_public_id(mut self, value: Uuid) -> Self {
        self.group_public_id = Some(value);
        self
    }

    /// Set the alias_id field (chainable)
    pub fn with_alias_id(mut self, value: Uuid) -> Self {
        self.alias_id = Some(value);
        self
    }

    /// Set the last_message_id field (chainable)
    pub fn with_last_message_id(mut self, value: Uuid) -> Self {
        self.last_message_id = Some(value);
        self
    }

    // ==========================================================
    // State Machine
    // ==========================================================

    /// Transition to a new state via the channel_type state machine.
    ///
    /// Returns `Err` if the transition is not permitted from the current state.
    /// Use this method instead of assigning `self.channel_type` directly.
    pub fn transition_to(&mut self, new_state: DiscussChannelState) -> Result<(), StateMachineError> {
        let current = self.channel_type.to_string().parse::<DiscussChannelState>()?;
        let mut sm = DiscussChannelStateMachine::from_state(current);
        sm.transition_to_state(new_state)?;
        self.channel_type = new_state.to_string().parse::<DiscussChannelType>()
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
                "description" => {
                    if let Ok(v) = serde_json::from_value(value) { self.description = v; }
                }
                "uuid" => {
                    if let Ok(v) = serde_json::from_value(value) { self.uuid = v; }
                }
                "default_access_mode" => {
                    if let Ok(v) = serde_json::from_value(value) { self.default_access_mode = v; }
                }
                "avatar_128" => {
                    if let Ok(v) = serde_json::from_value(value) { self.avatar_128 = v; }
                }
                "email_send" => {
                    if let Ok(v) = serde_json::from_value(value) { self.email_send = v; }
                }
                "moderation" => {
                    if let Ok(v) = serde_json::from_value(value) { self.moderation = v; }
                }
                "group_public_id" => {
                    if let Ok(v) = serde_json::from_value(value) { self.group_public_id = v; }
                }
                "alias_id" => {
                    if let Ok(v) = serde_json::from_value(value) { self.alias_id = v; }
                }
                "last_message_id" => {
                    if let Ok(v) = serde_json::from_value(value) { self.last_message_id = v; }
                }
                _ => {} // ignore unknown fields
            }
        }
    }

    // <<< CUSTOM METHODS START >>>
    // <<< CUSTOM METHODS END >>>
}

impl super::Entity for DiscussChannel {
    type Id = Uuid;

    fn entity_id(&self) -> &Self::Id {
        &self.id
    }

    fn entity_type() -> &'static str {
        "DiscussChannel"
    }
}

impl backbone_core::PersistentEntity for DiscussChannel {
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

impl backbone_orm::EntityRepoMeta for DiscussChannel {
    fn column_types() -> std::collections::HashMap<String, String> {
        let mut m = std::collections::HashMap::new();
        m.insert("id".to_string(), "uuid".to_string());
        m.insert("group_public_id".to_string(), "uuid".to_string());
        m.insert("alias_id".to_string(), "uuid".to_string());
        m.insert("last_message_id".to_string(), "uuid".to_string());
        m.insert("channel_type".to_string(), "discuss_channel_type".to_string());
        m
    }
    fn search_fields() -> &'static [&'static str] {
        &[]
    }
}

/// Builder for DiscussChannel entity
///
/// Provides a fluent API for constructing DiscussChannel instances.
/// System fields (id, metadata, timestamps) are auto-initialized.
#[derive(Debug, Clone, Default)]
pub struct DiscussChannelBuilder {
    name: Option<String>,
    channel_type: Option<DiscussChannelType>,
    description: Option<String>,
    uuid: Option<String>,
    default_access_mode: Option<String>,
    avatar_128: Option<String>,
    email_send: Option<bool>,
    moderation: Option<bool>,
    group_public_id: Option<Uuid>,
    alias_id: Option<Uuid>,
    last_message_id: Option<Uuid>,
}

impl DiscussChannelBuilder {
    /// Set the name field (optional)
    pub fn name(mut self, value: String) -> Self {
        self.name = Some(value);
        self
    }

    /// Set the channel_type field (default: `DiscussChannelType::default()`)
    pub fn channel_type(mut self, value: DiscussChannelType) -> Self {
        self.channel_type = Some(value);
        self
    }

    /// Set the description field (optional)
    pub fn description(mut self, value: String) -> Self {
        self.description = Some(value);
        self
    }

    /// Set the uuid field (optional)
    pub fn uuid(mut self, value: String) -> Self {
        self.uuid = Some(value);
        self
    }

    /// Set the default_access_mode field (optional)
    pub fn default_access_mode(mut self, value: String) -> Self {
        self.default_access_mode = Some(value);
        self
    }

    /// Set the avatar_128 field (optional)
    pub fn avatar_128(mut self, value: String) -> Self {
        self.avatar_128 = Some(value);
        self
    }

    /// Set the email_send field (default: `false`)
    pub fn email_send(mut self, value: bool) -> Self {
        self.email_send = Some(value);
        self
    }

    /// Set the moderation field (default: `false`)
    pub fn moderation(mut self, value: bool) -> Self {
        self.moderation = Some(value);
        self
    }

    /// Set the group_public_id field (optional)
    pub fn group_public_id(mut self, value: Uuid) -> Self {
        self.group_public_id = Some(value);
        self
    }

    /// Set the alias_id field (optional)
    pub fn alias_id(mut self, value: Uuid) -> Self {
        self.alias_id = Some(value);
        self
    }

    /// Set the last_message_id field (optional)
    pub fn last_message_id(mut self, value: Uuid) -> Self {
        self.last_message_id = Some(value);
        self
    }

    /// Build the DiscussChannel entity
    ///
    /// Returns Err if any required field without a default is missing.
    pub fn build(self) -> Result<DiscussChannel, String> {

        Ok(DiscussChannel {
            id: Uuid::new_v4(),
            name: self.name,
            channel_type: self.channel_type.unwrap_or(DiscussChannelType::default()),
            description: self.description,
            uuid: self.uuid,
            default_access_mode: self.default_access_mode,
            avatar_128: self.avatar_128,
            email_send: self.email_send.unwrap_or(false),
            moderation: self.moderation.unwrap_or(false),
            group_public_id: self.group_public_id,
            alias_id: self.alias_id,
            last_message_id: self.last_message_id,
            metadata: AuditMetadata::default(),
        })
    }
}
