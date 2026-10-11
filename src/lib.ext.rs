//! The module's extension (hand-written; ADR-0031): what the module adds beside its
//! generated services. The generated module and builder carry `ModuleExt` and
//! `ModuleBuilderExt` and dereference to them, so their fields read as `module.field`.

#[allow(unused_imports)]
use super::*;

/// State the module adds; read through the generated module's `Deref`.
pub struct ModuleExt {
    /// Hand-authored write services (the validated write surface — generic CRUD
    /// bypasses domain invariants by design). Each owns one Odoo-ported behavior:
    /// message_post notify pump (MAIL-M16/B), follower policies (MAIL-M10),
    /// activity chaining (MAIL-M17), the sms queue drainer (SM-M1/2, MMB-4),
    /// the mail queue drainer (MAIL-M2, MMB-4), alias resolution (MAIL-M33).
    pub message_write_service: Arc<MessageWriteService>,
    pub follower_write_service: Arc<FollowerWriteService>,
    pub activity_write_service: Arc<ActivityWriteService>,
    pub sms_write_service: Arc<SmsWriteService>,
    pub mail_queue_write_service: Arc<MailQueueWriteService>,
    pub alias_write_service: Arc<AliasWriteService>,
    /// The swappable MAIL-B1 resolver slot — install a host's thread ACL with
    /// [`MessagingModule::set_thread_acl`]. Defaults to DenyHostDocs.
    pub thread_acl: ThreadAclSlot,
    /// The host-facing chatter edge (post/read/follow/schedule, all gated).
    pub thread_chatter_service: Arc<ThreadChatterService>,
    /// Increment-2 write services (user-owned; see each file's header).
    pub channel_write_service: Arc<ChannelWriteService>,
    pub channel_member_write_service: Arc<ChannelMemberWriteService>,
    pub typing_service: Arc<TypingService>,
    pub reaction_write_service: Arc<ReactionWriteService>,
    pub message_edit_service: Arc<MessageEditService>,
    pub guest_write_service: Arc<GuestWriteService>,
    pub presence_write_service: Arc<PresenceWriteService>,
    pub schedule_write_service: Arc<ScheduleWriteService>,
    pub attachment_write_service: Arc<AttachmentWriteService>,
    /// Increment-2 query services (MAIL-B1 procedural gates live here).
    pub message_query_service: Arc<MessageQueryService>,
    pub channel_query_service: Arc<ChannelQueryService>,
    pub recipient_query_service: Arc<RecipientQueryService>,
    /// The shared pool (route composers build one-off services — e.g. the
    /// sms webhook — without re-threading a pool through every caller).
    pub pool: sqlx::PgPool,
    /// The replica-local realtime fan-out (SSE ring + connection caps). The
    /// host app spawns the outbox tailer onto THIS same instance — the
    /// identity between the two is what makes post→stream delivery work.
    pub realtime_registry: std::sync::Arc<realtime::RealtimeRegistry>,
    /// The phone-validation opener (user-owned): the blacklist verbs and the
    /// live sanitized-for verb over the swappable phone-book slot.
    pub phone_blacklist_write_service: Arc<PhoneBlacklistWriteService>,
    /// The email-blacklist verbs (user-owned): the sanctioned write path for
    /// mail_blacklists — add (recording the opt-out reason when supplied),
    /// archive-only remove, point/batch membership checks.
    pub mail_blacklist_write_service: Arc<MailBlacklistWriteService>,
    /// The swappable recipient-candidate slot — install a host's phone book
    /// with [`MessagingModule::set_phone_book`]. Defaults to the refusing
    /// (fail-closed) book.
    pub phone_book: PhoneBookSlot,
    /// The live read-side verb: candidates → E.164 → blacklist standing.
    pub phone_validation_service: Arc<PhoneValidationService>,
}

/// State the builder adds.
pub struct ModuleBuilderExt {
}

impl Default for ModuleBuilderExt {
    fn default() -> Self {
        Self {
        }
    }
}

impl ModuleBuilderExt {
    /// Build the extension's state from what the generated build made.
    #[allow(unused_variables, clippy::redundant_clone)]
    pub(crate) fn build(self, parts: &ModuleParts<'_>) -> anyhow::Result<ModuleExt> {
        let db_pool = parts.db_pool.clone();
        let message_write_service = Arc::new(MessageWriteService::new(db_pool.clone()));
        let follower_write_service = Arc::new(FollowerWriteService::new(db_pool.clone()));
        let activity_write_service = Arc::new(ActivityWriteService::new(db_pool.clone()));
        let sms_write_service = Arc::new(SmsWriteService::new(db_pool.clone()));
        let mail_queue_write_service = Arc::new(MailQueueWriteService::new(db_pool.clone()));
        let alias_write_service = Arc::new(AliasWriteService::new(db_pool.clone()));
        // The shared MAIL-B1 slot (default DenyHostDocs — hosts install via
        // set_thread_acl post-build). Every gated service resolves through it.
        let thread_acl = ThreadAclSlot::default();
        let acl: Arc<dyn ThreadAccessResolver> = Arc::new(thread_acl.clone());
        let thread_chatter_service =
            Arc::new(ThreadChatterService::new(db_pool.clone(), Arc::clone(&acl)));
        let schedule_write_service =
            Arc::new(ScheduleWriteService::new(db_pool.clone(), Arc::clone(&acl)));
        let message_query_service =
            Arc::new(MessageQueryService::new(db_pool.clone(), Arc::clone(&acl)));
        let recipient_query_service =
            Arc::new(RecipientQueryService::new(db_pool.clone(), Arc::clone(&acl)));
        let channel_write_service = Arc::new(ChannelWriteService::new(db_pool.clone()));
        let channel_member_write_service =
            Arc::new(ChannelMemberWriteService::new(db_pool.clone()));
        let channel_query_service = Arc::new(ChannelQueryService::new(db_pool.clone()));
        let typing_service = Arc::new(TypingService::new(db_pool.clone()));
        let reaction_write_service = Arc::new(ReactionWriteService::new(db_pool.clone()));
        let message_edit_service = Arc::new(MessageEditService::new(db_pool.clone()));
        let guest_write_service = Arc::new(GuestWriteService::new(db_pool.clone()));
        let presence_write_service = Arc::new(PresenceWriteService::new(db_pool.clone()));
        let attachment_write_service = Arc::new(AttachmentWriteService::new(db_pool.clone()));
        // The replica-local SSE fan-out point (Stage 4). One per process;
        // the host app's tailer publishes into it, the stream route reads.
        let realtime_registry = std::sync::Arc::new(realtime::RealtimeRegistry::new());
        // The phone-validation opener: the blacklist verbs + the live
        // sanitized-for verb over the fail-closed phone-book slot (hosts
        // install a real book via set_phone_book post-build).
        let phone_blacklist_write_service =
            Arc::new(PhoneBlacklistWriteService::new(db_pool.clone()));
        // The email-blacklist verbs: add (recording opt_out_reason_id when
        // supplied), archive-only remove, membership checks.
        let mail_blacklist_write_service =
            Arc::new(MailBlacklistWriteService::new(db_pool.clone()));
        let phone_book = PhoneBookSlot::default();
        let phone_validation_service = Arc::new(PhoneValidationService::new(
            db_pool.clone(),
            phone_book.clone(),
            phone_blacklist_write_service.clone(),
        ));
        Ok(ModuleExt {
            message_write_service,
            follower_write_service,
            activity_write_service,
            sms_write_service,
            mail_queue_write_service,
            alias_write_service,
            thread_acl,
            thread_chatter_service,
            channel_write_service,
            channel_member_write_service,
            typing_service,
            reaction_write_service,
            message_edit_service,
            guest_write_service,
            presence_write_service,
            schedule_write_service,
            attachment_write_service,
            message_query_service,
            channel_query_service,
            recipient_query_service,
            pool: db_pool.clone(),
            realtime_registry,
            phone_blacklist_write_service,
            mail_blacklist_write_service,
            phone_book,
            phone_validation_service,
        })
    }
}

impl crate::ThisModule {
    /// Register the host's thread ACL (MAIL-B1). Installs into the shared
    /// slot — every gated service (chatter facade, queries, schedules) sees
    /// the resolver from the next call. Idempotent (last install wins).
    pub fn set_thread_acl(&self, resolver: std::sync::Arc<dyn ThreadAccessResolver>) {
        self.thread_acl.install(resolver);
    }
    /// Register the host's phone book — the recipient-candidate walk for
    /// `PhoneValidationService::sanitized_for` over another module's
    /// records. Installs into the shared slot; until called, every lookup
    /// refuses (fail-closed, never a silent empty). Idempotent (last
    /// install wins).
    pub fn set_phone_book(&self, book: std::sync::Arc<dyn PhoneBookPort>) {
        self.phone_book.install(book);
    }
    /// Convenience: open a static set of document models to every
    /// authenticated partner — the config-driven resolver an app service can
    /// build without writing its own trait impl.
    pub fn set_static_thread_access(&self, open_models: Vec<String>) {
        self.thread_acl.install(std::sync::Arc::new(StaticThreadAccess { open_models }));
    }
    /// The shared pool (for app-level wiring: jobs, relay, one-off services).
    pub fn db_pool(&self) -> sqlx::PgPool {
        self.pool.clone()
    }
    // Route composers (increment 2 — the guarded wire surface). Kept inside
    // the CUSTOM METHODS block so regeneration preserves them.
    /// The guarded discuss/channel group: create, chat dedup, member verbs,
    /// typing, search, reactions, edits, stars. Mount behind app user auth +
    /// the `guest_context` middleware. Search is throttled (30/60s).
    pub fn discuss_routes(self: &Arc<Self>) -> Router {
        presentation::http::channel_routes::composer().with_state(self.clone())
    }
    /// The guarded thread/chatter group: fetches, counters, recipients,
    /// post/follow/schedule. Mount behind app user auth + `guest_context`.
    /// Recipient suggestions are throttled (30/60s).
    pub fn messaging_routes(self: &Arc<Self>) -> Router {
        presentation::http::thread_routes::composer().with_state(self.clone())
    }
    /// Guest self-service (mint + rename) — public surface behind
    /// `guest_context` only.
    pub fn guest_routes(self: &Arc<Self>) -> Router {
        presentation::http::guest_routes::composer().with_state(self.clone())
    }
    /// Presence (manual im_status + proof-gated bus liveness). Requires the
    /// `realtime::SessionSecret` router extension for the proof check.
    pub fn presence_routes(self: &Arc<Self>) -> Router {
        presentation::http::presence_routes::composer().with_state(self.clone())
    }
    /// Attachment register/attach/detach/token verbs (owner-gated,
    /// throttled 30/60s).
    pub fn attachment_routes(self: &Arc<Self>) -> Router {
        presentation::http::attachment_routes::composer().with_state(self.clone())
    }
    /// The public bootstrap — anonymous/guest entry to a public channel.
    /// Throttled hard (10/60s): unauthenticated by construction.
    pub fn public_routes(self: &Arc<Self>) -> Router {
        presentation::http::public_routes::composer().with_state(self.clone())
    }
    /// The sms provider callback (`POST /sms/status`, scheme `hmac_raw_body`
    /// per ADR-0021). BARE mount — no auth middleware; the HMAC IS the auth.
    /// Reads `SMS_WEBHOOK_SECRET` (env-var reference, ADR-0024); when unset
    /// the route answers 503 — fail closed, never forge-able with an empty key.
    /// Throttled 120/60s.
    pub fn webhook_routes(self: &Arc<Self>) -> Router {
        presentation::http::webhook_routes::composer().with_state(self.clone())
    }
    /// The inbound email webhook (`POST /mail/inbound/:server_id`, MAIL-M27
    /// webhook mode). BARE mount — no auth middleware; the per-server bearer
    /// token IS the auth (SHA-256 consteq against the stored hash; unknown
    /// server / inactive server / bad token all answer the same 401 — no
    /// oracle). Throttled 120/60s.
    pub fn inbound_routes(self: &Arc<Self>) -> Router {
        presentation::http::inbound_routes::composer().with_state(self.clone())
    }
    /// The realtime SSE stream (`GET /mail/realtime/stream`). Requires the
    /// `guest_context` middleware AND the `realtime::SessionSecret` router
    /// extension — the same contract as the presence group. The host app
    /// also spawns [`realtime::tailer::spawn`] onto
    /// [`Self::realtime_registry`] or the stream serves only its keepalives.
    pub fn realtime_routes(self: &Arc<Self>) -> Router {
        realtime::sse::composer().with_state(self.clone())
    }
}
