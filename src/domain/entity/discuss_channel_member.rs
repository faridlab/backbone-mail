use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use uuid::Uuid;

use super::SidebarFoldState;
use super::AuditMetadata;

/// Strongly-typed ID for DiscussChannelMember
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct DiscussChannelMemberId(pub Uuid);

impl DiscussChannelMemberId {
    pub fn new(id: Uuid) -> Self { Self(id) }
    pub fn generate() -> Self { Self(Uuid::new_v4()) }
    pub fn into_inner(self) -> Uuid { self.0 }
}

impl std::fmt::Display for DiscussChannelMemberId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::str::FromStr for DiscussChannelMemberId {
    type Err = uuid::Error;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(Self(Uuid::parse_str(s)?))
    }
}

impl From<Uuid> for DiscussChannelMemberId {
    fn from(id: Uuid) -> Self { Self(id) }
}

impl From<DiscussChannelMemberId> for Uuid {
    fn from(id: DiscussChannelMemberId) -> Self { id.0 }
}

impl AsRef<Uuid> for DiscussChannelMemberId {
    fn as_ref(&self) -> &Uuid { &self.0 }
}

impl std::ops::Deref for DiscussChannelMemberId {
    type Target = Uuid;
    fn deref(&self) -> &Self::Target { &self.0 }
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct DiscussChannelMember {
    pub id: Uuid,
    pub channel_id: Uuid,
    pub partner_id: Option<Uuid>,
    pub guest_id: Option<Uuid>,
    pub fold_state: Option<SidebarFoldState>,
    pub message_unread_counter: i32,
    pub unread_counter: i32,
    pub message_follower_counter: i32,
    pub seen_message_id: Option<Uuid>,
    pub last_interest_dt: Option<DateTime<Utc>>,
    pub is_pinned: bool,
    #[serde(default)]
    #[sqlx(json)]
    pub metadata: AuditMetadata,
}

impl DiscussChannelMember {
    /// Create a builder for DiscussChannelMember
    pub fn builder() -> DiscussChannelMemberBuilder {
        DiscussChannelMemberBuilder::default()
    }

    /// Create a new DiscussChannelMember with required fields
    pub fn new(channel_id: Uuid, message_unread_counter: i32, unread_counter: i32, message_follower_counter: i32, is_pinned: bool) -> Self {
        Self {
            id: Uuid::new_v4(),
            channel_id,
            partner_id: None,
            guest_id: None,
            fold_state: None,
            message_unread_counter,
            unread_counter,
            message_follower_counter,
            seen_message_id: None,
            last_interest_dt: None,
            is_pinned,
            metadata: AuditMetadata::default(),
        }
    }

    /// Get the entity's unique identifier
    pub fn id(&self) -> &Uuid {
        &self.id
    }

    /// Get a strongly-typed ID for this entity
    pub fn typed_id(&self) -> DiscussChannelMemberId {
        DiscussChannelMemberId(self.id)
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

    /// Set the partner_id field (chainable)
    pub fn with_partner_id(mut self, value: Uuid) -> Self {
        self.partner_id = Some(value);
        self
    }

    /// Set the guest_id field (chainable)
    pub fn with_guest_id(mut self, value: Uuid) -> Self {
        self.guest_id = Some(value);
        self
    }

    /// Set the fold_state field (chainable)
    pub fn with_fold_state(mut self, value: SidebarFoldState) -> Self {
        self.fold_state = Some(value);
        self
    }

    /// Set the seen_message_id field (chainable)
    pub fn with_seen_message_id(mut self, value: Uuid) -> Self {
        self.seen_message_id = Some(value);
        self
    }

    /// Set the last_interest_dt field (chainable)
    pub fn with_last_interest_dt(mut self, value: DateTime<Utc>) -> Self {
        self.last_interest_dt = Some(value);
        self
    }

    // ==========================================================
    // Partial Update
    // ==========================================================

    /// Apply partial updates from a map of field name to JSON value
    pub fn apply_patch(&mut self, fields: std::collections::HashMap<String, serde_json::Value>) {
        for (key, value) in fields {
            match key.as_str() {
                "channel_id" => {
                    if let Ok(v) = serde_json::from_value(value) { self.channel_id = v; }
                }
                "partner_id" => {
                    if let Ok(v) = serde_json::from_value(value) { self.partner_id = v; }
                }
                "guest_id" => {
                    if let Ok(v) = serde_json::from_value(value) { self.guest_id = v; }
                }
                "fold_state" => {
                    if let Ok(v) = serde_json::from_value(value) { self.fold_state = v; }
                }
                "message_unread_counter" => {
                    if let Ok(v) = serde_json::from_value(value) { self.message_unread_counter = v; }
                }
                "unread_counter" => {
                    if let Ok(v) = serde_json::from_value(value) { self.unread_counter = v; }
                }
                "message_follower_counter" => {
                    if let Ok(v) = serde_json::from_value(value) { self.message_follower_counter = v; }
                }
                "seen_message_id" => {
                    if let Ok(v) = serde_json::from_value(value) { self.seen_message_id = v; }
                }
                "last_interest_dt" => {
                    if let Ok(v) = serde_json::from_value(value) { self.last_interest_dt = v; }
                }
                "is_pinned" => {
                    if let Ok(v) = serde_json::from_value(value) { self.is_pinned = v; }
                }
                _ => {} // ignore unknown fields
            }
        }
    }

    // <<< CUSTOM METHODS START >>>
    // <<< CUSTOM METHODS END >>>
}

impl super::Entity for DiscussChannelMember {
    type Id = Uuid;

    fn entity_id(&self) -> &Self::Id {
        &self.id
    }

    fn entity_type() -> &'static str {
        "DiscussChannelMember"
    }
}

impl backbone_core::PersistentEntity for DiscussChannelMember {
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

impl backbone_orm::EntityRepoMeta for DiscussChannelMember {
    fn column_types() -> std::collections::HashMap<String, String> {
        let mut m = std::collections::HashMap::new();
        m.insert("id".to_string(), "uuid".to_string());
        m.insert("channel_id".to_string(), "uuid".to_string());
        m.insert("partner_id".to_string(), "uuid".to_string());
        m.insert("guest_id".to_string(), "uuid".to_string());
        m.insert("seen_message_id".to_string(), "uuid".to_string());
        m.insert("fold_state".to_string(), "sidebar_fold_state".to_string());
        m
    }
    fn search_fields() -> &'static [&'static str] {
        &[]
    }
}

/// Builder for DiscussChannelMember entity
///
/// Provides a fluent API for constructing DiscussChannelMember instances.
/// System fields (id, metadata, timestamps) are auto-initialized.
#[derive(Debug, Clone, Default)]
pub struct DiscussChannelMemberBuilder {
    channel_id: Option<Uuid>,
    partner_id: Option<Uuid>,
    guest_id: Option<Uuid>,
    fold_state: Option<SidebarFoldState>,
    message_unread_counter: Option<i32>,
    unread_counter: Option<i32>,
    message_follower_counter: Option<i32>,
    seen_message_id: Option<Uuid>,
    last_interest_dt: Option<DateTime<Utc>>,
    is_pinned: Option<bool>,
}

impl DiscussChannelMemberBuilder {
    /// Set the channel_id field (required)
    pub fn channel_id(mut self, value: Uuid) -> Self {
        self.channel_id = Some(value);
        self
    }

    /// Set the partner_id field (optional)
    pub fn partner_id(mut self, value: Uuid) -> Self {
        self.partner_id = Some(value);
        self
    }

    /// Set the guest_id field (optional)
    pub fn guest_id(mut self, value: Uuid) -> Self {
        self.guest_id = Some(value);
        self
    }

    /// Set the fold_state field (optional)
    pub fn fold_state(mut self, value: SidebarFoldState) -> Self {
        self.fold_state = Some(value);
        self
    }

    /// Set the message_unread_counter field (default: `0`)
    pub fn message_unread_counter(mut self, value: i32) -> Self {
        self.message_unread_counter = Some(value);
        self
    }

    /// Set the unread_counter field (default: `0`)
    pub fn unread_counter(mut self, value: i32) -> Self {
        self.unread_counter = Some(value);
        self
    }

    /// Set the message_follower_counter field (default: `0`)
    pub fn message_follower_counter(mut self, value: i32) -> Self {
        self.message_follower_counter = Some(value);
        self
    }

    /// Set the seen_message_id field (optional)
    pub fn seen_message_id(mut self, value: Uuid) -> Self {
        self.seen_message_id = Some(value);
        self
    }

    /// Set the last_interest_dt field (optional)
    pub fn last_interest_dt(mut self, value: DateTime<Utc>) -> Self {
        self.last_interest_dt = Some(value);
        self
    }

    /// Set the is_pinned field (default: `false`)
    pub fn is_pinned(mut self, value: bool) -> Self {
        self.is_pinned = Some(value);
        self
    }

    /// Build the DiscussChannelMember entity
    ///
    /// Returns Err if any required field without a default is missing.
    pub fn build(self) -> Result<DiscussChannelMember, String> {
        let channel_id = self.channel_id.ok_or_else(|| "channel_id is required".to_string())?;

        Ok(DiscussChannelMember {
            id: Uuid::new_v4(),
            channel_id,
            partner_id: self.partner_id,
            guest_id: self.guest_id,
            fold_state: self.fold_state,
            message_unread_counter: self.message_unread_counter.unwrap_or(0),
            unread_counter: self.unread_counter.unwrap_or(0),
            message_follower_counter: self.message_follower_counter.unwrap_or(0),
            seen_message_id: self.seen_message_id,
            last_interest_dt: self.last_interest_dt,
            is_pinned: self.is_pinned.unwrap_or(false),
            metadata: AuditMetadata::default(),
        })
    }
}
