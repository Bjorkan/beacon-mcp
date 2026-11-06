# Rust migration: test and behavior ledger

This ledger tracks the existing test cases **without deleting the TypeScript suite**.
The canonical 23 tool names are in `tests/fixtures/tool-names.json`; tests in both
languages must consume the same fixture. A registration-only test is insufficient:
each tool needs a successful call, local input validation and upstream-error
coverage before the TypeScript implementation can be removed.

Each row covers **all `it(...)` cases in that source file**, including the edge
cases listed. The Rust destination and status must be updated as the stack grows;
"pending" means no Rust parity is claimed. Verify case titles in the original
file when assessing a row, not only the number of tests.

| Original tests (case count)                     | Behaviors and edge cases to preserve                                                                                                                                    | Rust destination                                                                                                                               | Status   | Stack layer | Verification                                                                                                                  |
| ----------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------- | -------- | ----------- | ----------------------------------------------------------------------------------------------------------------------------- |
| `tests/unit/config.test.ts` (2)                 | Defaults, base URL normalization, credential rejection                                                                                                                  | `rust/src/config.rs` tests                                                                                                                     | Migrated | 2           | `cargo test --locked` passed                                                                                                  |
| `tests/unit/time-query.test.ts` (3)             | UTC RFC3339/calendar validation, epoch milliseconds, IATA normalization, encoded query, limit 1–50                                                                      | `rust/tests/core_tools.rs` + pending query tests                                                                                               | Partial  | 3–4         | MCP timestamp/IATA/limit checks pass; standalone parity pending                                                               |
| `tests/unit/pagination-errors-auth.test.ts` (7) | Numeric/opaque/compound cursors, truncation, error codes, Retry-After, redacted internal/5xx errors                                                                     | `rust/tests/client.rs`, `rust/tests/core_tools.rs`, `rust/tests/routing_tools.rs`                                                              | Partial  | 3–6         | Numeric/opaque and compound cursors pass; full parity pending                                                                 |
| `tests/unit/adapter-pagination.test.ts` (16)    | Exact ties and numeric fallback, sort and neighbors, multi-IATA routes, streaming limits, array truncation, payload aliases, all cursor forms                           | `rust/tests/core_tools.rs`, `rust/tests/packet_tools.rs`, `rust/tests/routing_tools.rs`                                                        | Partial  | 4–6         | All cursor forms exercised; exact ties/full parity pending                                                                    |
| `tests/unit/subscriptions.test.ts` (6)          | UUID validation, bounded debounce, ticket race with shutdown, WS hello/ack/event/pong, URL configuration                                                                | `rust/src/subscriptions.rs`, `rust/tests/live_subscriptions.rs`                                                                                | Partial  | 7           | Ticket, hello/ack, reconnect, legacy 404, debounce and URI validation pass; ping/pong timeout and shutdown race pending       |
| `tests/contract/client.test.ts` (16)            | Fixed GET paths, URL cap, safe headers, malformed JSON, bounded and incremental reads, cancellation, timeout, retry/429/503/Retry-After, admin rejection, stats timeout | `rust/tests/client.rs`                                                                                                                         | Partial  | 3           | 8 Rust cases pass; chunk-boundary and full parity still pending                                                               |
| `tests/mcp/http.test.ts` (15)                   | All 23 tool names, schemas, discovery, filters and ID validation, actual tool calls, result/error classification, modern and legacy protocol, health/readiness          | `rust/tests/http.rs`, `rust/tests/core_tools.rs`, `rust/tests/packet_tools.rs`, `rust/tests/routing_tools.rs`, `rust/tests/analytics_tools.rs` | Partial  | 2–6         | Fixture verifies 23 names and successful calls; broader parity pending                                                        |
| `tests/mcp/subscriptions.test.ts` (6)           | Resource templates and reads, disabled/legacy capability, SSE ack and targeting, two streams, active-stream shutdown                                                    | `rust/tests/resources.rs`, `rust/tests/live_subscriptions.rs`                                                                                  | Partial  | 7           | Reads, disabled/legacy capabilities, SSE ack and targeting, two streams, limit and shutdown pass; case-by-case parity pending |
| `tests/container/run.sh` + `mock-beacon.mjs`    | Mocked HTTP integration, nonroot, readonly FS, capabilities, resource limits, health/readiness, SIGTERM                                                                 | `tests/container/run.sh` + Rust mock                                                                                                           | Pending  | 8–9         | Pending                                                                                                                       |

Totals: **71 pre-existing `it` cases** (the tool-inventory assertion extends an
existing case in layer 1), plus the container checks. No Rust migration or deletion has been
claimed for the outstanding behaviors. All 23 tools are now registered with
working mock-backed calls, but full input, upstream, and test-case parity
has **not** been demonstrated. No TypeScript tests have been removed.

Latest upstream comparison for the core-tools layer: published `v2.1.1` tag
resolves (through its annotated tag object) to
`d0a55dbf6d6f5ed4345bd72c039845f6c947fd16` on 2026-11-05. Reviewed
`internal/api/handlers/{nodes,observers,scopes}.go` and the corresponding
local `src/beacon/adapter.ts`/tool inputs: observer `type` is text, node `type`
is numeric, nodes accept both cursor forms, and scopes return an unpaginated
array. No difference from the pinned local Beacon 2.1.1 contract was found
for these fields. Full test parity remains pending.

The packet-tools layer also checks the same release's
`internal/api/handlers/{packets,messages,channels}.go`: packet filters and
hash-prefix cursor semantics, `channelID` in the message query (but not the
channel messages path query), and the opaque channel continuation cursor match
the pinned local contract. Five more tools have successful mock-backed MCP
calls in `rust/tests/packet_tools.rs`; routing/traces and full case-by-case
parity are still pending. TypeScript tests remain enabled.

The routing-tools layer compares the same `v2.1.1` release's
`internal/api/handlers/{routes,traces}.go` against local adapter/tool inputs:
route hash lengths (1–4 bytes), up to 100 route IATAs, compound route and trace
cursors, and bounded streaming of non-pageable route searches are preserved.
Five successful MCP calls plus invalid-input and upstream-error tests are in
`rust/tests/routing_tools.rs`.

The analytics layer compares the pinned release's `internal/api/handlers/stats.go`
and `stats_series.go` with `src/beacon/adapter.ts`. Top advertisers uses a
required `flood`/`direct` sort and bounded top-N result; overview is a rolling
summary; series and observer comparison require an explicit time window.
`rust/tests/analytics_tools.rs` uses the original 23-name fixture and includes
successful MCP calls and invalid-input checks for all four analytics tools.
More comprehensive parity and subscriptions are still required before cutover.

The resource layer lists node/observer URI templates and reads canonical
`beacon://` UUID URIs through fixed public GET operations. `rust/tests/resources.rs`
exercises both over MCP HTTP and rejects path, fragment, encoded-segment and
nil-UUID injection. Resource subscriptions were not advertised in that
layer; WebSocket, debouncing and `subscriptions/listen` belong to the next one.

The live-subscriptions layer connects with a fresh `/ws-ticket` for each
WebSocket connection, falls back to an unticketed socket on 404, and maps only
validated node/observer updates to bounded, process-local, URI-filtered MCP
SSE listen streams. Rust tests exercise the handshake, reconnect, legacy 404,
debounce, two streams, stream capacity and shutdown. A short read-only smoke
against `beacon.meshat.se` on 2026-11-05 reached a confirmed WebSocket
subscription and exited cleanly; it did not verify delivery of a production
resource update. The remaining TS behaviors and final Node-free runtime,
container and CI are still pending.

## Added scope: MCP Events (#17)

[Issue #17](https://github.com/Bjorkan/beacon-mcp/issues/17) is included in
this GitHub stack as an **opt-in, separately gated follow-on feature**, not as
part of the existing resource subscription implementation or the 23 read-only
tools. See [the Events migration plan](mcp-events-migration.md) for the pinned
experimental contract, SDK incompatibility, authenticated and persistent
webhook prerequisites, phased Draft PRs, compatibility tests and honest
no-replay semantics. This planning layer does **not** implement or advertise
Events. The issue remains open; poll/push may remain linked follow-ups.

## Required invariants for every intermediate layer

- TypeScript CI and deployment remain in place until Rust has equivalent
  coverage; run both suites once the Rust project exists.
- The MCP endpoint is read-only and only fixed public Beacon GET operations may
  be called by existing tools; no caller-controlled upstream path, URL,
  credential or admin endpoint. Opt-in Events webhook delivery introduces a
  separately authenticated, persistent outbound trust boundary; it must not
  change the default public read-only behavior.
- Clamp every result to its requested bound; preserve opaque, numeric and
  compound cursors, and cancel oversized or no-longer-needed upstream reads.
- Keep resource URIs canonical and restrict subscriptions to the requested URI.
- Never log packet/message bodies; continue bounded timeouts, error envelopes,
  WebSocket reconnection, ticket freshness and bounded shutdown.
- The final build, CI and runtime must not use Node/npm/JavaScript, including
  schema generation and container mocks. Until that layer, retain the old CI.

## Intended stack

1. Behavior inventory and shared tool fixture (this layer).
2. Rust workspace, configuration and HTTP/MCP foundation; dual CI.
3. Fixed read-only upstream client and contract tests.
4. Geography, nodes and observers.
5. Packets, channels and messages.
6. Routes, traces and analytics (complete 23-tool parity).
7. Resources, WebSocket and subscription streams.
8. Container, multiarchitecture CI and security checks.
9. Remove TypeScript/npm, replace remaining tooling, complete parity ledger.
10. Issue #17 Events follow-on layers: typed WS ingress and protocol contract,
    authenticated durable webhooks and controlled ChatGPT verification, then
    optional push/poll. These may interleave with layers 8–9 only while every
    intermediate PR stays green and Events remain disabled until safe.

Each branch must compile and its relevant tests pass before the next branch is
created. All PRs remain draft and are based on the preceding branch, not `main`.
