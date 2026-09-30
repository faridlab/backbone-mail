//! Regression guard: the unguarded `all_crud_routes()` composer must NOT mount
//! generic CRUD on the machine-bearing messaging models — the mail state
//! columns (MailHooks and siblings over Mail / MailNotification /
//! MailPresence / MailActivityType, the discuss channel lifecycle, the SMS
//! lifecycle) move only through the queue's state-guarded SQL and the module's
//! verbs; a generic full-row PATCH would bypass the declared transition set
//! (e.g. a mail could skip outgoing straight to sent with no SMTP leg). The
//! generator narrows these mounts to the read surface on its own; this test
//! reads `src/lib.rs` and fails the build if a regen ever re-adds a generic
//! write mount for them.

const LIB_RS: &str = include_str!("../src/lib.rs");

/// The machine-bearing models whose generic write mounts are deliberately
/// excluded from `all_crud_routes`. Each entry is the exact write-mount call
/// site (function + the service field it would be called with).
const EXCLUDED_MACHINE_OWNED_ROUTE_MOUNTS: &[&str] = &[
    "create_mail_routes(self.mail_service",
    "create_mail_notification_routes(self.mail_notification_service",
    "create_mail_presence_routes(self.mail_presence_service",
    "create_mail_activity_type_routes(self.mail_activity_type_service",
    "create_discuss_channel_routes(self.discuss_channel_service",
    "create_sms_routes(self.sms_service",
];

#[test]
fn all_crud_routes_excludes_machine_owned_models() {
    for mount in EXCLUDED_MACHINE_OWNED_ROUTE_MOUNTS {
        assert!(
            !LIB_RS.contains(mount),
            "regression: `all_crud_routes` mounts a machine-bearing model's write route ({mount}). \
             A schema regen has re-added it. The messaging state columns move only through the \
             queue's guarded writes.",
        );
    }
}
