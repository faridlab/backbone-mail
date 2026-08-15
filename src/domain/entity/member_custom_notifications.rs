use serde::{Deserialize, Serialize};
use sqlx::Type;
use std::str::FromStr;
#[cfg(feature = "openapi")]
use utoipa::ToSchema;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Type)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
#[serde(rename_all = "snake_case")]
#[sqlx(type_name = "member_custom_notifications", rename_all = "snake_case")]
pub enum MemberCustomNotifications {
    All,
    Mentions,
    NoNotif,
}

impl std::fmt::Display for MemberCustomNotifications {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::All => write!(f, "all"),
            Self::Mentions => write!(f, "mentions"),
            Self::NoNotif => write!(f, "no_notif"),
        }
    }
}

impl FromStr for MemberCustomNotifications {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "all" => Ok(Self::All),
            "mentions" => Ok(Self::Mentions),
            "no_notif" => Ok(Self::NoNotif),
            _ => Err(format!("Unknown MemberCustomNotifications variant: {}", s)),
        }
    }
}

impl Default for MemberCustomNotifications {
    fn default() -> Self {
        Self::All
    }
}
