# ADR-note: fence-none × outbox company_id, and the interim credential posture

> Status: **owner-approved** (messaging port, increment 1). Documents two decided questions;
> not re-deciding. Companions: [ADR-0014](../../../../docs/handbook/adr/0014-odoo-port-company-fence-vocabulary.md)
> (company-fence vocabulary), [ADR-0011](../../../../docs/handbook/adr/0011-outbox-inbox-tenancy.md)
> (outbox/inbox tenancy), [ADR-0024](../../../../docs/handbook/adr/0024-one-oauth-generation-and-credential-store.md)
> (credential store), [ADR-0021](../../../../docs/handbook/adr/0021-webhook-verification-standard.md)
> (webhook verification). Register cross-refs: `../port-notes.md` §1, §5.

## (a) `company_fence: none` meets `outbox_events.company_id NOT NULL`

**The collision.** ADR-0014 posture 4 declares messaging has *no company dimension* — no RLS
policy is emitted for the module's own tables, and synthesizing one is forbidden ("the messaging
stack would grow company rows it never had, breaking its at-least-once consumers that key on
channels, not companies"). But the framework's outbox **requires** a tenant on every row:
`OutboxRecord.company_id` is a mandatory field and `outbox_events.company_id` is
`uuid NOT NULL` (see `outbox.rs::migrate` — the column is created unconditionally; only the RLS
fence is conditional).

**The framework fact.** backbone-outbox enables RLS **only under its `multi_tenant` feature**:

```rust
// modules/backbone-framework/backbone-outbox/src/outbox.rs
// ... company_id uuid NOT NULL — always created ...
#[cfg(feature = "multi_tenant")]
{
    // ENABLE/FORCE ROW LEVEL SECURITY + the ADR-0011 company-isolation policy
}
```

The feature exists "so the framework stays tenant-agnostic; a company-tenant service enables it."

**Decision (two halves):**

1. **`backbone-mail` depends on backbone-outbox WITHOUT the `multi_tenant` feature.** Messaging's
   `outbox_events` table therefore gets no RLS fence — which is exactly the correct posture-4
   outcome per ADR-0014: the module is *outside* the company-fence dimension by declaration, and
   an unfenced table is the declaration made real. (Note the asymmetry with fenced modules: for
   them the fence is opt-in via the same feature; for messaging the *absence* is the choice, and
   it must be stated here so a future "enable multi_tenant repo-wide" sweep knows this module
   deliberately stays out.)

2. **Every staged event carries the module-constant sentinel
   `MESSAGING_PLATFORM_COMPANY_ID = Uuid::nil()`** as its `company_id`, with the comment
   citing ADR-0014 at the constant's definition:

   ```rust
   /// ADR-0014 posture 4 (company_fence: none): messaging has no company dimension.
   /// The outbox column is NOT NULL, so platform events carry the nil sentinel.
   /// Consumers key on channels, not companies — never filter these rows by company.
   pub const MESSAGING_PLATFORM_COMPANY_ID: Uuid = Uuid::nil();
   ```

   The sentinel is a constant, not a config knob — introducing a configurable "default company"
   would quietly reintroduce the synthesized fence ADR-0014 forbids.

**The relay is cross-tenant by design.** `relay::drain_once` selects
`WHERE published_at IS NULL ORDER BY occurred_at, id` with no company scope (in the fenced
deployment it is admitted by the `metaphor_relay` carve-out in the ADR-0011 policy). Messaging's
events therefore drain regardless of tenant context — correct, because the addressed consumer is
a *channel*, and nothing in the drain path interprets `company_id` for them.

**What this does NOT mean.** The sentinel does not make messaging multi-company. It records
"this event stream is platform-scoped." Any future module that both fences by company AND emits
bus-derived channel events needs its own decision memo — that combination does not exist today.

## (b) ADR-0024 interim credential posture: env vars only

**The state of the world.** ADR-0024 decision 3 moves all credential material into a dedicated
**fenced credential store** (`integration_credential` — access-controlled reads, encryption at
rest, rotation/revocation as operations) and bans secrets in settings bags. That store does not
exist yet.

**Decision.** Until the store lands, `backbone-mail`'s configuration references **environment
variables ONLY**:

- **No literal secrets anywhere** — not in `config/*.yml`, not in schema seed data, not in
  migrations, not in job definitions. `config/application.yml` carries env-var *references*
  (e.g. `api_token: ${SMS_API_TOKEN}`); the file is safe to commit and diff.
- Migration to the real store is **a registered debt row** in the increment-2 obligations (it is
  the same row as the credential-store adoption across the integration family — when the store
  ships, the env refs are swapped for store reads and the debt row closes; see port-notes §5).
- Non-secret configuration (endpoints, sender identity, schema names) stays in plain YAML — the
  ADR-0024 split: ICP/config holds only what is not secret.

**The SMS webhook HMAC secret is designed under the same posture.** The `/sms/status` webhook
(ADR-0021 scheme — raw-body HMAC or re-fetch, fail-closed, no state writes before verification)
is an **increment-2** deliverable (SM-B2, port-notes §5.3). Its shared secret is specified NOW as
an env-var reference (`SMS_WEBHOOK_SECRET`, mirroring `SMS_API_TOKEN`) so increment 2 wires it
into the store migration for free rather than inventing a second interim scheme. Until the route
exists, no secret is read and no webhook state is writable.

## References

- `modules/backbone-framework/backbone-outbox/src/outbox.rs` — the unconditional
  `company_id uuid NOT NULL` column; the `#[cfg(feature = "multi_tenant")]` RLS block; the
  `metaphor_relay` carve-out in the policy.
- `modules/backbone-framework/backbone-outbox/src/record.rs` — `OutboxRecord::new(...,
  company_id: Uuid, ...)` (mandatory tenant field — why the sentinel exists).
- `modules/backbone-framework/backbone-outbox/src/relay.rs` — `drain_once` drains cross-tenant.
- `config/application.yml` / `config/application-dev.yml` — the posture applied: `sms.api_token`
  is `${SMS_API_TOKEN}`, `outbox.company_id` documents the nil sentinel.
