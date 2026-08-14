use serde::{Deserialize, Serialize};
use sqlx::Type;
use std::str::FromStr;
#[cfg(feature = "openapi")]
use utoipa::ToSchema;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Type)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
#[serde(rename_all = "snake_case")]
#[sqlx(type_name = "mail_alias_contact", rename_all = "snake_case")]
pub enum MailAliasContact {
    Everyone,
    Partners,
    Followers,
}

impl std::fmt::Display for MailAliasContact {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Everyone => write!(f, "everyone"),
            Self::Partners => write!(f, "partners"),
            Self::Followers => write!(f, "followers"),
        }
    }
}

impl FromStr for MailAliasContact {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "everyone" => Ok(Self::Everyone),
            "partners" => Ok(Self::Partners),
            "followers" => Ok(Self::Followers),
            _ => Err(format!("Unknown MailAliasContact variant: {}", s)),
        }
    }
}

impl Default for MailAliasContact {
    fn default() -> Self {
        Self::Followers
    }
}
