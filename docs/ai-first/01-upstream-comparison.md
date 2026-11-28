# AI-first 01 — Upstream and vendored contract comparison

Refs #121, epic #120.

## 1. Pin resolution (re-resolved 2026-11-28)

- Latest published release: `MeshCore-Beacon/beacon-server` **`v2.3.0`**,
  published `2026-11-28T16:04:31Z` (`gh release list`, `gh api …/releases/latest`).
- Annotated tag object: `7370576c1442ac880fbe7e45f6aaa9d5e7b56403`
  (`gh api repos/…/git/ref/tags/v2.3.0`).
- Tag resolves to commit **`4c391b849ade3e35884315126d61a468d51b24a6`**
  (`gh api repos/…/tags`, `commit.sha`). Merge PR #359.
- Vendored pin: `vendor/UPSTREAM.md` commit `4c391b84…`, source `docs/swagger.yaml`,
  docs repo `6660c56b2dbae965836556dbbf0f6c9fb1b06640`, contract version `2.3.0`.
- **Result: no delta — pin is current.** No silent overwrite performed.

> Stale-note correction in this PR: `vendor/UPSTREAM.md` said "latest published
> GitHub Release was still v2.2.0" at sync time; as of today v2.3.0 is published
> and the pin matches it. `docs/rust-migration.md` "Open release gates" cited
> comparison against v2.1.1; that history is retained but this PR re-verifies
> against v2.3.0.

## 2. Handler/DTO comparison method (not Swagger-only)

1. Enumerated vendored public paths: `grep -n "^  /" vendor/beacon-openapi.yaml`
   (see §3).
2. Enumerated gateway allowlist: `Operation::endpoint()` in `rust/src/client.rs`
   (54 variants incl. internal `WsTicket`).
3. Compared allowlisted public GETs against vendored public GETs; compared
   excluded surface (`/admin/*`, `/auth/*`, `/me/*`, Atlas, telemetry requests)
   against `Operation` to prove no prohibited route is reachable.
4. Spot-checked DTO-relevant quirks documented in `AGENTS.md` against vendored
   schemas (route segments vs full routes, 2/4/6-char hop hashes, 4–16-char
   packet prefixes incl. odd lengths, scope case-sensitivity, IATA uppercasing,
   neighbor-IDs vs neighbor-objects, `heardAt` vs server times,
   `[since,until)` vs inclusive `until`, `sentAt >= since`, null `/info`
   minimum versions, 503 `deadline_exceeded` + `Retry-After`).

## 3. Public-path parity

Vendored public GETs (64 incl. `/ws-ticket`; `/info`, `/brokers`, `/channels*`,
`/iatas*`, `/messages*`, `/nodes*`, `/observers*`, `/packets*`, `/regions/*`,
`/routes*`, `/scopes*`, `/stats/*`, `/traces*`) are all reachable through the
53 public `Operation` variants. No public vendored GET is missing from the
allowlist; no allowlisted GET targets a non-vendored public path.

Excluded by design (vendored but never mapped): `/admin/*`, `/auth/*`, `/me/*`
(incl. alerts, Atlas, telemetry requests, sessions, devices), matching the
`AGENTS.md` prohibited list. `Operation::WsTicket` (`/ws-ticket`) is internal
only and never exposed as an MCP tool.

## 4. Decision

Keep the vendored pin unchanged. Any future newer published release must be
adopted deliberately with a reviewed contract change, never by silently
replacing `vendor/beacon-openapi.yaml`.
