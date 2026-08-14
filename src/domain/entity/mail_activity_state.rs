use serde::{Deserialize, Serialize};
use sqlx::Type;
use std::str::FromStr;
#[cfg(feature = "openapi")]
use utoipa::ToSchema;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Type)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
#[serde(rename_all = "snake_case")]
#[sqlx(type_name = "mail_activity_state", rename_all = "snake_case")]
pub enum MailActivityState {
    Overdue,
    Today,
    Planned,
    Done,
}

impl std::fmt::Display for MailActivityState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Overdue => write!(f, "overdue"),
            Self::Today => write!(f, "today"),
            Self::Planned => write!(f, "planned"),
            Self::Done => write!(f, "done"),
        }
    }
}

impl FromStr for MailActivityState {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "overdue" => Ok(Self::Overdue),
            "today" => Ok(Self::Today),
            "planned" => Ok(Self::Planned),
            "done" => Ok(Self::Done),
            _ => Err(format!("Unknown MailActivityState variant: {}", s)),
        }
    }
}

impl Default for MailActivityState {
    fn default() -> Self {
        Self::Today
    }
}
