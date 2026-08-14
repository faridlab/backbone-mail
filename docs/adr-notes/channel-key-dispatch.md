# ADR-note: channel-key dispatch as a domain-side payload envelope

> Status: **owner-approved** (messaging port, increment 1). This memo documents the rationale of a
> decided question — it is not re-deciding. Companion to [ADR-0017](../../../../docs/handbook/adr/0017-bus-maps-onto-outbox-inbox.md)
> ("bus maps onto the existing outbox/inbox"); see also the port register's
> [bus mapping section](../port-notes.md#6-bus--backbone-outbox-mapping-adr-0017).

## The gap ADR-0017 left open

ADR-0017 decision 1 says: "`bus.bus`'s per-channel fan-out becomes outbox events addressed by a
**channel key**." But the outbox record the framework ships has nowhere to put a channel:

```rust
// modules/backbone-framework/backbone-outbox/src/record.rs
pub struct OutboxRecord {
    pub id: Uuid,              // the dedup key
    pub event_type: String,    // e.g. "PaymentSettled"
    pub aggregate_type: String,
    pub aggregate_id: String,
    pub company_id: Uuid,
    pub payload: serde_json::Value,
    ...
}
```

`outbox_events` (created by `backbone_outbox::outbox::migrate`) is exactly
`id, event_type, aggregate_type, aggregate_id, company_id, payload, occurred_at, correlation_id,
causation_id, version, created_at, published_at`. There is no `channel` column, and
`event_type`/`aggregate_*` are nominal typing fields, not addressing fields — a Discuss channel
notification is not a new aggregate.

## Decision

**A domain-side payload envelope.** Every bus-derived event staged by `backbone-mail` carries the
channel inside `payload`, mirroring `bus.bus`'s two-column shape verbatim:

```json
{
  "channel": "res.partner_42",
  "message": { "type": "mail.message/insert", "payload": { ... } }
}
```

- `channel` is the fully-constructed channel key (the port of `bus.bus.channel`, incl. the
  db-scoping the Odoo tuple carried — obsolete under one database, so the bare key remains).
- `message` is the port of `bus.bus.message`: an opaque JSON blob with Odoo's own
  `{"type": ..., "payload": ...}` notification wrapper preserved as-is, so the client contract of
  the 8 bus-listener hosts (`mail.message`, `mail.presence`, `mail.guest`, `discuss.channel`, …)
  translates without rewriting every consumer.
- `event_type` stays a **nominal type** — `"BusNotification"` — not a per-channel value. The
  event type says *what kind of wire event this is*; the channel says *who it is addressed to*.
  Keeping them orthogonal preserves the outbox's ability to route/filter/monitor by type.

Staging is unchanged: `outbox::stage(tx, "messaging", &record)` inside the producer's
transaction; `relay::drain_once` hands the whole record to the consumer, which dispatches on
`payload.channel` exactly as Odoo's websocket runtime dispatched on `bus.bus.channel`.

## Why not a framework column

`backbone-outbox` is **git-tag-pinned by every module** that produces or consumes events. Adding
a `channel` column to `outbox_events` means: a framework release, a migration every deployer must
run, and a repo-wide re-pin churn across all consumers — all to serve one module's addressing
scheme on day one. The envelope keeps the framework untouched and the cost local to the one
module that needs channels. If the shape proves out, one module changes; if a column proves out,
every module re-pins.

## What is preserved from the bus cycle (BUS-B2)

**Confidentiality is channel-key construction, not a constraint** (BUS-B2): in Odoo, record
channels are server-injected in `_build_bus_channel_list` and wire channels are validated
plain-str-only. The port preserves the property **by construction**: channel keys are constructed
server-side at stage time, from server-known identity (partner id, channel uuid, guest token
digest) — never accepted from a client or a consumer. There is no subscriber-writable dispatch
table anywhere in the outbox design, so a consumer cannot grant itself authority by choosing a
channel; the most a malicious consumer could do is fail to act on events addressed elsewhere.

## Revisit trigger

Add the framework column only when a **second consumer module needs SQL-level channel filtering**
(e.g. `WHERE payload->>'channel' = ...` queries showing up in a hot path, or per-channel
retention/backpressure in the relay). At that point the addressing concern has escaped one module
and belongs to the substrate. Until then: envelope.

## References

- `modules/backbone-framework/backbone-outbox/src/record.rs` — `OutboxRecord` (no channel field;
  `payload: serde_json::Value` is where the envelope rides).
- `modules/backbone-framework/backbone-outbox/src/outbox.rs` — `stage()` (idempotent
  `ON CONFLICT (id) DO NOTHING`, staged in the producer's tx) and `migrate()` (the fixed
  `outbox_events` shape above).
- `modules/backbone-framework/backbone-outbox/src/relay.rs` — `drain_once()` (SKIP LOCKED-class
  drain over `WHERE published_at IS NULL`, cross-tenant, at-least-once; consumer-side dispatch
  on `payload.channel` happens in the `publish` closure).
- `docs/odoo/messaging/bus/` — the bus cycle (BUS-M1..M3, BUS-B2/B4/B5).
