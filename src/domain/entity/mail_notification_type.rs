use serde::{Deserialize, Serialize};
use sqlx::Type;
use std::str::FromStr;
#[cfg(feature = "openapi")]
use utoipa::ToSchema;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Type)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
#[serde(rename_all = "snake_case")]
#[sqlx(type_name = "mail_notification_type", rename_all = "snake_case")]
pub enum MailNotificationType {
    Inbox,
    Email,
    Sms,
}

impl std::fmt::Display for MailNotificationType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Inbox => write!(f, "inbox"),
            Self::Email => write!(f, "email"),
            Self::Sms => write!(f, "sms"),
        }
    }
}

impl FromStr for MailNotificationType {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "inbox" => Ok(Self::Inbox),
            "email" => Ok(Self::Email),
            "sms" => Ok(Self::Sms),
            _ => Err(format!("Unknown MailNotificationType variant: {}", s)),
        }
    }
}

impl Default for MailNotificationType {
    fn default() -> Self {
        Self::Email
    }
}
