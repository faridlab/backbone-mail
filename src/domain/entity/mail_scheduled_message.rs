use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use uuid::Uuid;
use super::AuditMetadata;

/// Strongly-typed ID for MailScheduledMessage
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct MailScheduledMessageId(pub Uuid);

impl MailScheduledMessageId {
    pub fn new(id: Uuid) -> Self { Self(id) }
    pub fn generate() -> Self { Self(Uuid::new_v4()) }
    pub fn into_inner(self) -> Uuid { self.0 }
}

impl std::fmt::Display for MailScheduledMessageId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::str::FromStr for MailScheduledMessageId {
    type Err = uuid::Error;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(Self(Uuid::parse_str(s)?))
    }
}

impl From<Uuid> for MailScheduledMessageId {
    fn from(id: Uuid) -> Self { Self(id) }
}

impl From<MailScheduledMessageId> for Uuid {
    fn from(id: MailScheduledMessageId) -> Self { id.0 }
}

impl AsRef<Uuid> for MailScheduledMessageId {
    fn as_ref(&self) -> &Uuid { &self.0 }
}

impl std::ops::Deref for MailScheduledMessageId {
    type Target = Uuid;
    fn deref(&self) -> &Self::Target { &self.0 }
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct MailScheduledMessage {
    pub id: Uuid,
    pub subject: Option<String>,
    pub body: String,
    pub scheduled_date: DateTime<Utc>,
    pub composition_comment_option: Option<String>,
    pub model: String,
    pub res_id: Uuid,
    pub author_party_id: Uuid,
    pub recipient_party_ids: Option<serde_json::Value>,
    pub is_note: bool,
    pub notification_parameters: Option<String>,
    pub send_context: Option<serde_json::Value>,
    #[serde(default)]
    #[sqlx(json)]
    pub metadata: AuditMetadata,
}

impl MailScheduledMessage {
    /// Create a builder for MailScheduledMessage
    pub fn builder() -> MailScheduledMessageBuilder {
        <MailScheduledMessageBuilder as Default>::default()
    }

    /// Create a new MailScheduledMessage with required fields
    pub fn new(body: String, scheduled_date: DateTime<Utc>, model: String, res_id: Uuid, author_party_id: Uuid, is_note: bool) -> Self {
        Self {
            id: Uuid::new_v4(),
            subject: None,
            body,
            scheduled_date,
            composition_comment_option: None,
            model,
            res_id,
            author_party_id,
            recipient_party_ids: None,
            is_note,
            notification_parameters: None,
            send_context: None,
            metadata: AuditMetadata::default(),
        }
    }

    /// Get the entity's unique identifier
    pub fn id(&self) -> &Uuid {
        &self.id
    }

    /// Get a strongly-typed ID for this entity
    pub fn typed_id(&self) -> MailScheduledMessageId {
        MailScheduledMessageId(self.id)
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

    /// Set the composition_comment_option field (chainable)
    pub fn with_composition_comment_option(mut self, value: String) -> Self {
        self.composition_comment_option = Some(value);
        self
    }

    /// Set the recipient_party_ids field (chainable)
    pub fn with_recipient_party_ids(mut self, value: serde_json::Value) -> Self {
        self.recipient_party_ids = Some(value);
        self
    }

    /// Set the notification_parameters field (chainable)
    pub fn with_notification_parameters(mut self, value: String) -> Self {
        self.notification_parameters = Some(value);
        self
    }

    /// Set the send_context field (chainable)
    pub fn with_send_context(mut self, value: serde_json::Value) -> Self {
        self.send_context = Some(value);
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
                "body" => {
                    if let Ok(v) = serde_json::from_value(value) { self.body = v; }
                }
                "scheduled_date" => {
                    if let Ok(v) = serde_json::from_value(value) { self.scheduled_date = v; }
                }
                "composition_comment_option" => {
                    if let Ok(v) = serde_json::from_value(value) { self.composition_comment_option = v; }
                }
                "model" => {
                    if let Ok(v) = serde_json::from_value(value) { self.model = v; }
                }
                "res_id" => {
                    if let Ok(v) = serde_json::from_value(value) { self.res_id = v; }
                }
                "author_party_id" => {
                    if let Ok(v) = serde_json::from_value(value) { self.author_party_id = v; }
                }
                "recipient_party_ids" => {
                    if let Ok(v) = serde_json::from_value(value) { self.recipient_party_ids = v; }
                }
                "is_note" => {
                    if let Ok(v) = serde_json::from_value(value) { self.is_note = v; }
                }
                "notification_parameters" => {
                    if let Ok(v) = serde_json::from_value(value) { self.notification_parameters = v; }
                }
                "send_context" => {
                    if let Ok(v) = serde_json::from_value(value) { self.send_context = v; }
                }
                _ => {} // ignore unknown fields
            }
        }
    }

    // <<< CUSTOM METHODS START >>>
    // <<< CUSTOM METHODS END >>>
}

impl super::Entity for MailScheduledMessage {
    type Id = Uuid;

    fn entity_id(&self) -> &Self::Id {
        &self.id
    }

    fn entity_type() -> &'static str {
        "MailScheduledMessage"
    }
}

impl backbone_core::PersistentEntity for MailScheduledMessage {
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

impl backbone_orm::EntityRepoMeta for MailScheduledMessage {
    fn column_types() -> std::collections::HashMap<String, String> {
        let mut m = std::collections::HashMap::new();
        m.insert("id".to_string(), "uuid".to_string());
        m.insert("res_id".to_string(), "uuid".to_string());
        m.insert("author_party_id".to_string(), "uuid".to_string());
        m
    }
    fn search_fields() -> &'static [&'static str] {
        &["body", "model"]
    }
}

/// Builder for MailScheduledMessage entity
///
/// Provides a fluent API for constructing MailScheduledMessage instances.
/// System fields (id, metadata, timestamps) are auto-initialized.
#[derive(Debug, Clone, Default)]
pub struct MailScheduledMessageBuilder {
    subject: Option<String>,
    body: Option<String>,
    scheduled_date: Option<DateTime<Utc>>,
    composition_comment_option: Option<String>,
    model: Option<String>,
    res_id: Option<Uuid>,
    author_party_id: Option<Uuid>,
    recipient_party_ids: Option<serde_json::Value>,
    is_note: Option<bool>,
    notification_parameters: Option<String>,
    send_context: Option<serde_json::Value>,
}

impl MailScheduledMessageBuilder {
    /// Set the subject field (optional)
    pub fn subject(mut self, value: String) -> Self {
        self.subject = Some(value);
        self
    }

    /// Set the body field (required)
    pub fn body(mut self, value: String) -> Self {
        self.body = Some(value);
        self
    }

    /// Set the scheduled_date field (required)
    pub fn scheduled_date(mut self, value: DateTime<Utc>) -> Self {
        self.scheduled_date = Some(value);
        self
    }

    /// Set the composition_comment_option field (optional)
    pub fn composition_comment_option(mut self, value: String) -> Self {
        self.composition_comment_option = Some(value);
        self
    }

    /// Set the model field (required)
    pub fn model(mut self, value: String) -> Self {
        self.model = Some(value);
        self
    }

    /// Set the res_id field (required)
    pub fn res_id(mut self, value: Uuid) -> Self {
        self.res_id = Some(value);
        self
    }

    /// Set the author_party_id field (required)
    pub fn author_party_id(mut self, value: Uuid) -> Self {
        self.author_party_id = Some(value);
        self
    }

    /// Set the recipient_party_ids field (optional)
    pub fn recipient_party_ids(mut self, value: serde_json::Value) -> Self {
        self.recipient_party_ids = Some(value);
        self
    }

    /// Set the is_note field (default: `false`)
    pub fn is_note(mut self, value: bool) -> Self {
        self.is_note = Some(value);
        self
    }

    /// Set the notification_parameters field (optional)
    pub fn notification_parameters(mut self, value: String) -> Self {
        self.notification_parameters = Some(value);
        self
    }

    /// Set the send_context field (optional)
    pub fn send_context(mut self, value: serde_json::Value) -> Self {
        self.send_context = Some(value);
        self
    }

    /// Build the MailScheduledMessage entity
    ///
    /// Returns Err if any required field without a default is missing.
    pub fn build(self) -> Result<MailScheduledMessage, String> {
        let body = self.body.ok_or_else(|| "body is required".to_string())?;
        let scheduled_date = self.scheduled_date.ok_or_else(|| "scheduled_date is required".to_string())?;
        let model = self.model.ok_or_else(|| "model is required".to_string())?;
        let res_id = self.res_id.ok_or_else(|| "res_id is required".to_string())?;
        let author_party_id = self.author_party_id.ok_or_else(|| "author_party_id is required".to_string())?;

        Ok(MailScheduledMessage {
            id: Uuid::new_v4(),
            subject: self.subject,
            body,
            scheduled_date,
            composition_comment_option: self.composition_comment_option,
            model,
            res_id,
            author_party_id,
            recipient_party_ids: self.recipient_party_ids,
            is_note: self.is_note.unwrap_or(false),
            notification_parameters: self.notification_parameters,
            send_context: self.send_context,
            metadata: AuditMetadata::default(),
        })
    }
}
