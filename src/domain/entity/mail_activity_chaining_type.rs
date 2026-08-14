use serde::{Deserialize, Serialize};
use sqlx::Type;
use std::str::FromStr;
#[cfg(feature = "openapi")]
use utoipa::ToSchema;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Type)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
#[serde(rename_all = "snake_case")]
#[sqlx(type_name = "mail_activity_chaining_type", rename_all = "snake_case")]
pub enum MailActivityChainingType {
    Suggest,
    Trigger,
}

impl std::fmt::Display for MailActivityChainingType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Suggest => write!(f, "suggest"),
            Self::Trigger => write!(f, "trigger"),
        }
    }
}

impl FromStr for MailActivityChainingType {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "suggest" => Ok(Self::Suggest),
            "trigger" => Ok(Self::Trigger),
            _ => Err(format!("Unknown MailActivityChainingType variant: {}", s)),
        }
    }
}

impl Default for MailActivityChainingType {
    fn default() -> Self {
        Self::Suggest
    }
}
