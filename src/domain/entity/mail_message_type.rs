use serde::{Deserialize, Serialize};
use sqlx::Type;
use std::str::FromStr;
#[cfg(feature = "openapi")]
use utoipa::ToSchema;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Type)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
#[serde(rename_all = "snake_case")]
#[sqlx(type_name = "mail_message_type", rename_all = "snake_case")]
pub enum MailMessageType {
    Email,
    Comment,
    Notification,
    Sms,
}

impl std::fmt::Display for MailMessageType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Email => write!(f, "email"),
            Self::Comment => write!(f, "comment"),
            Self::Notification => write!(f, "notification"),
            Self::Sms => write!(f, "sms"),
        }
    }
}

impl FromStr for MailMessageType {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "email" => Ok(Self::Email),
            "comment" => Ok(Self::Comment),
            "notification" => Ok(Self::Notification),
            "sms" => Ok(Self::Sms),
            _ => Err(format!("Unknown MailMessageType variant: {}", s)),
        }
    }
}

impl Default for MailMessageType {
    fn default() -> Self {
        Self::Comment
    }
}
