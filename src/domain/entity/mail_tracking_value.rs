use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use uuid::Uuid;
use super::AuditMetadata;

/// Strongly-typed ID for MailTrackingValue
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct MailTrackingValueId(pub Uuid);

impl MailTrackingValueId {
    pub fn new(id: Uuid) -> Self { Self(id) }
    pub fn generate() -> Self { Self(Uuid::new_v4()) }
    pub fn into_inner(self) -> Uuid { self.0 }
}

impl std::fmt::Display for MailTrackingValueId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::str::FromStr for MailTrackingValueId {
    type Err = uuid::Error;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(Self(Uuid::parse_str(s)?))
    }
}

impl From<Uuid> for MailTrackingValueId {
    fn from(id: Uuid) -> Self { Self(id) }
}

impl From<MailTrackingValueId> for Uuid {
    fn from(id: MailTrackingValueId) -> Self { id.0 }
}

impl AsRef<Uuid> for MailTrackingValueId {
    fn as_ref(&self) -> &Uuid { &self.0 }
}

impl std::ops::Deref for MailTrackingValueId {
    type Target = Uuid;
    fn deref(&self) -> &Self::Target { &self.0 }
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct MailTrackingValue {
    pub id: Uuid,
    pub mail_message_id: Uuid,
    pub field: String,
    pub field_desc: Option<String>,
    pub field_type: Option<String>,
    pub old_value_integer: Option<i32>,
    pub old_value_float: Option<f64>,
    pub old_value_text: Option<String>,
    pub old_value_datetime: Option<DateTime<Utc>>,
    pub new_value_integer: Option<i32>,
    pub new_value_float: Option<f64>,
    pub new_value_text: Option<String>,
    pub new_value_datetime: Option<DateTime<Utc>>,
    pub currency_id: Option<Uuid>,
    #[serde(default)]
    #[sqlx(json)]
    pub metadata: AuditMetadata,
}

impl MailTrackingValue {
    /// Create a builder for MailTrackingValue
    pub fn builder() -> MailTrackingValueBuilder {
        MailTrackingValueBuilder::default()
    }

    /// Create a new MailTrackingValue with required fields
    pub fn new(mail_message_id: Uuid, field: String) -> Self {
        Self {
            id: Uuid::new_v4(),
            mail_message_id,
            field,
            field_desc: None,
            field_type: None,
            old_value_integer: None,
            old_value_float: None,
            old_value_text: None,
            old_value_datetime: None,
            new_value_integer: None,
            new_value_float: None,
            new_value_text: None,
            new_value_datetime: None,
            currency_id: None,
            metadata: AuditMetadata::default(),
        }
    }

    /// Get the entity's unique identifier
    pub fn id(&self) -> &Uuid {
        &self.id
    }

    /// Get a strongly-typed ID for this entity
    pub fn typed_id(&self) -> MailTrackingValueId {
        MailTrackingValueId(self.id)
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

    /// Set the field_desc field (chainable)
    pub fn with_field_desc(mut self, value: String) -> Self {
        self.field_desc = Some(value);
        self
    }

    /// Set the field_type field (chainable)
    pub fn with_field_type(mut self, value: String) -> Self {
        self.field_type = Some(value);
        self
    }

    /// Set the old_value_integer field (chainable)
    pub fn with_old_value_integer(mut self, value: i32) -> Self {
        self.old_value_integer = Some(value);
        self
    }

    /// Set the old_value_float field (chainable)
    pub fn with_old_value_float(mut self, value: f64) -> Self {
        self.old_value_float = Some(value);
        self
    }

    /// Set the old_value_text field (chainable)
    pub fn with_old_value_text(mut self, value: String) -> Self {
        self.old_value_text = Some(value);
        self
    }

    /// Set the old_value_datetime field (chainable)
    pub fn with_old_value_datetime(mut self, value: DateTime<Utc>) -> Self {
        self.old_value_datetime = Some(value);
        self
    }

    /// Set the new_value_integer field (chainable)
    pub fn with_new_value_integer(mut self, value: i32) -> Self {
        self.new_value_integer = Some(value);
        self
    }

    /// Set the new_value_float field (chainable)
    pub fn with_new_value_float(mut self, value: f64) -> Self {
        self.new_value_float = Some(value);
        self
    }

    /// Set the new_value_text field (chainable)
    pub fn with_new_value_text(mut self, value: String) -> Self {
        self.new_value_text = Some(value);
        self
    }

    /// Set the new_value_datetime field (chainable)
    pub fn with_new_value_datetime(mut self, value: DateTime<Utc>) -> Self {
        self.new_value_datetime = Some(value);
        self
    }

    /// Set the currency_id field (chainable)
    pub fn with_currency_id(mut self, value: Uuid) -> Self {
        self.currency_id = Some(value);
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
                "field" => {
                    if let Ok(v) = serde_json::from_value(value) { self.field = v; }
                }
                "field_desc" => {
                    if let Ok(v) = serde_json::from_value(value) { self.field_desc = v; }
                }
                "field_type" => {
                    if let Ok(v) = serde_json::from_value(value) { self.field_type = v; }
                }
                "old_value_integer" => {
                    if let Ok(v) = serde_json::from_value(value) { self.old_value_integer = v; }
                }
                "old_value_float" => {
                    if let Ok(v) = serde_json::from_value(value) { self.old_value_float = v; }
                }
                "old_value_text" => {
                    if let Ok(v) = serde_json::from_value(value) { self.old_value_text = v; }
                }
                "old_value_datetime" => {
                    if let Ok(v) = serde_json::from_value(value) { self.old_value_datetime = v; }
                }
                "new_value_integer" => {
                    if let Ok(v) = serde_json::from_value(value) { self.new_value_integer = v; }
                }
                "new_value_float" => {
                    if let Ok(v) = serde_json::from_value(value) { self.new_value_float = v; }
                }
                "new_value_text" => {
                    if let Ok(v) = serde_json::from_value(value) { self.new_value_text = v; }
                }
                "new_value_datetime" => {
                    if let Ok(v) = serde_json::from_value(value) { self.new_value_datetime = v; }
                }
                "currency_id" => {
                    if let Ok(v) = serde_json::from_value(value) { self.currency_id = v; }
                }
                _ => {} // ignore unknown fields
            }
        }
    }

    // <<< CUSTOM METHODS START >>>
    // <<< CUSTOM METHODS END >>>
}

impl super::Entity for MailTrackingValue {
    type Id = Uuid;

    fn entity_id(&self) -> &Self::Id {
        &self.id
    }

    fn entity_type() -> &'static str {
        "MailTrackingValue"
    }
}

impl backbone_core::PersistentEntity for MailTrackingValue {
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

impl backbone_orm::EntityRepoMeta for MailTrackingValue {
    fn column_types() -> std::collections::HashMap<String, String> {
        let mut m = std::collections::HashMap::new();
        m.insert("id".to_string(), "uuid".to_string());
        m.insert("mail_message_id".to_string(), "uuid".to_string());
        m.insert("currency_id".to_string(), "uuid".to_string());
        m
    }
    fn search_fields() -> &'static [&'static str] {
        &["field"]
    }
}

/// Builder for MailTrackingValue entity
///
/// Provides a fluent API for constructing MailTrackingValue instances.
/// System fields (id, metadata, timestamps) are auto-initialized.
#[derive(Debug, Clone, Default)]
pub struct MailTrackingValueBuilder {
    mail_message_id: Option<Uuid>,
    field: Option<String>,
    field_desc: Option<String>,
    field_type: Option<String>,
    old_value_integer: Option<i32>,
    old_value_float: Option<f64>,
    old_value_text: Option<String>,
    old_value_datetime: Option<DateTime<Utc>>,
    new_value_integer: Option<i32>,
    new_value_float: Option<f64>,
    new_value_text: Option<String>,
    new_value_datetime: Option<DateTime<Utc>>,
    currency_id: Option<Uuid>,
}

impl MailTrackingValueBuilder {
    /// Set the mail_message_id field (required)
    pub fn mail_message_id(mut self, value: Uuid) -> Self {
        self.mail_message_id = Some(value);
        self
    }

    /// Set the field field (required)
    pub fn field(mut self, value: String) -> Self {
        self.field = Some(value);
        self
    }

    /// Set the field_desc field (optional)
    pub fn field_desc(mut self, value: String) -> Self {
        self.field_desc = Some(value);
        self
    }

    /// Set the field_type field (optional)
    pub fn field_type(mut self, value: String) -> Self {
        self.field_type = Some(value);
        self
    }

    /// Set the old_value_integer field (optional)
    pub fn old_value_integer(mut self, value: i32) -> Self {
        self.old_value_integer = Some(value);
        self
    }

    /// Set the old_value_float field (optional)
    pub fn old_value_float(mut self, value: f64) -> Self {
        self.old_value_float = Some(value);
        self
    }

    /// Set the old_value_text field (optional)
    pub fn old_value_text(mut self, value: String) -> Self {
        self.old_value_text = Some(value);
        self
    }

    /// Set the old_value_datetime field (optional)
    pub fn old_value_datetime(mut self, value: DateTime<Utc>) -> Self {
        self.old_value_datetime = Some(value);
        self
    }

    /// Set the new_value_integer field (optional)
    pub fn new_value_integer(mut self, value: i32) -> Self {
        self.new_value_integer = Some(value);
        self
    }

    /// Set the new_value_float field (optional)
    pub fn new_value_float(mut self, value: f64) -> Self {
        self.new_value_float = Some(value);
        self
    }

    /// Set the new_value_text field (optional)
    pub fn new_value_text(mut self, value: String) -> Self {
        self.new_value_text = Some(value);
        self
    }

    /// Set the new_value_datetime field (optional)
    pub fn new_value_datetime(mut self, value: DateTime<Utc>) -> Self {
        self.new_value_datetime = Some(value);
        self
    }

    /// Set the currency_id field (optional)
    pub fn currency_id(mut self, value: Uuid) -> Self {
        self.currency_id = Some(value);
        self
    }

    /// Build the MailTrackingValue entity
    ///
    /// Returns Err if any required field without a default is missing.
    pub fn build(self) -> Result<MailTrackingValue, String> {
        let mail_message_id = self.mail_message_id.ok_or_else(|| "mail_message_id is required".to_string())?;
        let field = self.field.ok_or_else(|| "field is required".to_string())?;

        Ok(MailTrackingValue {
            id: Uuid::new_v4(),
            mail_message_id,
            field,
            field_desc: self.field_desc,
            field_type: self.field_type,
            old_value_integer: self.old_value_integer,
            old_value_float: self.old_value_float,
            old_value_text: self.old_value_text,
            old_value_datetime: self.old_value_datetime,
            new_value_integer: self.new_value_integer,
            new_value_float: self.new_value_float,
            new_value_text: self.new_value_text,
            new_value_datetime: self.new_value_datetime,
            currency_id: self.currency_id,
            metadata: AuditMetadata::default(),
        })
    }
}
