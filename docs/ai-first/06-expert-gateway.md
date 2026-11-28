# AI-first 06 — Expert gateway (beacon_explore)

Refs #126, epic #120. Base: `ai-first/05-workflows` @ `f8b6cce`.
Status: additive gateway; legacy 55 tools retained; no selectable profiles.

Upstream re-verified 2026-11-28: latest published release still `v2.3.0`
(tag `7370576c…` → `4c391b84…`); handler/type diff vs vendored contract:
no delta adopted.

## Modes

`beacon_explore` (`explore-tool-definitions.json`, draft 2020-12, `allOf`/`if`/`then`
with `const` supported by the extended validator):

- `list {category?, limit?}`: bounded category-filtered discovery over the 53
  registry keys. Categories derived from key prefixes: server, iata, region,
  scope, node, observer, packet, message, channel, route, trace, stats, broker.
  Default 20, max 50, never more than requested. Unknown category → empty list.
- `describe {operation}`: exact `inputSchema`/`outputSchema` from the frozen
  tool JSON (mapped via `tool_for_key`), runnable `example_query`, capabilities
  (cost Normal/Expensive → timeout class, min version, pagination, time window),
  limitations (retention, sparse-vs-zero, segments-vs-routes, hash collisions,
  non-monotonic cursors, exclusivity). Sufficient to build `execute` alone.
- `execute {operation, args}`: validated args → fixed `GET /api/v1/…` via
  `Operation`; raw upstream fields verbatim + `{"data","key"}` envelope.
  Direct execute with valid args needs no prior list/describe.

## Reuse and exclusion

Shared registry (key→Operation→path), validation (unknown keys, `const`/`if`/`then`,
limits, mutually exclusive modes), budgets/cancellation/concurrency
(admitted-step counting + transport single-retry, `Retry-After` honored),
pagination (opaque `base64url` tokens, exact cursor round-trip,
`hasMore:false` vs `truncated:true`), canonical codes, brief text.
No arbitrary URL/method/header/credentials/admin/query plans; path injection
(`..`, `/`, `%`, control, overlong) rejected before network; unknown
operation/mode/args and prohibited targets rejected with zero upstream calls.
`WsTicket` never listable/describable/executable; absent from all schemas,
examples, payloads, logs.

## Reachability

All 53 public reads reachable via `execute` against offline fixtures
(`explore_gateway::all_53_reads_reachable_through_execute`); legacy catalog
unchanged (55 names + gateway = 56 in discovery during this stage).
Final cutover to the 14-tool catalog belongs to PR07.
