use serde::{Deserialize, Serialize};
use sqlx::Type;
use std::str::FromStr;
#[cfg(feature = "openapi")]
use utoipa::ToSchema;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Type)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
#[serde(rename_all = "snake_case")]
#[sqlx(type_name = "mail_moderation_status", rename_all = "snake_case")]
pub enum MailModerationStatus {
    Pending,
    Accepted,
    RejectedModeration,
}

impl std::fmt::Display for MailModerationStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Pending => write!(f, "pending"),
            Self::Accepted => write!(f, "accepted"),
            Self::RejectedModeration => write!(f, "rejected_moderation"),
        }
    }
}

impl FromStr for MailModerationStatus {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "pending" => Ok(Self::Pending),
            "accepted" => Ok(Self::Accepted),
            "rejected_moderation" => Ok(Self::RejectedModeration),
            _ => Err(format!("Unknown MailModerationStatus variant: {}", s)),
        }
    }
}

impl Default for MailModerationStatus {
    fn default() -> Self {
        Self::Pending
    }
}
