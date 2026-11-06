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

| Original tests (case count)                     | Behaviors and edge cases to preserve                                                                                                                                    | Rust destination                                    | Status  | Stack layer | Verification |
| ----------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------- | --------------------------------------------------- | ------- | ----------- | ------------ |
| `tests/unit/config.test.ts` (2)                 | Defaults, base URL normalization, credential rejection                                                                                                                  | `rust/src/config.rs` tests                          | Pending | 2           | Pending      |
| `tests/unit/time-query.test.ts` (3)             | UTC RFC3339/calendar validation, epoch milliseconds, IATA normalization, encoded query, limit 1–50                                                                      | `rust/src/query.rs` tests                           | Pending | 3–4         | Pending      |
| `tests/unit/pagination-errors-auth.test.ts` (7) | Numeric/opaque/compound cursors, truncation, error codes, Retry-After, redacted internal/5xx errors                                                                     | `rust/src/pagination.rs`, `rust/src/error.rs` tests | Pending | 3–6         | Pending      |
| `tests/unit/adapter-pagination.test.ts` (16)    | Exact ties and numeric fallback, sort and neighbors, multi-IATA routes, streaming limits, array truncation, payload aliases, all cursor forms                           | `rust/tests/adapter.rs`                             | Pending | 4–6         | Pending      |
| `tests/unit/subscriptions.test.ts` (6)          | UUID validation, bounded debounce, ticket race with shutdown, WS hello/ack/event/pong, URL configuration                                                                | `rust/src/subscriptions.rs` tests                   | Pending | 7           | Pending      |
| `tests/contract/client.test.ts` (16)            | Fixed GET paths, URL cap, safe headers, malformed JSON, bounded and incremental reads, cancellation, timeout, retry/429/503/Retry-After, admin rejection, stats timeout | `rust/tests/client.rs`                              | Pending | 3           | Pending      |
| `tests/mcp/http.test.ts` (15)                   | All 23 tool names, schemas, discovery, filters and ID validation, actual tool calls, result/error classification, modern and legacy protocol, health/readiness          | `rust/tests/mcp_http.rs`                            | Pending | 2–6         | Pending      |
| `tests/mcp/subscriptions.test.ts` (6)           | Resource templates and reads, disabled/legacy capability, SSE ack and targeting, two streams, active-stream shutdown                                                    | `rust/tests/subscriptions.rs`                       | Pending | 7           | Pending      |
| `tests/container/run.sh` + `mock-beacon.mjs`    | Mocked HTTP integration, nonroot, readonly FS, capabilities, resource limits, health/readiness, SIGTERM                                                                 | `tests/container/run.sh` + Rust mock                | Pending | 8–9         | Pending      |

Totals: **71 pre-existing `it` cases** (the tool-inventory assertion extends an
existing case in layer 1), plus the container checks. No Rust migration or deletion has been
claimed in this layer.

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
