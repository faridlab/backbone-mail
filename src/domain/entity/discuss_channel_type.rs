use serde::{Deserialize, Serialize};
use sqlx::Type;
use std::str::FromStr;
#[cfg(feature = "openapi")]
use utoipa::ToSchema;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Type)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
#[serde(rename_all = "snake_case")]
#[sqlx(type_name = "discuss_channel_type", rename_all = "snake_case")]
pub enum DiscussChannelType {
    Chat,
    Channel,
    Group,
}

impl std::fmt::Display for DiscussChannelType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Chat => write!(f, "chat"),
            Self::Channel => write!(f, "channel"),
            Self::Group => write!(f, "group"),
        }
    }
}

impl FromStr for DiscussChannelType {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "chat" => Ok(Self::Chat),
            "channel" => Ok(Self::Channel),
            "group" => Ok(Self::Group),
            _ => Err(format!("Unknown DiscussChannelType variant: {}", s)),
        }
    }
}

impl Default for DiscussChannelType {
    fn default() -> Self {
        Self::Channel
    }
}
