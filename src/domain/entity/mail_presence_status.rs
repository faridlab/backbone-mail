use serde::{Deserialize, Serialize};
use sqlx::Type;
use std::str::FromStr;
#[cfg(feature = "openapi")]
use utoipa::ToSchema;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Type)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
#[serde(rename_all = "snake_case")]
#[sqlx(type_name = "mail_presence_status", rename_all = "snake_case")]
pub enum MailPresenceStatus {
    Online,
    Away,
    Offline,
}

impl std::fmt::Display for MailPresenceStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Online => write!(f, "online"),
            Self::Away => write!(f, "away"),
            Self::Offline => write!(f, "offline"),
        }
    }
}

impl FromStr for MailPresenceStatus {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "online" => Ok(Self::Online),
            "away" => Ok(Self::Away),
            "offline" => Ok(Self::Offline),
            _ => Err(format!("Unknown MailPresenceStatus variant: {}", s)),
        }
    }
}

impl Default for MailPresenceStatus {
    fn default() -> Self {
        Self::Offline
    }
}
