use serde::{Deserialize, Serialize};
use sqlx::Type;
use std::str::FromStr;
#[cfg(feature = "openapi")]
use utoipa::ToSchema;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Type)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
#[serde(rename_all = "snake_case")]
#[sqlx(type_name = "sms_state", rename_all = "snake_case")]
pub enum SmsState {
    Outgoing,
    Process,
    Pending,
    Sent,
    Error,
    Canceled,
}

impl std::fmt::Display for SmsState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Outgoing => write!(f, "outgoing"),
            Self::Process => write!(f, "process"),
            Self::Pending => write!(f, "pending"),
            Self::Sent => write!(f, "sent"),
            Self::Error => write!(f, "error"),
            Self::Canceled => write!(f, "canceled"),
        }
    }
}

impl FromStr for SmsState {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "outgoing" => Ok(Self::Outgoing),
            "process" => Ok(Self::Process),
            "pending" => Ok(Self::Pending),
            "sent" => Ok(Self::Sent),
            "error" => Ok(Self::Error),
            "canceled" => Ok(Self::Canceled),
            _ => Err(format!("Unknown SmsState variant: {}", s)),
        }
    }
}

impl Default for SmsState {
    fn default() -> Self {
        Self::Outgoing
    }
}
