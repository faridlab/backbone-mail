// This directory's extension (hand-written; ADR-0031). The generated mod.rs (lib.rs for
// src/) pulls it in with include!, so its lines resolve against this directory.

// Hand-written messaging SQL (increment 1; user-owned files declared in
// metaphor.codegen.yaml). Distinct names from the generated per-entity
// repositories — these hold the write-path SQL the services above orchestrate.
pub mod message_pipeline_repository;
pub mod follower_repository;
pub mod activity_repository;
pub mod sms_queue_repository;
pub mod mail_queue_repository;
pub mod alias_resolution_repository;
// Increment-2 SQL (user-owned): channel/member ops, reactions, edits, stars,
// presence, guests, schedules, GC/sweeps, attachments, and the chatter reads.
pub mod channel_repository;
pub mod channel_member_repository;
pub mod reaction_repository;
pub mod message_edit_repository;
pub mod presence_repository;
pub mod guest_repository;
pub mod schedule_repository;
pub mod gc_repository;
pub mod attachment_repository;
pub mod chatter_repository;
// Increment-3 gateway SQL (user-owned): the SMTP selection ladder
// (_find_mail_server) and the inbound pipeline reads/writes.
pub mod smtp_selection_repository;
pub mod inbound_repository;
// Phone-validation opener SQL (user-owned): the phone-blacklist verbs —
// canonical upsert add, conditional archive remove, point/batch membership.
// Distinct path from the generated per-entity phone_blacklist_repository.
pub mod phone_blacklist_verb_repository;
// Email-blacklist verb SQL (user-owned): the same add/remove/membership
// shape over mail_blacklists, carrying the opt_out_reason_id column.
// Distinct path from the generated per-entity mail_blacklist_repository.
pub mod mail_blacklist_verb_repository;
