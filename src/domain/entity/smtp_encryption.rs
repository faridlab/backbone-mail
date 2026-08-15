use serde::{Deserialize, Serialize};
use sqlx::Type;
use std::str::FromStr;
#[cfg(feature = "openapi")]
use utoipa::ToSchema;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Type)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
#[serde(rename_all = "snake_case")]
#[sqlx(type_name = "smtp_encryption", rename_all = "snake_case")]
pub enum SmtpEncryption {
    None,
    Starttls,
    Ssl,
}

impl std::fmt::Display for SmtpEncryption {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::None => write!(f, "none"),
            Self::Starttls => write!(f, "starttls"),
            Self::Ssl => write!(f, "ssl"),
        }
    }
}

impl FromStr for SmtpEncryption {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "none" => Ok(Self::None),
            "starttls" => Ok(Self::Starttls),
            "ssl" => Ok(Self::Ssl),
            _ => Err(format!("Unknown SmtpEncryption variant: {}", s)),
        }
    }
}

impl Default for SmtpEncryption {
    fn default() -> Self {
        Self::None
    }
}
