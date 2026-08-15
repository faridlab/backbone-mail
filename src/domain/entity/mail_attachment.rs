use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use uuid::Uuid;
use super::AuditMetadata;

/// Strongly-typed ID for MailAttachment
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct MailAttachmentId(pub Uuid);

impl MailAttachmentId {
    pub fn new(id: Uuid) -> Self { Self(id) }
    pub fn generate() -> Self { Self(Uuid::new_v4()) }
    pub fn into_inner(self) -> Uuid { self.0 }
}

impl std::fmt::Display for MailAttachmentId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::str::FromStr for MailAttachmentId {
    type Err = uuid::Error;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(Self(Uuid::parse_str(s)?))
    }
}

impl From<Uuid> for MailAttachmentId {
    fn from(id: Uuid) -> Self { Self(id) }
}

impl From<MailAttachmentId> for Uuid {
    fn from(id: MailAttachmentId) -> Self { id.0 }
}

impl AsRef<Uuid> for MailAttachmentId {
    fn as_ref(&self) -> &Uuid { &self.0 }
}

impl std::ops::Deref for MailAttachmentId {
    type Target = Uuid;
    fn deref(&self) -> &Self::Target { &self.0 }
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct MailAttachment {
    pub id: Uuid,
    pub name: String,
    pub mimetype: Option<String>,
    pub size: Option<i32>,
    pub datas: Option<String>,
    pub checksum: Option<String>,
    pub access_token: Option<Uuid>,
    pub owner_party_id: Option<Uuid>,
    pub owner_guest_id: Option<Uuid>,
    #[serde(default)]
    #[sqlx(json)]
    pub metadata: AuditMetadata,
}

impl MailAttachment {
    /// Create a builder for MailAttachment
    pub fn builder() -> MailAttachmentBuilder {
        <MailAttachmentBuilder as Default>::default()
    }

    /// Create a new MailAttachment with required fields
    pub fn new(name: String) -> Self {
        Self {
            id: Uuid::new_v4(),
            name,
            mimetype: None,
            size: None,
            datas: None,
            checksum: None,
            access_token: None,
            owner_party_id: None,
            owner_guest_id: None,
            metadata: AuditMetadata::default(),
        }
    }

    /// Get the entity's unique identifier
    pub fn id(&self) -> &Uuid {
        &self.id
    }

    /// Get a strongly-typed ID for this entity
    pub fn typed_id(&self) -> MailAttachmentId {
        MailAttachmentId(self.id)
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

    /// Set the mimetype field (chainable)
    pub fn with_mimetype(mut self, value: String) -> Self {
        self.mimetype = Some(value);
        self
    }

    /// Set the size field (chainable)
    pub fn with_size(mut self, value: i32) -> Self {
        self.size = Some(value);
        self
    }

    /// Set the datas field (chainable)
    pub fn with_datas(mut self, value: String) -> Self {
        self.datas = Some(value);
        self
    }

    /// Set the checksum field (chainable)
    pub fn with_checksum(mut self, value: String) -> Self {
        self.checksum = Some(value);
        self
    }

    /// Set the access_token field (chainable)
    pub fn with_access_token(mut self, value: Uuid) -> Self {
        self.access_token = Some(value);
        self
    }

    /// Set the owner_party_id field (chainable)
    pub fn with_owner_party_id(mut self, value: Uuid) -> Self {
        self.owner_party_id = Some(value);
        self
    }

    /// Set the owner_guest_id field (chainable)
    pub fn with_owner_guest_id(mut self, value: Uuid) -> Self {
        self.owner_guest_id = Some(value);
        self
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
                "mimetype" => {
                    if let Ok(v) = serde_json::from_value(value) { self.mimetype = v; }
                }
                "size" => {
                    if let Ok(v) = serde_json::from_value(value) { self.size = v; }
                }
                "datas" => {
                    if let Ok(v) = serde_json::from_value(value) { self.datas = v; }
                }
                "checksum" => {
                    if let Ok(v) = serde_json::from_value(value) { self.checksum = v; }
                }
                "access_token" => {
                    if let Ok(v) = serde_json::from_value(value) { self.access_token = v; }
                }
                "owner_party_id" => {
                    if let Ok(v) = serde_json::from_value(value) { self.owner_party_id = v; }
                }
                "owner_guest_id" => {
                    if let Ok(v) = serde_json::from_value(value) { self.owner_guest_id = v; }
                }
                _ => {} // ignore unknown fields
            }
        }
    }

    // <<< CUSTOM METHODS START >>>
    // <<< CUSTOM METHODS END >>>
}

impl super::Entity for MailAttachment {
    type Id = Uuid;

    fn entity_id(&self) -> &Self::Id {
        &self.id
    }

    fn entity_type() -> &'static str {
        "MailAttachment"
    }
}

impl backbone_core::PersistentEntity for MailAttachment {
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

impl backbone_orm::EntityRepoMeta for MailAttachment {
    fn column_types() -> std::collections::HashMap<String, String> {
        let mut m = std::collections::HashMap::new();
        m.insert("id".to_string(), "uuid".to_string());
        m.insert("owner_party_id".to_string(), "uuid".to_string());
        m.insert("owner_guest_id".to_string(), "uuid".to_string());
        m
    }
    fn search_fields() -> &'static [&'static str] {
        &["name"]
    }
}

/// Builder for MailAttachment entity
///
/// Provides a fluent API for constructing MailAttachment instances.
/// System fields (id, metadata, timestamps) are auto-initialized.
#[derive(Debug, Clone, Default)]
pub struct MailAttachmentBuilder {
    name: Option<String>,
    mimetype: Option<String>,
    size: Option<i32>,
    datas: Option<String>,
    checksum: Option<String>,
    access_token: Option<Uuid>,
    owner_party_id: Option<Uuid>,
    owner_guest_id: Option<Uuid>,
}

impl MailAttachmentBuilder {
    /// Set the name field (required)
    pub fn name(mut self, value: String) -> Self {
        self.name = Some(value);
        self
    }

    /// Set the mimetype field (optional)
    pub fn mimetype(mut self, value: String) -> Self {
        self.mimetype = Some(value);
        self
    }

    /// Set the size field (optional)
    pub fn size(mut self, value: i32) -> Self {
        self.size = Some(value);
        self
    }

    /// Set the datas field (optional)
    pub fn datas(mut self, value: String) -> Self {
        self.datas = Some(value);
        self
    }

    /// Set the checksum field (optional)
    pub fn checksum(mut self, value: String) -> Self {
        self.checksum = Some(value);
        self
    }

    /// Set the access_token field (optional)
    pub fn access_token(mut self, value: Uuid) -> Self {
        self.access_token = Some(value);
        self
    }

    /// Set the owner_party_id field (optional)
    pub fn owner_party_id(mut self, value: Uuid) -> Self {
        self.owner_party_id = Some(value);
        self
    }

    /// Set the owner_guest_id field (optional)
    pub fn owner_guest_id(mut self, value: Uuid) -> Self {
        self.owner_guest_id = Some(value);
        self
    }

    /// Build the MailAttachment entity
    ///
    /// Returns Err if any required field without a default is missing.
    pub fn build(self) -> Result<MailAttachment, String> {
        let name = self.name.ok_or_else(|| "name is required".to_string())?;

        Ok(MailAttachment {
            id: Uuid::new_v4(),
            name,
            mimetype: self.mimetype,
            size: self.size,
            datas: self.datas,
            checksum: self.checksum,
            access_token: self.access_token,
            owner_party_id: self.owner_party_id,
            owner_guest_id: self.owner_guest_id,
            metadata: AuditMetadata::default(),
        })
    }
}
