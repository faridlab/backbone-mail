# backbone-mail — port register (Odoo `mail` + `sms` + `bus` → `messaging`)

> The port-time register for the Odoo 19 messaging stack (`addons/mail` 1.19, `addons/sms` 3.0,
> `addons/bus` 1.0; source commit `b9eb72eb`). Source of truth for the port decisions:
> `docs/odoo/messaging/{mail,sms,bus}/` (schema indexes carry the full flag registers).
>
> **Standing rule: flag IDs travel with the code.** When a model, behavior, or risk documented in
> the cycle docs is ported, deferred, or fixed, its flag ID (`MAIL-*`, `SM-*`, `BUS-*`, `MMB-*`)
> moves with it into the schema YAML comments, the service code, and this register. A flag that
> stops being citable is a lost audit trail.
>
> Module declaration: `module: messaging`, `schema: messaging`, `company_fence: none`
> (ADR-0014 posture 4 — the whole Odoo messaging stack ships no company dimension; do not
> synthesize one).

---

## 1. Ported entities — increment 1

The increment-1 cut: the persisted core (message → notify → send-queue pipeline, activities,
followers/presence/blacklist, aliases, Discuss channel + membership + guests, tracking values)
plus the full SMS transport trio. 22 entities.

| # | Entity | Flag IDs | Notes |
|---|--------|----------|-------|
| 1 | `MailMessage` | MAIL-M1 | `mail.message`; 43 fields; polymorphic `res_model`/`res_id` edge. **Hand-rolled ACL — zero ir.rules** (read rights walk to the parent document; MAIL-B1 obligation). `message_type` email/comment/notification (the `sms` value arrives with the SM-M6 overlay, increment 2). Raw-SQL indexes `_model_res_id_idx` + `_model_res_id_id_idx` preserved. |
| 2 | `Mail` | MAIL-M2 | `mail.mail`; the outgoing-email send-queue row. `_inherits mail.message` — port as delegation via owned FK `mail_message_id` + denormalized Integer mirror `mail_message_id_int` **without a DB FK** (the GC seam: mail row dies, message survives). `state` outgoing/sent/received/exception/cancel is HAND-SET and **NOT label-inverted**. `_send` pre-writes `state='exception'` before SMTP (crash-safety) — preserved. |
| 3 | `MailNotification` | MAIL-M3 (+ SM-M5 overlay fold) | `mail.notification`; the per-recipient delivery-status sink and **the origin of the label inversion** (see §4). Partial UniqueIndex `(mail_message_id, res_partner_id)`, 2 CHECK + 3 Index. The SM-M5 fold bakes the SMS columns in at increment 1: `notification_type += 'sms'`, `sms_id_int` (Integer, no FK — survives `sms.sms` GC), `sms_number`, and the 12 SMS `failure_type` values — so the SmsTracker pump (row 22) has its sink on day one. The `_gc_notifications` 180d policy defers its MAIL-B8 fix to increment 2. |
| 4 | `MailMessageSubtype` | MAIL-M4 | `mail.message.subtype`; the chatter event type (10 fields). create/write/unlink clear the registry cache. No constraints. |
| 5 | `MailMessageReaction` | MAIL-M5 | `mail.message.reaction`; emoji reactions, 4 readonly fields, 2 partial UniqueIndex + partner/guest XOR CHECK. |
| 6 | `MailFollowers` | MAIL-M10 | `mail.followers`; the subscription registry. G-MAIL-1 SQL-level `unique(res_model, res_id, partner_id)` preserved (fires on raw SQL — ADR-0015). `_insert_followers` policies (skip/force/replace/update) are service logic. |
| 7 | `MailPresence` | MAIL-M11 | `mail.presence`; IM presence, user/guest one2one CHECK + 2 partial UniqueIndex. `_gc_bus_presence` rides the autovacuum job. |
| 8 | `MailBlacklist` | MAIL-M24 | `mail.blacklist`; email denylist. G-MAIL-2 SQL `unique(email)` preserved; case-insensitivity stays APP-LAYER (not a DB citext) — documented, deliberate. `_add`/`_remove` archive pattern. |
| 9 | `MailTrackingValue` | MAIL-M22 | `mail.tracking.value`; typed field-change slots (old/new × int/float/char/text/datetime + currency). **TYPED SLOTS, NOT JSON** — do not "modernize" into a jsonb blob. |
| 10 | `MailActivity` | MAIL-M17 | `mail.activity`; 25 fields. `state` overdue/today/planned/**done** with `done` COMPUTED-FROM-ARCHIVE (`active=False`; you cannot `write(state='done')` — the only path is `_action_done`). G-MAIL-3/4 SQL CHECKs preserved. `_action_done` chaining pump (posts message, migrates attachments, archives, advances next only when `chaining_type=='trigger'`). |
| 11 | `MailActivityType` | MAIL-M19 | `mail.activity.type`; 22 fields. `chaining_type` suggest/trigger; bi-stable compute+inverse next-type pair; unlink reassigns to Todo. `_unlink_except_todo` is ORM-only — re-express as a service check + DB constraint where it guards correctness (ADR-0015). |
| 12 | `MailActivityPlan` | MAIL-M20 | `mail.activity.plan`; 8 fields. `res_model` domain filters `is_mail_activity=True`. |
| 13 | `MailActivityPlanTemplate` | MAIL-M21 | `mail.activity.plan.template`; 14 fields incl. the 5 "seed-from-type, user-overridable" compute+stored+readonly=False hybrids. |
| 14 | `MailAlias` | MAIL-M33 | `mail.alias`; 13 fields. Raw-SQL `UniqueIndex (alias_name, COALESCE(alias_domain_id,0))` — the only-NULL-domain protection is in the index expression; preserve verbatim. The ascii check is ORM-only (MAIL-ORM-1) — fine to lose on raw SQL, it's a UX check. |
| 15 | `MailAliasDomain` | MAIL-M34 | `mail.alias.domain`; 9 fields. G-MAIL-5 two SQL UNIQUEs (bounce_alias,name)+(catchall_alias,name). Per-company in Odoo but carries **no fence** here (posture 4; the "company" is a name scoping device, not a tenant). |
| 16 | `DiscussChannel` | MAIL-M37 | `discuss.channel` (v19 rename of `mail.channel`); **defined fresh, not extended**. Followers DISABLED (`_message_subscribe` always raises; membership is DiscussChannelMember). `create()` silently injects the creator as a member. `channel_type` immutable post-create. Raw-SQL patterns preserved: `channel_fetched FOR NO KEY UPDATE SKIP LOCKED`, pin UPDATE without write_date bump, `_get_or_create_chat` ARRAY_AGG set-equality. RTC seams (SFU threshold, 75s reaper) deferred — see §2. |
| 17 | `DiscussChannelMember` | MAIL-M38 | `discuss.channel.member`; ~18 fields. G-MAIL-8 partner/guest XOR CHECK + 2 partial UniqueIndex; immutability of (channel, partner, guest) re-expressed as service rule. `mute_until_dt` kept TZ-aware. |
| 18 | `MailGuest` | MAIL-M43 | `mail.guest`; the cookie guest identity (`dgid`), `access_token` uuid4 compared with `secrets.consteq` — keep constant-time comparison. |
| 19 | `Sms` | SM-M1/2 | `sms.sms`; the send-queue row + HAND-SET state machine with the **label inversion** (see §4). `uuid` unique (G-SM1) is the IAP correlation key. Lifecycle is event-driven: sync IAP response + async `/sms/status` webhook; the cron only DISPATCHES (`state='outgoing'`), never advances status. GC via autovacuum when `to_delete`. |
| 20 | `SmsTemplate` | SM-M3 | `sms.template`; the inline-template model (`body` is a `{{ object.x }}` inline template, `_unrestricted_rendering=True`). `model_id` domain gated by `is_mail_thread_sms`. No seeded records. **Port note:** its `mail.render.mixin` inheritance is NOT ported (M30 deferred) — see the discrepancy note in §2's mixins row; increment 1 ships a minimal inline-placeholder renderer inside the template's custom service. |
| 21 | `SmsTracker` | SM-M1/4 | `sms.tracker`; **NOTIFICATION-centric by design** (SM-M1 correction): exactly 2 fields — `sms_uuid` (unique, G-SM2) + `mail_notification_id`. The trace surface (`mailing_trace_id`, `SMS_STATE_TO_TRACE_STATUS`) is the mass_mailing_sms overlay and is NOT baked in. The `notifications_statuses_to_ignore` monotonic guard becomes a DB trigger (SM-B6, §3). |
| 22 | *(sink of row 21)* | — | The pump writes MailNotification.notification_status via the `SMS_STATE_TO_NOTIFICATION_STATUS` map — one map, owned by the tracker service, applied in the trigger + the sync-response path. |

## 2. Deferred models register

Everything the cycle docs catalogue that increment 1 does NOT port. Each row carries its risk
obligations forward — a deferred flag is an obligation that travels, not a forgotten row.

| Group | Flag IDs | Deferred to | Notes / risks that travel |
|-------|----------|-------------|---------------------------|
| Framework host extensions (23 rows: `res.users`, `res.partner`, `res.company`, `res.config.settings`, `ir.model`, `ir.model.fields`, `ir.actions.server`, `ir.attachment`, `ir.config_parameter`, `ir.websocket`, `ir.http`, `ir.cron`, `ir.qweb`, `ir.ui.menu`, `ir.ui.view`, `ir.actions.act_window.view`, `mail.thread.cc`, `mail.thread.main.attachment`, `mail.ice.server`, `publisher.warranty.contract`, `res.role`, `res.users.settings`, `res.users.settings.volumes`) | MAIL-M44..M66 | increment 2/3 | These extend HOST modules (`identity`, `party`, `sapiens`, `system`) — they port as overlays in the host modules, not as `messaging` entities. **MAIL-B7 travels with M44** (security-update email to the PREVIOUS address — must-fix-preserve, see §5). MAIL-M65 (`mail.ice.server`, STUN/TURN) is RTC support — defers with the RTC group. Note 4 of these rows are genuinely NEW persisted models (M48 res.role, M50 settings.volumes, M65 ice.server, M66 publisher.warranty) hiding in the "extensions" range — they re-home at deferral time. |
| Wizards (mail + sms transients: compose.message, blacklist.remove, followers.edit, template.preview, template.reset, activity.schedule, merge/uninstall; sms composer, template preview/reset, account phone/code/sender) | MAIL-M67..M74, SM-M16..M21 | increment 3 | Odoo transients have no backbone equivalent; each re-expresses as a service operation / command endpoint. **SM-B1 travels with SM-M21** (sender-name regex missing `$` anchor — fix when ported, it is both broken and ORM-only). **MMB-4-class duplicate-mint risk travels with MAIL-M67** mass mode (see §3). |
| Abstract mixins — `mail.thread` (M16), `mail.activity.mixin` (M18), `mail.thread.blacklist` (M25), `mail.render.mixin` (M30), `mail.composer.mixin` (M31), `template.reset.mixin` (M32), `mail.alias.mixin(.optional)` (M35/M36), `mail.tracking.duration.mixin` (M23), `mail.thread.cc` (M63), `mail.thread.main.attachment` (M64) | MAIL-M16, M18, M23, M25, M30..M32, M35/M36, M63/M64 | increment 2 | **Design note: chatter is a polymorphic edge, not a host column.** `mail.thread` contributes ZERO physical columns — every chatter field is an edge join back to `mail_message`/`mail_followers` via `(res_model, res_id)` (MAIL-M16). The backbone port has no inheritance mixin: a host module declares "I have chatter" and the edge is queried through this module's services. Never add `message_*` columns to host tables. |
| SMS host overlays (mail.message/message/notification/followers/thread extensions, ir.actions.server, ir.model, res.company, iap.account, base recipient-info) | SM-M6..SM-M15 | increment 2 (with their hosts) | Increment 1 folds ONLY SM-M5 (the MailNotification columns) because the tracker pump needs its sink. SM-M7/10/11 (`_message_sms`, `_notify_thread_by_sms`) is the 4th channel of the notify pump — the clean channel-extension seam; SM-M6's `message_type += 'sms'` on MailMessage lands with it. |
| `mail.push.device` + `mail.push` | MAIL-M13/M14 | increment 3 | **MAIL-B4 (🔴 must-fix) travels with this row:** a missing VAPID key DELETES ALL devices in Odoo. The port treats a missing/unreadable key as a hard configuration error — never a delete-all. MAIL-B3 (bare `except` masking push errors) fixed at the same time. |
| RTC / call history / GIF / voice | MAIL-M39..M42 (+M65) | increment 3 | `discuss.channel.rtc.session` (SFU ≥3, 75s reaper, JWT HS256 to SFU), `discuss.call.history`, `discuss.gif.favorite`, `discuss.voice.metadata`. Frontend-realtime-heavy; waits for the WebSocket surface decision. |
| Link previews | MAIL-M7, MAIL-M12 | increment 3 | `mail.message.link.preview` + `mail.link.preview` (URL preview cache, cap 5, UniqueIndex(source_url)). MAIL-B5 (the `_is_domain_thottled` typo — rate-limit silently dead) is fixed for free by rewriting it properly. |
| Translations | MAIL-M6 | increment 3 | `mail.message.translation`; UniqueIndex(message_id,target_lang), 2-week GC. |
| Scheduled messages — `mail.message.schedule` (M8), `mail.scheduled.message` (M9) | MAIL-M8/M9 | models deferred to increment 2; **the scheduled job is declared now** | The jobs skeleton (`schema/hooks`) declares the dispatch job names; the tables + `_send_notifications`/`_post_message` logic land in increment 2. Per ADR-0020 the pickup uses SKIP LOCKED (§3). |
| Inbound gateway — `fetchmail.server` (M27), `ir.mail_server` (M26), `mail.gateway.allowed` (M28) | MAIL-M26..M28 | increment 3 | Outbound SMTP server config + inbound IMAP/POP. **MAIL-B2 travels with M27** (separate-cursor per-message commit → partial-state on crash): the port moves fetch into the job runner's transactional pickup. MAIL-B6 (advisory-lock on 32-bit `hashtext`, collision-bucketed) re-expressed as a proper dedup key. Credentials follow ADR-0024 (env vars until the store exists — see `docs/adr-notes/outbox-fence-and-credentials.md`). |
| `mail.canned.response` | MAIL-M15 | increment 3 | Chat shortcuts; trivial, rides the Discuss UI increment. |
| `mail.template` + its render stack | MAIL-M29 (M30..M32 above) | **OUT permanently** | `mail.template` is NOT ported. Backbone already owns templated outbound comms: cross-ref **`backbone-notification`'s `NotificationTemplate`** (`modules/backbone-notification/schema/models/notification_template.model.yaml`). Email templates become notification templates; the RESTRICTED-render gatekeeper (`mail.group_mail_template_editor`) is a backbone-notification concern. Do not re-add a parallel template model here. |

## 3. Fixed-at-port 🔴 rows

Defects in the Odoo source that this port fixes at port time — not faithfully reproduced.

| Flag | Defect in Odoo | Fix at port | Authority |
|------|----------------|-------------|-----------|
| **SM-B6** | The monotonic status guard (`notifications_statuses_to_ignore`) is a pure-Python local-var dict filtered before the ORM write — no row lock on `mail.notification`; TOCTOU window on concurrent webhooks lets a `bounce` regress to `sent`. | **Hand-written DB trigger** enforcing the monotonic status lattice on `mail_notification.notification_status` (and the mirror on `sms_sms.state`): a transition that regresses status is rejected at the DB, surviving raw SQL, concurrent webhooks, and the sync-IAP path alike. The service-level map (`SMS_STATE_TO_NOTIFICATION_STATUS`) remains the single source for which advance is legal. | ADR-0015 (constraint enforcement declared + hard-linted; check-then-act service code does not survive concurrency), ADR-0017 §decision-4 (monotonic guards are DB-level). |
| **MMB-4** | Queue-drainer crons (`process_email_queue` on `mail.mail`, `_process_queue` on `sms.sms`) pick up rows with no row lock — concurrent workers mint duplicate sends. (Class exemplar: mass_mailing `:1180-1189`; the SMS side is MORE exposed because `sms.sms.create()` force-triggers the send cron — TR-SM-1.) | **`FOR UPDATE SKIP LOCKED` pickup on both queue drainers** — email and SMS. One claim per row per pass; concurrent workers drain disjoint sets; a crashed worker's claim expires with the transaction. | ADR-0020 (scheduler postures + the pickup-lock standard), ADR-0017 §decision-2 (same SKIP LOCKED standard as the outbox relay). |

## 4. Preserved-verbatim behaviors

Behaviors that LOOK like bugs and are load-bearing. Do not "fix" without a consumer audit.

| Flag | Behavior | Why it stays |
|------|----------|--------------|
| MAIL-M3 | `mail.notification.notification_status`: `pending` renders as **"Sent"**, `sent` renders as **"Delivered"**. THE inversion originates here — sms and mass_mailing copied it. | The bus todo counter, failure badges, and bounce/auto-blacklist all key off the VALUES. Port stores the raw enum values verbatim; any display-layer relabeling is a UI concern only. |
| SM-F19 | `sms.sms.state`: same inversion — `pending`="Sent", `sent`="Delivered". Only the `/sms/status` webhook advances a row to `sent`. | mass_mailing_sms inherits the same inversion on `trace_status`; the enum VALUES are cross-module contract. |
| MAIL-M2 | `mail.mail.state` is **NOT inverted** — `sent` means sent. Hand-set: `outgoing/sent/received/exception/cancel`; `_send` writes `exception` BEFORE the SMTP attempt. | The inversion lives on the notification row, not the mail row. "Fixing" Mail.state to match MailNotification would corrupt the send-queue semantics. |
| SM-M21 / MAIL-M2 / SM-M5 | Correlation by uuid/Integer **without a DB FK** in three places: `sms.sms.uuid ↔ sms.tracker.sms_uuid`, `mail.mail.mail_message_id_int`, `mail.notification.sms_id_int`. | The tracker/notification must OUTLIVE the sms/mail row's GC to keep delivery history visible. Introducing a real FK breaks the GC design. Deliberate decoupling — preserve it. |
| MAIL-M17 | `mail.activity.state='done'` is computed-from-archive (`active=False`); no write path to `done`. | The whole activity-archive semantics (and `_action_done`'s chaining) depends on done being unreachable by write. |

## 5. Increment-2 obligations

Registered debts — each carries its flag ID into the increment-2 backlog.

1. **BUS-B2 — str-only wire contract.** Bus wire channels must remain plain-`str` validated at the
   boundary; record-type channels are server-constructed. In the port: channel keys are built
   server-side at stage time; no consumer- or client-supplied channel string is ever trusted
   (see `docs/adr-notes/channel-key-dispatch.md`).
2. **MAIL-B7 — security-update email to the PREVIOUS address** (travels with the `res.users`
   extension, MAIL-M44). A hijacker who changes the email must not be able to suppress the
   warning to the real owner. Load-bearing — do NOT "fix" by sending to the new address.
3. **SM-B2 — `/sms/status` webhook → ADR-0021 scheme.** Odoo ships it `auth='public'` with
   UUID-only auth (unguessability + the monotonic guard as the entire defense). The port uses
   **raw-body HMAC or API re-fetch** — constant-time compare, timestamp window against replay,
   **fail-closed** on missing/invalid verification, and **no state writes before verification
   passes** (not even error states). The route itself is increment 2; the HMAC secret follows the
   ADR-0024 interim posture (env var, `docs/adr-notes/outbox-fence-and-credentials.md`).
4. **MAIL-B1 — document-level ACL.** `mail.message` read rights derive from the parent document
   (hand-rolled `_search`/`_check_access`, zero ir.rules). Port as a procedural check that walks
   the `(res_model, res_id)` edge to the parent and consults the parent's ACL — never an ir.rule
   keyed on a mail.message column (that over/under-exposes).
5. **SM-B13 — `process`-state orphans.** An `sms.sms` row in `process` is invisible to the
   dispatch cron (domain `state='outgoing'`); only the webhook advances it — no timeout, no
   re-queue, no alert exists. Port adds a stuck-in-process sweep (timeout → alert/re-queue) to
   the job skeleton.
6. **MAIL-B2 / B3 / B8.** B2: fetchmail per-message commit partial-state (rides the M27 deferral,
   §2). B3: `mail.push` bare-`except` masks send errors (rides MAIL-M13, §2). B8:
   `_gc_notifications` never reaps `partner_share=True` rows — unbounded portal accumulation;
   increment 2's GC reaps portal rows too (or partitions by age).
7. **BUS-B4 / BUS-B5 — mapped onto outbox/inbox.** B4 (bus GC is one unbounded raw DELETE):
   resolved structurally — the outbox tail is bounded by `published_at` and the partial index
   `idx_{schema}_outbox_unpublished` keeps the drain cheap; retention is a bounded archive job,
   never one giant DELETE. B5 (at-least-once, in-memory per-process dedup): consumers MUST be
   idempotent; the durable dedup layer is `inbox_consumed` (consumer, event_id), replacing the
   10s per-process window that replays across worker reconnects.

## 6. bus → backbone-outbox mapping (ADR-0017)

Odoo's `bus` is **a role, not a schema** — it is NOT ported as a module. `bus.bus` (BUS-M1),
`bus.listener.mixin` (BUS-M2), and `ir.websocket` (BUS-M3, bus-owned per the BUS-F1 correction)
map onto the existing outbox/inbox substrate:

| Odoo bus concept | Backbone mapping |
|---|---|
| `bus.bus` row (channel + message JSON, append-only) | An `OutboxRecord` staged in `messaging.outbox_events` inside the producer's transaction (`backbone_outbox::outbox::stage`) — the channel rides inside the payload envelope (see below). |
| Per-channel fan-out (`_sendone(channel, message)`) | Channel-key addressing inside the payload: `{"channel": "<channel-key>", "message": {"type": ..., "payload": ...}}` — mirrors `bus.bus`'s `(channel, message)` shape verbatim. Rationale and the why-not-a-framework-column argument: `docs/adr-notes/channel-key-dispatch.md`. |
| Cross-DB `pg_notify('imbus')` produce path (BUS-B3) | **Not ported.** The carrier of record is `FOR UPDATE SKIP LOCKED` polling via the relay (`backbone_outbox::relay::drain_once`); `pg_notify` at most a wakeup hint. A pooled/sharded store changes nothing about correctness. |
| `ImDispatch` LISTEN → websocket poke | Relay loop on a `backbone-jobs` schedule draining `WHERE published_at IS NULL` (cross-tenant by design — the relay is the one component that ignores company scoping). |
| At-least-once delivery + 10s per-process dedup (BUS-B5) | At-least-once stays the contract; idempotency moves from an in-memory window to the durable `inbox_consumed` log (`crate::inbox::once` dedups on the event id — `OutboxRecord.id` is the single end-to-end dedup key). |
| `_gc_messages` autovacuum (BUS-B4) | No unbounded DELETE: published rows age out via a bounded retention job; the unpublished tail stays indexed. |
| `_build_bus_channel_list` server-injected channels (BUS-B2) | Channel keys are constructed server-side at stage time only; consumers never gain authority by choosing a channel — preserved by construction (there is no subscriber-writable dispatch table). |

Mail and sms port as producers/consumers of this ONE substrate: the notify pump publishes, the
email/SMS queue drainers consume. Neither introduces its own queue tables.

---

## 7. Increment-2 scope register (the delivery surface)

**Locked owner decisions (2026-08-15):** transport = **SSE over the outbox** (no websocket — every
client→server op is a POST route; Odoo's ws is receive-push only); SSE feed = **read-only tailer
per replica**; scope = **full ~47-route surface staged** (schema → services → routes → realtime →
app); host = **new `apps/backbone-messaging-app`** backend-service.

### In scope (flag IDs)

| Surface | Flags | Notes |
|---|---|---|
| Chatter/thread routes (fetch/post/edit/reaction/star/subscribe/attachments, inbox/history/starred mailboxes) | MAIL-M16/B, MAIL-M1/M5 | `_message_fetch` pagination port; MAIL-B1 procedural ACL walk (below). |
| Discuss channel ops (12 routes: members, messages, pinned, mark_as_read, separator, typing, attachments, join, avatar, sub-channels, search) | MAIL-M37/M38 | `channel_fetched FOR NO KEY UPDATE SKIP LOCKED`; pin UPDATE without `updated_at` bump; `_get_or_create_chat` ARRAY_AGG set-equality. |
| Guest identity (`/mail/guest/update_name`, dgid middleware, public bootstrap endpoints) | MAIL-M43 | Public pages → JSON bootstrap, no HTML; find-or-create variants are POST (ADR-0019 rule 1). |
| Presence (`set_manual_im_status`, `update_bus_presence` with SSE-session proof) | MAIL-M11 | Broadcasts `bus.bus/im_status_updated` on the partner/guest channel. |
| `/sms/status` webhook | **SM-B2**, ADR-0021 | Raw-body HMAC, consteq, ±5min replay window, **fail-closed, zero state writes before verification**. |
| Scheduled-message models (M8/M9) + dispatch | MAIL-M8/M9 | Jobs declared in increment 1; tables + `dispatch_due` (SKIP LOCKED) land now. |
| Member state fields (separator, custom_notifications, mute_until_dt) | MAIL-M38 | Schema gap found at increment-2 planning. |
| Pinned messages, stars, slim attachments | MAIL-M37 adjunct, MAIL-M1, MAIL-M45 (slim) | `MailAttachment` is a minimal messaging-owned entity (access_token consteq, owner XOR); full `ir.attachment` stays with `system`. |
| New jobs: SM-B13 stuck-process sweep, presence/guest GC; MAIL-B8 GC change | SM-B13, MAIL-B8 | MAIL-B8 delta: the port has no `partner_share` flag, so the GC reaps ALL rows by age — the portal carve-out is dropped by documented decision. SM-B13 reconciliation: the requeue's `process → outgoing` regression is rank-falling under the SM-B6 guard, so migration `20260815220007` widens the guard with a one-shot escape — permitted ONLY on the UPDATE that mints the `swept_at` marker (NULL → set). The marker is the SM-B13 bound itself (set once per row), so the escape cannot be replayed; it is the only sanctioned regression path to `'outgoing'`. |
| Chatter seam (`ThreadAccessResolver` + `ChatterService`) | MAIL-M16, MAIL-B1 | Host modules register a resolver via `MessagingModuleBuilder::with_thread_acl`; default DenyHostDocs. |
| SSE realtime (registry, tailer, session proof, stream) | BUS-B2/B5, ADR-0017 | Channel-key validation at the boundary (plain-str only); `Last-Event-ID` watermark with clamp-to-window; per-replica read-only tailer over `outbox_events` (55s window, 1s poll) feeding the local broadcast registry — the relay stays carrier of record for durable consumers. |

### Deferred from increment 2 (with justification — obligations travel)

| Item | Flags | Deferred to | Why |
|---|---|---|---|
| MAIL-B7 (security email to PREVIOUS address) | MAIL-B7, MAIL-M44 | increment 3 (gateway group M26/M27) | No SMTP transport exists until the gateway group — "email the previous address" is untestable dead code now. **Seam ships:** `UserEmailChanged` outbox event contract for the sapiens `User` host (the User host is `backbone-sapiens`, NOT a `backbone-identity` — that module doesn't exist). |
| Link-preview routes | MAIL-M7/M12 | increment 3 | Routes without their persistence are stubs; defer with the models. |
| RTC, push devices, fetchmail/ir.mail_server, wizards, translations, canned responses, GIF/voice | MAIL-M13/M14, M26..M28, M39..M42, M15, M6 | increment 3 | Per §2 unchanged. |

### Posture decisions

- **ADR-0019 lint debt:** the safe-method reachability lint does not exist yet. Compensating
  controls: every ported route is POST by construction (the two GET handlers — SSE stream +
  read-only bootstrap — are snapshot-tested for zero side effects); a manual method-audit
  checklist runs at Stage 6. The framework lint itself is registered debt, not scope.
- **Throttling:** enumeration-shaped endpoints (`/discuss/search`, recipients lookups,
  `/mail/partner/from_email`, public bootstrap, `/sms/status`, attachment ops) are throttled with
  the EXISTING `backbone-rate-limit` middleware (ADR-0019 rule 3) — no new framework code.
- **Auth:** fence-none (ADR-0014 posture 4) ⇒ the app uses user-scope auth + guest middleware,
  NEVER `company_auth` (no company context exists to prove).

### Increment-2 closure (2026-08-15)

Landed. Obligation ledger (§5 refs → resolution):

| Obligation | Status |
|---|---|
| BUS-B2 str-only wire contract | **Closed.** `realtime::tailer::valid_channel_key` (`^[A-Za-z0-9_.-]+$`, ≤128) at the SSE boundary; record-shaped keys resolved per-identity via `ThreadAccessResolver` — out-of-allowlist events are dropped + warn-logged (`mail::realtime_stream`), never 403 (no enumeration oracle). |
| MAIL-B7 previous-address email | **Deferred → increment 3** (gateway group), seam as registered in the table above. |
| SM-B2 `/sms/status` HMAC | **Closed.** Raw-body HMAC-SHA256, consteq, ±5min window, fail-closed 503 on unset secret, zero pre-verification writes — proven by row-snapshot tests (commit abca0ec). |
| MAIL-B1 document-level ACL | **Closed.** `chatter_acl::ThreadAccessResolver` (default `DenyHostDocs`); the SSE allowlist and `_message_fetch` both consult it procedurally — no rule-shaped WHERE on `mail.message`. |
| SM-B13 stuck-process sweep | **Closed.** `GcService::sweep_stuck_process` + the widened SM-B6 guard escape (migration `20260815220007`, one-shot via the `swept_at` marker). |
| MAIL-B2/B3 (fetchmail/push error masking) | **Ride the M13/M27 increment-3 deferrals** — unchanged. |
| MAIL-B8 notification GC | **Closed.** Reaps ALL rows by age (portal carve-out dropped — documented delta); `GcService::notification_gc`. |
| BUS-B4/B5 (bus GC / dedup) | **Closed structurally** per §6; the SSE tailer adds per-replica in-memory dedup keyed on the outbox id (window overlap + dedup, never a watermark that can skip rows). |

**ADR-0019 manual method-audit (Stage 6):** 61 custom routes audited — 56 POST, 5 GET. The GETs:
`/mail/inbox/unread_count`, `/mail/channels/unread_counts`, `/discuss/channel/:id/members`,
`/discuss/channel/:id/pinned` (all query-service reads, no writes), and `/mail/realtime/stream`
(SSE; reserves an in-memory connection slot and mints a session proof — no domain-state writes).
Find-or-create surfaces (`/discuss/chat`, `/discuss/public/bootstrap`, `/mail/guest`) are POST.

**App composition:** `apps/backbone-messaging-app` (registered in the root `metaphor.yaml`) —
user-scope verifier (backbone-auth ships no user-scope verifier; the app adds a thin one proving
the partner id from the Bearer JWT sub), guest middleware, bare-mounted webhook, 7 job runners,
outbox relay (logging publish seam — increment-3 transport), per-replica SSE tailer. Increment-3
seams documented in the app README: noop sms provider, relay publish seam, missing `sms::gc`.

---

## 8. Increment-3 scope register (the gateway core)

**Locked owner decisions (2026-08-15):** scope = **gateway core only** (outbound SMTP over the
existing framework `backbone-email` crate, inbound email, MAIL-B7, relay→backbone-notification
transport wiring, `sms::gc`); inbound shape = **webhook** (`POST /mail/inbound/:server_id`, token
consteq — the proven `/sms/status` ADR-0021 playbook; SES/SendGrid-style inbound parse; an
IMAP/POP poller can later land behind the same `MailInboundService`); SMS provider = **generic
HTTP JSON adapter** mirroring Odoo's IAP batch shape (endpoint + token env refs), noop retained.

### In scope (flag IDs)

| Surface | Flags | Notes |
|---|---|---|
| `MailServer` (ir.mail_server) + `MailApiPort` + queue send path | MAIL-M26 | Server-selection ladder (exact from_filter address → domain → no-filter → first by sequence); failure vocabulary = mail.mail's (`mail_smtp`, `mail_email_invalid`, `mail_email_missing`, `mail_from_invalid`, `mail_from_missing`, `mail_spam`, `unknown`). Transport lives in the APP over `backbone-email` — the module holds the port. |
| `FetchmailServer` (webhook mode) + `MailGatewayAllowed` + inbound pipeline | MAIL-M27, MAIL-M28, **MAIL-B2**, **MAIL-B6** | MAIL-B2 fix: one transaction per message, dedup-guarded. MAIL-B6 fix: the advisory-lock-on-hashtext is replaced by a UNIQUE partial index on `mail_message.message_id WHERE message_id IS NOT NULL`. IMAP/POP transport fields deliberately absent — 3b adds them with the poller. |
| MAIL-B7 previous-address security email | **MAIL-B7** | `UserEmailChanged` contract `{user_id, previous_email, new_email, changed_at}` — staged by the sapiens User host; the APP's relay consumer sends the warning to the PREVIOUS address through the mail queue. Load-bearing: never "fix" by sending to the new address. |
| relay → backbone-notification transport wiring | app seam | The app implements notification's `CommunicationPort` over the SAME gateway adapters (one gateway, two producers) + mounts its routes and `dispatch_pending` job. |
| `sms::gc` | job roster | Bounded reap of `to_delete` + terminal-state Sms rows (the MAIL-B8-age analog for sms). |
| Generic HTTP SMS adapter | SM group | IAP batch shape `{content, numbers:[{uuid, number}]}`; state map `processing→process`, `success/sent→pending`, `delivered→sent`; error codes → `sms_credit`, `sms_number_format`, `sms_country_not_supported`, `sms_server`, `sms_acc`. |

### Closure ledger (2026-08-15, increment 3 landed)

| Scope row | Status | Proof |
|---|---|---|
| `MailServer` + `MailApiPort` + queue send path (MAIL-M26) | ✅ closed | selection ladder tests (exact/domain/wildcard/sequence, case-insensitive); queue send path via port (fake port success/failure); `process_queue(port, batch, max_batches)` signature live |
| `FetchmailServer` + `MailGatewayAllowed` + inbound pipeline (MAIL-M27/M28) | ✅ closed | `POST /mail/inbound/:server_id` bare-mounted, POST-only (ADR-0019 audit 2026-08-15); zero-writes-before-auth proven by full-table snapshot test; replay idempotent; sender-reject stages `InboundEmailRejected` with no message row |
| **MAIL-B2** (per-message commit partial-state) | ✅ closed | one transaction per message, dedup-guarded — the webhook path never holds a partial cursor at all |
| **MAIL-B6** (advisory-lock-on-hashtext dedup) | ✅ closed | UNIQUE partial index `mail_messages.message_id WHERE message_id IS NOT NULL`; conflict → idempotent 200 |
| **MAIL-B7** (previous-address security email) | ✅ closed | module-side `UserEmailChanged` contract + app relay consumer; verified live 2026-08-15 against a hand-staged event (mail row landed on `victim@old.example`, outgoing); single-tx `message_post` ⇒ relay retry cannot double-send |
| `sms::gc` | ✅ closed | `GcService::sms_gc(retention_days)` + app job `sms::gc` (retention under `gc.sms_retention_days`) |
| Generic HTTP SMS adapter | ✅ closed | `HttpSmsApi` (reqwest, IAP-shaped) in the app; state/error-code map unit-tested; boot gate `jobs.sms_provider: noop\|http` |
| relay → backbone-notification transport wiring | ⛔ **deferred to 3b — blocked seam** | backbone-notification is company-fence RLS (its write paths wrap `with_company_scope`; `dispatch_pending` needs a per-company sweep); backbone-messaging-app is fence-none (ADR-0014 posture 4) with no company registry in its DB. Composing it today would require an RLS-bypassing role — a silently wrong production posture. Revisit with the 3b push/RTC work (also blocked: no `notify()` producers exist in this app yet). |

### Credential posture (ADR-0024 interim, one documented deviation)

- SMTP **passwords**: env-var REFERENCES (`smtp_pass_ref` = env var name; the app resolves at send
  time). Never a literal in DB or config.
- Inbound webhook **token** (per-row, unsuited to env vars): the `FetchmailServer` row stores only
  its **SHA-256 hash** (`token_hash`); the route hashes the presented token and consteq's. No
  plaintext secret in DB, no per-row env sprawl. This is a deliberate, documented deviation from
  the env-ref-only interim posture — strictly stronger than Odoo's plaintext column.

### Deferred to 3b (obligations travel)

| Item | Flags | Traveling obligations |
|---|---|---|
| Push devices | MAIL-M13, MAIL-M14 | **MAIL-B3** (bare-except masks push errors) and **MAIL-B4 (🔴)**: missing VAPID key DELETES ALL devices in Odoo — port treats missing/unreadable key as hard config error, never delete-all. Community v19 is WebPush-only (VAPID in config params); FCM/APNs are Enterprise. |
| Link previews | MAIL-M7, MAIL-M12 | **MAIL-B5**: `_is_domain_thottled` typo kills rate limiting — rewrite properly. URL fetcher is an SSRF surface (cap 5 previews/message, UniqueIndex source_url, domain throttle 10s). |
| Canned responses | MAIL-M15 | Rides the Discuss UI increment; `::`-prefix substitution is frontend. |
| Translations | MAIL-M6 | UniqueIndex (message_id, target_lang), 2-week GC; needs a translate-provider decision. |
| Wizards (compose/message, sms) | MAIL-M67..M74, SM-M16..SM-M21 | **SM-B1** (sender-name regex missing `$` anchor) + **MMB-4-class** duplicate-mint risk on mass mode. Re-express as command endpoints, not transient models. |
| RTC / call history / GIF / voice / ICE | MAIL-M39..M42, MAIL-M65 | Waits on the websocket-surface decision (SFU ≥3, 75s reaper, JWT HS256 sessions). |
| IMAP/POP poller | MAIL-M27 adjunct | Webhook chose first; poller lands behind the same service if a customer needs mailbox polling. |
| backbone-notification composition (relay→transport fan-out) | app seam | **Blocked, not forgotten** (owner decision 2026-08-15): company-fence RLS module vs a fence-none app DB — composing needs an RLS-bypassing role or a company registry in the app. Revisit with the push/RTC work, when `notify()` producers also first exist. See closure ledger above. |
