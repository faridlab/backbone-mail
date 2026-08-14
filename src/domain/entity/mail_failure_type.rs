use serde::{Deserialize, Serialize};
use sqlx::Type;
use std::str::FromStr;
#[cfg(feature = "openapi")]
use utoipa::ToSchema;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Type)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
#[serde(rename_all = "snake_case")]
#[sqlx(type_name = "mail_failure_type", rename_all = "snake_case")]
pub enum MailFailureType {
    MailSmtp,
    MailEmailInvalid,
    MailBounce,
    MailBlacklist,
    MailRecipient,
    MailServer,
    Unknown,
}

impl std::fmt::Display for MailFailureType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::MailSmtp => write!(f, "mail_smtp"),
            Self::MailEmailInvalid => write!(f, "mail_email_invalid"),
            Self::MailBounce => write!(f, "mail_bounce"),
            Self::MailBlacklist => write!(f, "mail_blacklist"),
            Self::MailRecipient => write!(f, "mail_recipient"),
            Self::MailServer => write!(f, "mail_server"),
            Self::Unknown => write!(f, "unknown"),
        }
    }
}

impl FromStr for MailFailureType {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "mail_smtp" => Ok(Self::MailSmtp),
            "mail_email_invalid" => Ok(Self::MailEmailInvalid),
            "mail_bounce" => Ok(Self::MailBounce),
            "mail_blacklist" => Ok(Self::MailBlacklist),
            "mail_recipient" => Ok(Self::MailRecipient),
            "mail_server" => Ok(Self::MailServer),
            "unknown" => Ok(Self::Unknown),
            _ => Err(format!("Unknown MailFailureType variant: {}", s)),
        }
    }
}

impl Default for MailFailureType {
    fn default() -> Self {
        Self::Unknown
    }
}
