# Rust gateway: behavior and release gates

The Rust implementation lives in `rust/` and exposes 23 public read-only MCP
tools. `tests/fixtures/tool-names.json` freezes the name inventory. Complete
tool definitions (input/output schemas, descriptions, hints) are checked in as
`rust/src/*-tool-definitions.json` and compared exactly in Rust tests. The
server has no build-time schema download.

## Test coverage

| Area                        | Rust tests                                                                                  | Covered cases                                                                                                                                                                                          |
| --------------------------- | ------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| Configuration and lifecycle | `rust/src/config.rs`, `rust/tests/http.rs`                                                  | Defaults, URL restrictions, IPv6/hostname binding, logging, startup, health, shutdown grace                                                                                                            |
| Public REST client          | `rust/tests/client.rs`, `rust/tests/parity.rs`                                              | Fixed GET paths and headers, encoded filters, ID and URL limits, bounded JSON, chunked arrays, envelope fallback, 204, cancellations, network/timeouts, retries, 400/404/429/503 and redaction         |
| MCP tools and metadata      | `rust/tests/{core_tools,packet_tools,routing_tools,analytics_tools,parity,input_parity}.rs` | All 23 mock-backed calls; full definitions; 30 frozen negative input results; IATA, timestamps, hashes, payload aliases, location filters, list bounds, numeric/opaque/compound cursors and truncation |
| Resources and notifications | `rust/tests/{resources,live_subscriptions,events_scope}.rs`                                 | URI safety, reads, ticket/legacy fallback, hello/ack gating, reconnection, frame cap, debounce, two SSE streams, limits, shutdown, no unsupported Events                                               |
| Protocol compatibility      | `rust/tests/{http,protocol_parity,live_subscriptions}.rs`                                   | Modern discovery, stateless legacy initialize through 2024-10-07, host/origin handling, legacy resource-subscribe absent                                                                               |
| Image                       | `tests/container/run.sh`, `tests/container/{mock-beacon,smoke}.py`                          | Mock-backed MCP, health/readiness, UID 1000, read-only rootfs, capabilities, no-new-privileges, resource limits, SIGTERM; no language runtime bundled                                                  |

Local read-only live checks against `beacon.meshat.se` exercised all 23 tools,
pagination, resource reads, the production WebSocket handshake and SSE ack.
They do not prove delivery of an actual production resource update. The live
auditing scripts are intentionally **outside the repository**; never print
packet or message bodies.

## Open release gates

Every PR in the stack must build and pass its checks. Keep all PRs Draft until
the Rust-only final image and CI succeed across amd64 and arm64, the mock
container hardening checks pass, and read-only live checks confirm core tools
and subscription handshake. **Input-error parity remains under review:** the
Rust handler validates the frozen schemas and returns unstructured MCP errors
for schema rejections; calendar/window and upstream HTTP 400 errors retain
structured metadata. All 30 frozen negative results match the previous HTTP
handler exactly, including text and metadata. Broader invalid cases and live
subscription delivery still need verification before complete parity can be
claimed. Do not merge automatically. MCP Events issue #17
is an independently gated opt-in follow-on; its current stack layer is a plan,
not an implementation. Do not advertise Events without authenticated durable
webhook delivery. See [the Events plan](mcp-events-migration.md).

The latest comparison was against the published Beacon `v2.1.1` release at
`d0a55dbf6d6f5ed4345bd72c039845f6c947fd16`; review its handlers rather
than relying only on Swagger when changing the integration. Release metadata
and vendored contract are in `vendor/UPSTREAM.md`.
