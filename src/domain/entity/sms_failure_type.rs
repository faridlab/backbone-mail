use serde::{Deserialize, Serialize};
use sqlx::Type;
use std::str::FromStr;
#[cfg(feature = "openapi")]
use utoipa::ToSchema;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Type)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
#[serde(rename_all = "snake_case")]
#[sqlx(type_name = "sms_failure_type", rename_all = "snake_case")]
pub enum SmsFailureType {
    SmsNumberMissing,
    SmsNumberFormat,
    SmsCountryNotSupported,
    SmsRegistrationNeeded,
    SmsCredit,
    SmsServer,
    SmsAcc,
    SmsBlacklist,
    SmsDuplicate,
    SmsOptout,
    Unknown,
}

impl std::fmt::Display for SmsFailureType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::SmsNumberMissing => write!(f, "sms_number_missing"),
            Self::SmsNumberFormat => write!(f, "sms_number_format"),
            Self::SmsCountryNotSupported => write!(f, "sms_country_not_supported"),
            Self::SmsRegistrationNeeded => write!(f, "sms_registration_needed"),
            Self::SmsCredit => write!(f, "sms_credit"),
            Self::SmsServer => write!(f, "sms_server"),
            Self::SmsAcc => write!(f, "sms_acc"),
            Self::SmsBlacklist => write!(f, "sms_blacklist"),
            Self::SmsDuplicate => write!(f, "sms_duplicate"),
            Self::SmsOptout => write!(f, "sms_optout"),
            Self::Unknown => write!(f, "unknown"),
        }
    }
}

impl FromStr for SmsFailureType {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "sms_number_missing" => Ok(Self::SmsNumberMissing),
            "sms_number_format" => Ok(Self::SmsNumberFormat),
            "sms_country_not_supported" => Ok(Self::SmsCountryNotSupported),
            "sms_registration_needed" => Ok(Self::SmsRegistrationNeeded),
            "sms_credit" => Ok(Self::SmsCredit),
            "sms_server" => Ok(Self::SmsServer),
            "sms_acc" => Ok(Self::SmsAcc),
            "sms_blacklist" => Ok(Self::SmsBlacklist),
            "sms_duplicate" => Ok(Self::SmsDuplicate),
            "sms_optout" => Ok(Self::SmsOptout),
            "unknown" => Ok(Self::Unknown),
            _ => Err(format!("Unknown SmsFailureType variant: {}", s)),
        }
    }
}

impl Default for SmsFailureType {
    fn default() -> Self {
        Self::SmsCredit
    }
}
