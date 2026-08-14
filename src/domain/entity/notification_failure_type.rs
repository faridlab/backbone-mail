use serde::{Deserialize, Serialize};
use sqlx::Type;
use std::str::FromStr;
#[cfg(feature = "openapi")]
use utoipa::ToSchema;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Type)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
#[serde(rename_all = "snake_case")]
#[sqlx(type_name = "notification_failure_type", rename_all = "snake_case")]
pub enum NotificationFailureType {
    MailSmtp,
    MailEmailInvalid,
    MailBounce,
    MailBlacklist,
    MailRecipient,
    MailServer,
    Unknown,
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
    SmsExpired,
    SmsInvalidDestination,
    SmsNotAllowed,
    SmsNotDelivered,
    SmsRejected,
}

impl std::fmt::Display for NotificationFailureType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::MailSmtp => write!(f, "mail_smtp"),
            Self::MailEmailInvalid => write!(f, "mail_email_invalid"),
            Self::MailBounce => write!(f, "mail_bounce"),
            Self::MailBlacklist => write!(f, "mail_blacklist"),
            Self::MailRecipient => write!(f, "mail_recipient"),
            Self::MailServer => write!(f, "mail_server"),
            Self::Unknown => write!(f, "unknown"),
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
            Self::SmsExpired => write!(f, "sms_expired"),
            Self::SmsInvalidDestination => write!(f, "sms_invalid_destination"),
            Self::SmsNotAllowed => write!(f, "sms_not_allowed"),
            Self::SmsNotDelivered => write!(f, "sms_not_delivered"),
            Self::SmsRejected => write!(f, "sms_rejected"),
        }
    }
}

impl FromStr for NotificationFailureType {
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
            "sms_expired" => Ok(Self::SmsExpired),
            "sms_invalid_destination" => Ok(Self::SmsInvalidDestination),
            "sms_not_allowed" => Ok(Self::SmsNotAllowed),
            "sms_not_delivered" => Ok(Self::SmsNotDelivered),
            "sms_rejected" => Ok(Self::SmsRejected),
            _ => Err(format!("Unknown NotificationFailureType variant: {}", s)),
        }
    }
}

impl Default for NotificationFailureType {
    fn default() -> Self {
        Self::Unknown
    }
}
