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

| Original tests (case count)                     | Behaviors and edge cases to preserve                                                                                                                                    | Rust destination                                                                                              | Status   | Stack layer | Verification                                                    |
| ----------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------- | -------- | ----------- | --------------------------------------------------------------- |
| `tests/unit/config.test.ts` (2)                 | Defaults, base URL normalization, credential rejection                                                                                                                  | `rust/src/config.rs` tests                                                                                    | Migrated | 2           | `cargo test --locked` passed                                    |
| `tests/unit/time-query.test.ts` (3)             | UTC RFC3339/calendar validation, epoch milliseconds, IATA normalization, encoded query, limit 1–50                                                                      | `rust/tests/core_tools.rs` + pending query tests                                                              | Partial  | 3–4         | MCP timestamp/IATA/limit checks pass; standalone parity pending |
| `tests/unit/pagination-errors-auth.test.ts` (7) | Numeric/opaque/compound cursors, truncation, error codes, Retry-After, redacted internal/5xx errors                                                                     | `rust/tests/client.rs`, `rust/tests/core_tools.rs`, `rust/tests/routing_tools.rs`                             | Partial  | 3–6         | Numeric/opaque and compound cursors pass; full parity pending   |
| `tests/unit/adapter-pagination.test.ts` (16)    | Exact ties and numeric fallback, sort and neighbors, multi-IATA routes, streaming limits, array truncation, payload aliases, all cursor forms                           | `rust/tests/core_tools.rs`, `rust/tests/packet_tools.rs`, `rust/tests/routing_tools.rs`                       | Partial  | 4–6         | All cursor forms exercised; exact ties/full parity pending      |
| `tests/unit/subscriptions.test.ts` (6)          | UUID validation, bounded debounce, ticket race with shutdown, WS hello/ack/event/pong, URL configuration                                                                | `rust/src/subscriptions.rs` tests                                                                             | Pending  | 7           | Pending                                                         |
| `tests/contract/client.test.ts` (16)            | Fixed GET paths, URL cap, safe headers, malformed JSON, bounded and incremental reads, cancellation, timeout, retry/429/503/Retry-After, admin rejection, stats timeout | `rust/tests/client.rs`                                                                                        | Partial  | 3           | 8 Rust cases pass; chunk-boundary and full parity still pending |
| `tests/mcp/http.test.ts` (15)                   | All 23 tool names, schemas, discovery, filters and ID validation, actual tool calls, result/error classification, modern and legacy protocol, health/readiness          | `rust/tests/http.rs`, `rust/tests/core_tools.rs`, `rust/tests/packet_tools.rs`, `rust/tests/routing_tools.rs` | Partial  | 2–6         | Health/discovery and 19 MCP calls pass; four tools pending      |
| `tests/mcp/subscriptions.test.ts` (6)           | Resource templates and reads, disabled/legacy capability, SSE ack and targeting, two streams, active-stream shutdown                                                    | `rust/tests/subscriptions.rs`                                                                                 | Pending  | 7           | Pending                                                         |
| `tests/container/run.sh` + `mock-beacon.mjs`    | Mocked HTTP integration, nonroot, readonly FS, capabilities, resource limits, health/readiness, SIGTERM                                                                 | `tests/container/run.sh` + Rust mock                                                                          | Pending  | 8–9         | Pending                                                         |

Totals: **71 pre-existing `it` cases** (the tool-inventory assertion extends an
existing case in layer 1), plus the container checks. No Rust migration or deletion has been
claimed for the outstanding behaviors. The first three tool layers implement and test
nineteen tools; the remaining four analytics tools are deliberately not registered yet.

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
`rust/tests/routing_tools.rs`. Four analytics tools still await migration.

## Required invariants for every intermediate layer

- TypeScript CI and deployment remain in place until Rust has equivalent
  coverage; run both suites once the Rust project exists.
- The MCP endpoint is read-only and only fixed public Beacon GET operations may
  be called; no caller-controlled path, URL, credential or admin endpoint.
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

Each branch must compile and its relevant tests pass before the next branch is
created. All PRs remain draft and are based on the preceding branch, not `main`.
