use serde::{Deserialize, Serialize};
use sqlx::Type;
use std::str::FromStr;
#[cfg(feature = "openapi")]
use utoipa::ToSchema;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Type)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
#[serde(rename_all = "snake_case")]
#[sqlx(type_name = "mail_activity_category", rename_all = "snake_case")]
pub enum MailActivityCategory {
    UploadInvoice,
    Phonecall,
    Meeting,
}

impl std::fmt::Display for MailActivityCategory {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UploadInvoice => write!(f, "upload_invoice"),
            Self::Phonecall => write!(f, "phonecall"),
            Self::Meeting => write!(f, "meeting"),
        }
    }
}

impl FromStr for MailActivityCategory {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "upload_invoice" => Ok(Self::UploadInvoice),
            "phonecall" => Ok(Self::Phonecall),
            "meeting" => Ok(Self::Meeting),
            _ => Err(format!("Unknown MailActivityCategory variant: {}", s)),
        }
    }
}

impl Default for MailActivityCategory {
    fn default() -> Self {
        Self::Meeting
    }
}
