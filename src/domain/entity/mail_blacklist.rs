use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use uuid::Uuid;
use super::AuditMetadata;

/// Strongly-typed ID for MailBlacklist
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct MailBlacklistId(pub Uuid);

impl MailBlacklistId {
    pub fn new(id: Uuid) -> Self { Self(id) }
    pub fn generate() -> Self { Self(Uuid::new_v4()) }
    pub fn into_inner(self) -> Uuid { self.0 }
}

impl std::fmt::Display for MailBlacklistId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::str::FromStr for MailBlacklistId {
    type Err = uuid::Error;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(Self(Uuid::parse_str(s)?))
    }
}

impl From<Uuid> for MailBlacklistId {
    fn from(id: Uuid) -> Self { Self(id) }
}

impl From<MailBlacklistId> for Uuid {
    fn from(id: MailBlacklistId) -> Self { id.0 }
}

impl AsRef<Uuid> for MailBlacklistId {
    fn as_ref(&self) -> &Uuid { &self.0 }
}

impl std::ops::Deref for MailBlacklistId {
    type Target = Uuid;
    fn deref(&self) -> &Self::Target { &self.0 }
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct MailBlacklist {
    pub id: Uuid,
    pub email: String,
    pub opt_out_reason_id: Option<Uuid>,
    pub active: bool,
    #[serde(default)]
    #[sqlx(json)]
    pub metadata: AuditMetadata,
}

impl MailBlacklist {
    /// Create a builder for MailBlacklist
    pub fn builder() -> MailBlacklistBuilder {
        <MailBlacklistBuilder as Default>::default()
    }

    /// Create a new MailBlacklist with required fields
    pub fn new(email: String, active: bool) -> Self {
        Self {
            id: Uuid::new_v4(),
            email,
            opt_out_reason_id: None,
            active,
            metadata: AuditMetadata::default(),
        }
    }

    /// Get the entity's unique identifier
    pub fn id(&self) -> &Uuid {
        &self.id
    }

    /// Get a strongly-typed ID for this entity
    pub fn typed_id(&self) -> MailBlacklistId {
        MailBlacklistId(self.id)
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

    /// Set the opt_out_reason_id field (chainable)
    pub fn with_opt_out_reason_id(mut self, value: Uuid) -> Self {
        self.opt_out_reason_id = Some(value);
        self
    }

    // ==========================================================
    // Partial Update
    // ==========================================================

    /// Apply partial updates from a map of field name to JSON value
    pub fn apply_patch(&mut self, fields: std::collections::HashMap<String, serde_json::Value>) {
        for (key, value) in fields {
            match key.as_str() {
                "email" => {
                    if let Ok(v) = serde_json::from_value(value) { self.email = v; }
                }
                "opt_out_reason_id" => {
                    if let Ok(v) = serde_json::from_value(value) { self.opt_out_reason_id = v; }
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

impl super::Entity for MailBlacklist {
    type Id = Uuid;

    fn entity_id(&self) -> &Self::Id {
        &self.id
    }

    fn entity_type() -> &'static str {
        "MailBlacklist"
    }
}

impl backbone_core::PersistentEntity for MailBlacklist {
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

impl backbone_orm::EntityRepoMeta for MailBlacklist {
    fn column_types() -> std::collections::HashMap<String, String> {
        let mut m = std::collections::HashMap::new();
        m.insert("id".to_string(), "uuid".to_string());
        m.insert("opt_out_reason_id".to_string(), "uuid".to_string());
        m
    }
    fn search_fields() -> &'static [&'static str] {
        &["email"]
    }
}

/// Builder for MailBlacklist entity
///
/// Provides a fluent API for constructing MailBlacklist instances.
/// System fields (id, metadata, timestamps) are auto-initialized.
#[derive(Debug, Clone, Default)]
pub struct MailBlacklistBuilder {
    email: Option<String>,
    opt_out_reason_id: Option<Uuid>,
    active: Option<bool>,
}

impl MailBlacklistBuilder {
    /// Set the email field (required)
    pub fn email(mut self, value: String) -> Self {
        self.email = Some(value);
        self
    }

    /// Set the opt_out_reason_id field (optional)
    pub fn opt_out_reason_id(mut self, value: Uuid) -> Self {
        self.opt_out_reason_id = Some(value);
        self
    }

    /// Set the active field (default: `true`)
    pub fn active(mut self, value: bool) -> Self {
        self.active = Some(value);
        self
    }

    /// Build the MailBlacklist entity
    ///
    /// Returns Err if any required field without a default is missing.
    pub fn build(self) -> Result<MailBlacklist, String> {
        let email = self.email.ok_or_else(|| "email is required".to_string())?;

        Ok(MailBlacklist {
            id: Uuid::new_v4(),
            email,
            opt_out_reason_id: self.opt_out_reason_id,
            active: self.active.unwrap_or(true),
            metadata: AuditMetadata::default(),
        })
    }
}
