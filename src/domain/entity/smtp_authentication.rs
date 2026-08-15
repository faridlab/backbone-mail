use serde::{Deserialize, Serialize};
use sqlx::Type;
use std::str::FromStr;
#[cfg(feature = "openapi")]
use utoipa::ToSchema;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Type)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
#[serde(rename_all = "snake_case")]
#[sqlx(type_name = "smtp_authentication", rename_all = "snake_case")]
pub enum SmtpAuthentication {
    Login,
    Certificate,
}

impl std::fmt::Display for SmtpAuthentication {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Login => write!(f, "login"),
            Self::Certificate => write!(f, "certificate"),
        }
    }
}

impl FromStr for SmtpAuthentication {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "login" => Ok(Self::Login),
            "certificate" => Ok(Self::Certificate),
            _ => Err(format!("Unknown SmtpAuthentication variant: {}", s)),
        }
    }
}

impl Default for SmtpAuthentication {
    fn default() -> Self {
        Self::Login
    }
}
