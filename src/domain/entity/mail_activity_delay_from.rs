use serde::{Deserialize, Serialize};
use sqlx::Type;
use std::str::FromStr;
#[cfg(feature = "openapi")]
use utoipa::ToSchema;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Type)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
#[serde(rename_all = "snake_case")]
#[sqlx(type_name = "mail_activity_delay_from", rename_all = "snake_case")]
pub enum MailActivityDelayFrom {
    PlanDate,
    PreviousActivity,
}

impl std::fmt::Display for MailActivityDelayFrom {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::PlanDate => write!(f, "plan_date"),
            Self::PreviousActivity => write!(f, "previous_activity"),
        }
    }
}

impl FromStr for MailActivityDelayFrom {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "plan_date" => Ok(Self::PlanDate),
            "previous_activity" => Ok(Self::PreviousActivity),
            _ => Err(format!("Unknown MailActivityDelayFrom variant: {}", s)),
        }
    }
}

impl Default for MailActivityDelayFrom {
    fn default() -> Self {
        Self::PreviousActivity
    }
}
