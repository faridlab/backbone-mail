// This directory's extension (hand-written; ADR-0031). The generated mod.rs (lib.rs for
// src/) pulls it in with include!, so its lines resolve against this directory.

use crate::application::service::{
    ActivityWriteService, AliasWriteService, AttachmentWriteService, ChannelMemberWriteService,
    ChannelQueryService, ChannelWriteService, FollowerWriteService, GuestWriteService,
    MailBlacklistWriteService, MailQueueWriteService, MessageEditService, MessageQueryService,
    MessageWriteService, PhoneBlacklistWriteService, PhoneBookPort, PhoneBookSlot,
    PhoneValidationService, PresenceWriteService, ReactionWriteService, RecipientQueryService,
    ScheduleWriteService, SmsStatusWebhookService, SmsWriteService, StaticThreadAccess,
    ThreadAclSlot, ThreadAccessResolver, ThreadChatterService, TypingService,
};
/// The realtime surface (SSE session proofs; tailer + stream land in Stage 4).
/// Declared inside the CUSTOM block so regeneration preserves it.
pub mod realtime;
