use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use uuid::Uuid;

use super::MailMessageType;
use super::MailModerationStatus;
use super::AuditMetadata;

/// Strongly-typed ID for MailMessage
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct MailMessageId(pub Uuid);

impl MailMessageId {
    pub fn new(id: Uuid) -> Self { Self(id) }
    pub fn generate() -> Self { Self(Uuid::new_v4()) }
    pub fn into_inner(self) -> Uuid { self.0 }
}

impl std::fmt::Display for MailMessageId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::str::FromStr for MailMessageId {
    type Err = uuid::Error;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(Self(Uuid::parse_str(s)?))
    }
}

impl From<Uuid> for MailMessageId {
    fn from(id: Uuid) -> Self { Self(id) }
}

impl From<MailMessageId> for Uuid {
    fn from(id: MailMessageId) -> Self { id.0 }
}

impl AsRef<Uuid> for MailMessageId {
    fn as_ref(&self) -> &Uuid { &self.0 }
}

impl std::ops::Deref for MailMessageId {
    type Target = Uuid;
    fn deref(&self) -> &Self::Target { &self.0 }
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct MailMessage {
    pub id: Uuid,
    pub subject: Option<String>,
    pub date: DateTime<Utc>,
    pub body: String,
    pub message_type: MailMessageType,
    pub subtype_id: Option<Uuid>,
    pub is_internal: bool,
    pub author_id: Option<Uuid>,
    pub author_guest_id: Option<Uuid>,
    pub email_from: Option<String>,
    pub message_id: Option<String>,
    pub reply_to: Option<String>,
    pub model: Option<String>,
    pub res_id: Option<Uuid>,
    pub record_name: Option<String>,
    pub moderation_status: Option<MailModerationStatus>,
    pub needaction: bool,
    pub has_error: bool,
    pub failure_reason: Option<String>,
    pub rating_value: Option<f64>,
    #[serde(default)]
    #[sqlx(json)]
    pub metadata: AuditMetadata,
}

impl MailMessage {
    /// Create a builder for MailMessage
    pub fn builder() -> MailMessageBuilder {
        <MailMessageBuilder as Default>::default()
    }

    /// Create a new MailMessage with required fields
    pub fn new(date: DateTime<Utc>, body: String, message_type: MailMessageType, is_internal: bool, needaction: bool, has_error: bool) -> Self {
        Self {
            id: Uuid::new_v4(),
            subject: None,
            date,
            body,
            message_type,
            subtype_id: None,
            is_internal,
            author_id: None,
            author_guest_id: None,
            email_from: None,
            message_id: None,
            reply_to: None,
            model: None,
            res_id: None,
            record_name: None,
            moderation_status: None,
            needaction,
            has_error,
            failure_reason: None,
            rating_value: None,
            metadata: AuditMetadata::default(),
        }
    }

    /// Get the entity's unique identifier
    pub fn id(&self) -> &Uuid {
        &self.id
    }

    /// Get a strongly-typed ID for this entity
    pub fn typed_id(&self) -> MailMessageId {
        MailMessageId(self.id)
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

    /// Set the subject field (chainable)
    pub fn with_subject(mut self, value: String) -> Self {
        self.subject = Some(value);
        self
    }

    /// Set the subtype_id field (chainable)
    pub fn with_subtype_id(mut self, value: Uuid) -> Self {
        self.subtype_id = Some(value);
        self
    }

    /// Set the author_id field (chainable)
    pub fn with_author_id(mut self, value: Uuid) -> Self {
        self.author_id = Some(value);
        self
    }

    /// Set the author_guest_id field (chainable)
    pub fn with_author_guest_id(mut self, value: Uuid) -> Self {
        self.author_guest_id = Some(value);
        self
    }

    /// Set the email_from field (chainable)
    pub fn with_email_from(mut self, value: String) -> Self {
        self.email_from = Some(value);
        self
    }

    /// Set the message_id field (chainable)
    pub fn with_message_id(mut self, value: String) -> Self {
        self.message_id = Some(value);
        self
    }

    /// Set the reply_to field (chainable)
    pub fn with_reply_to(mut self, value: String) -> Self {
        self.reply_to = Some(value);
        self
    }

    /// Set the model field (chainable)
    pub fn with_model(mut self, value: String) -> Self {
        self.model = Some(value);
        self
    }

    /// Set the res_id field (chainable)
    pub fn with_res_id(mut self, value: Uuid) -> Self {
        self.res_id = Some(value);
        self
    }

    /// Set the record_name field (chainable)
    pub fn with_record_name(mut self, value: String) -> Self {
        self.record_name = Some(value);
        self
    }

    /// Set the moderation_status field (chainable)
    pub fn with_moderation_status(mut self, value: MailModerationStatus) -> Self {
        self.moderation_status = Some(value);
        self
    }

    /// Set the failure_reason field (chainable)
    pub fn with_failure_reason(mut self, value: String) -> Self {
        self.failure_reason = Some(value);
        self
    }

    /// Set the rating_value field (chainable)
    pub fn with_rating_value(mut self, value: f64) -> Self {
        self.rating_value = Some(value);
        self
    }

    // ==========================================================
    // Partial Update
    // ==========================================================

    /// Apply partial updates from a map of field name to JSON value
    pub fn apply_patch(&mut self, fields: std::collections::HashMap<String, serde_json::Value>) {
        for (key, value) in fields {
            match key.as_str() {
                "subject" => {
                    if let Ok(v) = serde_json::from_value(value) { self.subject = v; }
                }
                "date" => {
                    if let Ok(v) = serde_json::from_value(value) { self.date = v; }
                }
                "body" => {
                    if let Ok(v) = serde_json::from_value(value) { self.body = v; }
                }
                "message_type" => {
                    if let Ok(v) = serde_json::from_value(value) { self.message_type = v; }
                }
                "subtype_id" => {
                    if let Ok(v) = serde_json::from_value(value) { self.subtype_id = v; }
                }
                "is_internal" => {
                    if let Ok(v) = serde_json::from_value(value) { self.is_internal = v; }
                }
                "author_id" => {
                    if let Ok(v) = serde_json::from_value(value) { self.author_id = v; }
                }
                "author_guest_id" => {
                    if let Ok(v) = serde_json::from_value(value) { self.author_guest_id = v; }
                }
                "email_from" => {
                    if let Ok(v) = serde_json::from_value(value) { self.email_from = v; }
                }
                "message_id" => {
                    if let Ok(v) = serde_json::from_value(value) { self.message_id = v; }
                }
                "reply_to" => {
                    if let Ok(v) = serde_json::from_value(value) { self.reply_to = v; }
                }
                "model" => {
                    if let Ok(v) = serde_json::from_value(value) { self.model = v; }
                }
                "res_id" => {
                    if let Ok(v) = serde_json::from_value(value) { self.res_id = v; }
                }
                "record_name" => {
                    if let Ok(v) = serde_json::from_value(value) { self.record_name = v; }
                }
                "moderation_status" => {
                    if let Ok(v) = serde_json::from_value(value) { self.moderation_status = v; }
                }
                "needaction" => {
                    if let Ok(v) = serde_json::from_value(value) { self.needaction = v; }
                }
                "has_error" => {
                    if let Ok(v) = serde_json::from_value(value) { self.has_error = v; }
                }
                "failure_reason" => {
                    if let Ok(v) = serde_json::from_value(value) { self.failure_reason = v; }
                }
                "rating_value" => {
                    if let Ok(v) = serde_json::from_value(value) { self.rating_value = v; }
                }
                _ => {} // ignore unknown fields
            }
        }
    }

    // <<< CUSTOM METHODS START >>>
    // <<< CUSTOM METHODS END >>>
}

impl super::Entity for MailMessage {
    type Id = Uuid;

    fn entity_id(&self) -> &Self::Id {
        &self.id
    }

    fn entity_type() -> &'static str {
        "MailMessage"
    }
}

impl backbone_core::PersistentEntity for MailMessage {
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

impl backbone_orm::EntityRepoMeta for MailMessage {
    fn column_types() -> std::collections::HashMap<String, String> {
        let mut m = std::collections::HashMap::new();
        m.insert("id".to_string(), "uuid".to_string());
        m.insert("subtype_id".to_string(), "uuid".to_string());
        m.insert("author_id".to_string(), "uuid".to_string());
        m.insert("author_guest_id".to_string(), "uuid".to_string());
        m.insert("res_id".to_string(), "uuid".to_string());
        m.insert("message_type".to_string(), "mail_message_type".to_string());
        m.insert("moderation_status".to_string(), "mail_moderation_status".to_string());
        m
    }
    fn search_fields() -> &'static [&'static str] {
        &["body"]
    }
}

/// Builder for MailMessage entity
///
/// Provides a fluent API for constructing MailMessage instances.
/// System fields (id, metadata, timestamps) are auto-initialized.
#[derive(Debug, Clone, Default)]
pub struct MailMessageBuilder {
    subject: Option<String>,
    date: Option<DateTime<Utc>>,
    body: Option<String>,
    message_type: Option<MailMessageType>,
    subtype_id: Option<Uuid>,
    is_internal: Option<bool>,
    author_id: Option<Uuid>,
    author_guest_id: Option<Uuid>,
    email_from: Option<String>,
    message_id: Option<String>,
    reply_to: Option<String>,
    model: Option<String>,
    res_id: Option<Uuid>,
    record_name: Option<String>,
    moderation_status: Option<MailModerationStatus>,
    needaction: Option<bool>,
    has_error: Option<bool>,
    failure_reason: Option<String>,
    rating_value: Option<f64>,
}

impl MailMessageBuilder {
    /// Set the subject field (optional)
    pub fn subject(mut self, value: String) -> Self {
        self.subject = Some(value);
        self
    }

    /// Set the date field (default: `Utc::now()`)
    pub fn date(mut self, value: DateTime<Utc>) -> Self {
        self.date = Some(value);
        self
    }

    /// Set the body field (required)
    pub fn body(mut self, value: String) -> Self {
        self.body = Some(value);
        self
    }

    /// Set the message_type field (default: `MailMessageType::default()`)
    pub fn message_type(mut self, value: MailMessageType) -> Self {
        self.message_type = Some(value);
        self
    }

    /// Set the subtype_id field (optional)
    pub fn subtype_id(mut self, value: Uuid) -> Self {
        self.subtype_id = Some(value);
        self
    }

    /// Set the is_internal field (default: `false`)
    pub fn is_internal(mut self, value: bool) -> Self {
        self.is_internal = Some(value);
        self
    }

    /// Set the author_id field (optional)
    pub fn author_id(mut self, value: Uuid) -> Self {
        self.author_id = Some(value);
        self
    }

    /// Set the author_guest_id field (optional)
    pub fn author_guest_id(mut self, value: Uuid) -> Self {
        self.author_guest_id = Some(value);
        self
    }

    /// Set the email_from field (optional)
    pub fn email_from(mut self, value: String) -> Self {
        self.email_from = Some(value);
        self
    }

    /// Set the message_id field (optional)
    pub fn message_id(mut self, value: String) -> Self {
        self.message_id = Some(value);
        self
    }

    /// Set the reply_to field (optional)
    pub fn reply_to(mut self, value: String) -> Self {
        self.reply_to = Some(value);
        self
    }

    /// Set the model field (optional)
    pub fn model(mut self, value: String) -> Self {
        self.model = Some(value);
        self
    }

    /// Set the res_id field (optional)
    pub fn res_id(mut self, value: Uuid) -> Self {
        self.res_id = Some(value);
        self
    }

    /// Set the record_name field (optional)
    pub fn record_name(mut self, value: String) -> Self {
        self.record_name = Some(value);
        self
    }

    /// Set the moderation_status field (optional)
    pub fn moderation_status(mut self, value: MailModerationStatus) -> Self {
        self.moderation_status = Some(value);
        self
    }

    /// Set the needaction field (default: `false`)
    pub fn needaction(mut self, value: bool) -> Self {
        self.needaction = Some(value);
        self
    }

    /// Set the has_error field (default: `false`)
    pub fn has_error(mut self, value: bool) -> Self {
        self.has_error = Some(value);
        self
    }

    /// Set the failure_reason field (optional)
    pub fn failure_reason(mut self, value: String) -> Self {
        self.failure_reason = Some(value);
        self
    }

    /// Set the rating_value field (optional)
    pub fn rating_value(mut self, value: f64) -> Self {
        self.rating_value = Some(value);
        self
    }

    /// Build the MailMessage entity
    ///
    /// Returns Err if any required field without a default is missing.
    pub fn build(self) -> Result<MailMessage, String> {
        let body = self.body.ok_or_else(|| "body is required".to_string())?;

        Ok(MailMessage {
            id: Uuid::new_v4(),
            subject: self.subject,
            date: self.date.unwrap_or(Utc::now()),
            body,
            message_type: self.message_type.unwrap_or_default(),
            subtype_id: self.subtype_id,
            is_internal: self.is_internal.unwrap_or(false),
            author_id: self.author_id,
            author_guest_id: self.author_guest_id,
            email_from: self.email_from,
            message_id: self.message_id,
            reply_to: self.reply_to,
            model: self.model,
            res_id: self.res_id,
            record_name: self.record_name,
            moderation_status: self.moderation_status,
            needaction: self.needaction.unwrap_or(false),
            has_error: self.has_error.unwrap_or(false),
            failure_reason: self.failure_reason,
            rating_value: self.rating_value,
            metadata: AuditMetadata::default(),
        })
    }
}
