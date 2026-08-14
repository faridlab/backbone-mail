use serde::{Deserialize, Serialize};
use sqlx::Type;
use std::str::FromStr;
#[cfg(feature = "openapi")]
use utoipa::ToSchema;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Type)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
#[serde(rename_all = "snake_case")]
#[sqlx(type_name = "mail_state", rename_all = "snake_case")]
pub enum MailState {
    Outgoing,
    Sent,
    Received,
    Exception,
    Cancel,
}

impl std::fmt::Display for MailState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Outgoing => write!(f, "outgoing"),
            Self::Sent => write!(f, "sent"),
            Self::Received => write!(f, "received"),
            Self::Exception => write!(f, "exception"),
            Self::Cancel => write!(f, "cancel"),
        }
    }
}

impl FromStr for MailState {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "outgoing" => Ok(Self::Outgoing),
            "sent" => Ok(Self::Sent),
            "received" => Ok(Self::Received),
            "exception" => Ok(Self::Exception),
            "cancel" => Ok(Self::Cancel),
            _ => Err(format!("Unknown MailState variant: {}", s)),
        }
    }
}

impl Default for MailState {
    fn default() -> Self {
        Self::Outgoing
    }
}
