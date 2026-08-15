mod mail_hooks_state_machine;
mod mail_activity_type_hooks_state_machine;
mod mail_notification_hooks_state_machine;
mod mail_presence_hooks_state_machine;
mod sms_hooks_state_machine;

/// Shared error type for all state machines in this module
#[derive(Debug, Clone, thiserror::Error)]
pub enum StateMachineError {
    #[error("Invalid state: {0}")]
    InvalidState(String),

    #[error("Invalid transition: {0}")]
    InvalidTransition(String),

    #[error("Transition '{transition}' not allowed from state '{from}'")]
    TransitionNotAllowed {
        transition: String,
        from: String,
    },

    #[error("Role '{role}' not authorized for transition '{transition}'")]
    RoleNotAuthorized {
        role: String,
        transition: String,
    },

    #[error("Guard condition failed for transition '{0}'")]
    GuardFailed(String),

    #[error("Cannot transition from final state '{0}'")]
    FinalStateReached(String),
}

pub use mail_hooks_state_machine::{MailHooksState, MailHooksTransition, MailHooksStateMachine};
pub use mail_activity_type_hooks_state_machine::{MailActivityTypeHooksState, MailActivityTypeHooksTransition, MailActivityTypeHooksStateMachine};
pub use mail_notification_hooks_state_machine::{MailNotificationHooksState, MailNotificationHooksTransition, MailNotificationHooksStateMachine};
pub use mail_presence_hooks_state_machine::{MailPresenceHooksState, MailPresenceHooksTransition, MailPresenceHooksStateMachine};
pub use sms_hooks_state_machine::{SmsHooksState, SmsHooksTransition, SmsHooksStateMachine};
