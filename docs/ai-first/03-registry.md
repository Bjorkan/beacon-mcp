# AI-first 03 — Operation registry, schemas and continuation tokens

Refs #123, epic #120. Base: `ai-first/02-workspace` @ `7285c5d`.
Status: centralizes the 53-read allowlist; preserves outward behavior.

## Registry

Source: `crates/beacon-api/src/registry.rs` (`REGISTRY`, 53 entries).
Each entry: stable key (from `01-baseline-inventory.md`), `Operation`,
`GET /api/v1/…` path, cost class (`Normal` vs `Expensive` → `BEACON_TIMEOUT_MS`
vs `BEACON_STATS_TIMEOUT_MS`), minimum version (pinned `v2.3.0`; activity
`>=2.2`, polling/poller/telemetry-latest `>=2.3`), pagination/limit semantics
(default 20, reject `>50`, never more than requested), time-window semantics
(stats half-open `[since,until)`, traces inclusive `until`, messages
`sentAt >= since`, activity `1m–720h`), example query, error recovery
(canonical codes + single jittered retry on 502/503/504 without `Retry-After`,
no immediate retry on 429/503 with `Retry-After`, 5xx redacted).

`WsTicket` stays internal-only (`WSTICKET_INTERNAL_NOTE`); absent from all
tool listings and from `beacon_explore`-style operation lists.

## Schemas

Published contracts: `crates/beacon-workflows/src/*-tool-definitions.json`
(draft 2020-12, `$schema` declared). Generated from typed Rust structures in
that validation is performed by `crates/beacon-mcp/src/server.rs`
(`validate_field_with_root`), which is 2020-12 compatible for the published
keywords: `$ref`/`$defs`, `oneOf`/`anyOf`/`allOf`, nested objects/arrays,
`type` arrays, `format` (uuid asserted via `Uuid::parse_str`, others pass),
`if`/`then`/`else`, plus original flat bounds. Unknown input keys rejected;
unknown upstream output fields retained raw.

Generation method: contracts are checked in for review; `registry_schema_tokens`
tests fail on unchecked drift (keys/paths/$schema) and on upstream mismatch
(allowlist vs vendored `vendor/beacon-openapi.yaml`). No build-time network
fetch. Flattening (`$defs` inlining) applied only with documented
before/after equivalence tests; default keeps standard `$ref`.

## Continuation tokens

Source: `crates/beacon-api/src/continuation.rs`. Stateless opaque
`base64url(JSON)` with `{v, op, cursor, pageCursor, cursorId, filters, sort,
order, since, until}`; `MAX_TOKEN_BYTES = 2048`; `TOKEN_VERSION = 1`.
Rejects tampered/oversized/cross-operation/cross-filter tokens with
`INVALID_ARGUMENT`; never changes host/path/method/headers; no network fetch.
Pageable-exhausted (`hasMore:false`, no cursor) vs nonpageable-truncated
(`hasMore:false` + `truncated:true`, no cursor, no total) preserved exactly;
exact upstream cursors (incl. non-monotonic observation IDs) round-trip.
Same token + same filters work on another replica when upstream is unchanged.

## Compatibility

Old outward behavior preserved: 55 names/shapes, limits, canonical codes,
redaction, 7 resources, 3 prompts, subscriptions. New validator accepts all
previously valid inputs (frozen `invalid-tool-cases.json` still
`INVALID_ARGUMENT` with identical text) and additionally enforces the newly
supported keywords. Tokens are additive; raw cursors keep working.
