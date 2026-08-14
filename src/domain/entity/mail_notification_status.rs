use serde::{Deserialize, Serialize};
use sqlx::Type;
use std::str::FromStr;
#[cfg(feature = "openapi")]
use utoipa::ToSchema;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Type)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
#[serde(rename_all = "snake_case")]
#[sqlx(type_name = "mail_notification_status", rename_all = "snake_case")]
pub enum MailNotificationStatus {
    Ready,
    Process,
    Pending,
    Sent,
    Bounce,
    Exception,
    Canceled,
}

impl std::fmt::Display for MailNotificationStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Ready => write!(f, "ready"),
            Self::Process => write!(f, "process"),
            Self::Pending => write!(f, "pending"),
            Self::Sent => write!(f, "sent"),
            Self::Bounce => write!(f, "bounce"),
            Self::Exception => write!(f, "exception"),
            Self::Canceled => write!(f, "canceled"),
        }
    }
}

impl FromStr for MailNotificationStatus {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "ready" => Ok(Self::Ready),
            "process" => Ok(Self::Process),
            "pending" => Ok(Self::Pending),
            "sent" => Ok(Self::Sent),
            "bounce" => Ok(Self::Bounce),
            "exception" => Ok(Self::Exception),
            "canceled" => Ok(Self::Canceled),
            _ => Err(format!("Unknown MailNotificationStatus variant: {}", s)),
        }
    }
}

impl Default for MailNotificationStatus {
    fn default() -> Self {
        Self::Ready
    }
}
