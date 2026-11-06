# Issue #17 in the Rust stack: MCP Events

Tracking issue: [#17](https://github.com/Bjorkan/beacon-mcp/issues/17). This
document is an **implementation plan, not an Events implementation**. The
current public MCP gateway does not advertise Events, accept webhook
registrations, or send callbacks. Keep #17 open until its acceptance criteria
have been verified. This branch follows `rust/live-subscriptions` in GitHub
stack #24; further implementation belongs in small Draft PRs above it.

The first implementation layer now exposes an **internal-only** bounded,
process-local `EventReceiver` from the shared Beacon WebSocket. It emits
individual validated node/observer occurrences (fresh UUID per ingress,
UTC timestamp, canonical resource URI and ID) before resource-hint debounce.
Upstream `lagged`, reconnect/disconnect and slow-consumer overflow are explicit
gap signals. This is **not** a replay log or delivery queue: an occurrence
is lost if no consumer receives it, a process restarts, or Beacon drops it.
No MCP `events/*` methods or capabilities are exposed yet.

## Contract and compatibility

The experimental [Events design sketch][sketch] was reviewed at commit
`d5316be214be2e76e51462069c1d67e00a5a7619` (2026-11-06). Pin that
revision for implementation tests; explicitly reconcile it with the
[ChatGPT Events guide][chatgpt] before shipping. The guide currently uses
`-32015` for callback verification failures; the sketch uses provisional
`-32027`. Confirm actual ChatGPT behavior rather than guessing. ChatGPT
supports webhook delivery in Work/Cloud and dots, not poll or SSE push, and
accepts Events advertised via both legacy `initialize` and modern
`server/discover`. Do not limit compatibility testing to MCP `2026-07-28`.

`rmcp 3.8.0` dispatches unknown JSON-RPC methods through
`ServerHandler::on_custom_request`, but its typed `ServerCapabilities` has no
top-level `events` field. The `extensions` or `experimental` maps are **not**
interchangeable with the `capabilities.events` that ChatGPT currently reads.
The current HTTP gateway returns JSON-RPC `-32601` with HTTP 404 for the
unimplemented `events/subscribe` method; `rust/tests/events_scope.rs`
locks down the absence of accidental advertising in both discovery eras.
Implement and test an explicit extension/transport adapter for both discovery
eras, including `Mcp-Method` validation, or adopt an upstream SDK integration
that supports the exact contract. Never register `events/*` as tools or
advertise Events before those methods are fully operational.

Events are distinct from MCP core `subscriptions/listen`. The latter is a
process-local, debounced, ephemeral **resource change hint** stream. An Event
is an individually identified, timestamped occurrence with filtered delivery;
it must enter a separate bounded flow **before** URI debounce. The shared
Beacon WebSocket may fan out to both consumers, but do not open one upstream
socket per webhook subscription. Keep only `nodeUpdate` and `observerStatus`
as v1 sources, with validated UUID filters and minimal ID/resource-URI payloads.
Do not infer changed fields, or announce packet/message events without a
verified upstream signal and privacy/replay review. Beacon `/ws` provides no
replay: webhook outbox durability after ingestion does not recover occurrences
missed while disconnected. Where no replay exists, expose `cursor: null` and
do not claim lossless delivery. An upstream event has no guaranteed stable ID;
assign an ID once on ingestion and retain it for all retries of that occurrence.

## Additional stacked Draft PRs (after migration prerequisites)

1. **Event ingress and protocol contract.** Add a separate typed, bounded
   occurrence channel and gap tracking, with mock-WS tests for filters,
   duplicates, reconnect and overflow. Add protocol compatibility tests for
   custom `events/*` JSON-RPC routing and top-level capability serialization
   on modern discovery and legacy initialization. Leave Events disabled by
   default; no discovery until end-to-end delivery works. Keep all 23 tools,
   resources, and listen-stream tests passing.
2. **Opt-in principal and storage.** Add independently configured OAuth
   resource-server authentication (or an explicitly restricted, authenticated
   deployment with a real stable principal), scoped to Events registration;
   preserve the public read-only tools. Persist subscriptions, protected
   secrets, TTLs and pending deliveries/outbox across restarts. Select a
   documented single-ingress/worker deployment or shared storage with leader
   election and worker leases for replicas; do not silently duplicate delivery
   from each replica. No anonymous subscribe/unsubscribe. Do not reuse
   `BEACON_SUBSCRIPTIONS_*` as Events configuration.
3. **Webhook P0.** On the same MCP root, implement `events/list`,
   `events/subscribe`, `events/unsubscribe` as protocol methods with validated
   catalog schemas and server-side UUID filters. Key subscriptions by canonical
   `(principal, callback URL, event name, arguments)`, support idempotent
   refresh, rotation, expiration and owner-scoped unsubscribe. Require HTTPS
   and a valid `whsec_` secret, resolve and **pin public destination IPs on
   every connection** (including retries), preserve TLS hostname/SNI, block
   private/reserved destinations and redirects, and challenge-verify before
   activation. Sign exact serialized body bytes with Standard Webhooks;
   bound body size, queue, retries, per-principal rate and shutdown, never log
   secrets or payloads. Only then advertise `capabilities.events` and
   `delivery: ["webhook"]` when auth, storage and workers are available.
4. **Client and operational verification.** Mock integration/security tests
   cover discovery in both eras, authorization/isolation, DNS rebinding,
   redirect/SSRF, callback challenge, signing, filtering, backpressure,
   restart/TTL, retries with the same ID, duplicates and WS gaps. Add a
   repeatable ChatGPT Work/Cloud or dots end-to-end test with a controlled
   callback; never register production callbacks against uncontrolled hosts.
   Update `.env.example`, Compose, CI/container checks, README and AGENTS.
5. **Optional separate modes.** Add `events/stream` SSE push only with
   bounded streams, heartbeats and cancellation, and announce `push` per
   event type only then. Add `events/poll` only when a bounded, queryable log
   provides well-defined cursor validity and `truncated` for gaps across
   restarts/replicas; announce `poll` only then. These are not ChatGPT
   requirements and may be split into linked follow-up issues while #17 stays
   open, as its definition of done permits.

## Release gates

- Default deployment remains stateless, public and read-only; Events must be
  absent from discovery unless explicitly enabled **and** safe auth, storage
  and webhook delivery are actually configured. Outbound callbacks are a
  separate, opt-in trust boundary, not an extension of read-only Beacon GETs.
- Each stacked branch compiles and passes its CI; do not merge automatically.
  The gateway, container checks and CI use the Rust toolchain.
- Live `beacon.meshat.se` checks remain small and read-only: verify upstream
  ingress, never create webhook registrations or print packet/message content.
  Webhook correctness uses local mocks plus an authorized controlled client
  for the ChatGPT end-to-end gate.

[sketch]: https://github.com/modelcontextprotocol/experimental-ext-triggers-events/blob/d5316be214be2e76e51462069c1d67e00a5a7619/docs/design-sketch-proposal.md
[chatgpt]: https://developers.openai.com/plugins/build/mcp-events
