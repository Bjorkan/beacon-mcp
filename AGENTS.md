# Technical guide for Beacon MCP Server

This document is for maintainers, contributors, deployment operators, and
coding agents working in this repository. The end-user introduction lives in
[`README.md`](README.md).

## Architecture and invariants

**Beacon MCP Server** is an unofficial, community-built, standalone, read-only
Model Context Protocol gateway for the public MeshCore Beacon REST API. It is
not affiliated with or endorsed by the MeshCore or Beacon projects. It
translates intentionally designed MCP tools into fixed `GET /api/v1/*`
operations; it is not an arbitrary HTTP proxy and cannot access Beacon admin
routes.

Use **Beacon MCP Server** as the product name in prose. Keep `beacon-mcp` for
technical identifiers such as the package, container image, service, repository,
and MCP implementation name.

```text
MCP client -> HTTPS / on dedicated subdomain -> Beacon MCP Server -> Beacon /api/v1
```

The MCP endpoint is intentionally public and has no application-layer
authentication. The service has no database, cache, queue, WebSocket, volume,
or other persistent state. Every MCP request gets a fresh MCP server instance,
so replicas can sit behind a round-robin load balancer.

## Compatibility and pinned contracts

- Node.js 24 LTS, TypeScript, ESM, Fastify
- MCP TypeScript SDK `@modelcontextprotocol/server` 2.3.0
- MCP era `2026-07-28` default, served statelessly; older clients negotiate over
  the same endpoint via legacy `initialize` (SDK defaults: `2025-11-25`,
  `2025-06-18`, `2025-03-26`, `2024-11-05`, `2024-10-07`), also statelessly
- Beacon API version `2.0.3`, also advertised as the MCP server version
- Beacon server release `v2.0.3`, commit `768889243c4f8ecb685751aa1b9af231eab07513`
- Beacon docs commit `9e4298d5897184f1875e3d6be97164f40d208000`

The exact upstream Swagger 2.0 document is committed at
`vendor/beacon-openapi.yaml`. Build-time tooling converts it to OpenAPI 3 and
generates `src/generated/beacon-api.d.ts`; production never downloads a schema.

## Upstream API comparison

Before reviewing or changing the Beacon API integration, identify the latest
published GitHub release of
[`MeshCore-Beacon/beacon-server`](https://github.com/MeshCore-Beacon/beacon-server),
resolve its tag to the full immutable commit, and compare the local API
implementation against that release—not `main` or `dev`. This applies to the
REST client, adapter, MCP tool schemas, vendored OpenAPI document, and generated
declarations. Only use an unreleased branch as the adoption target when the
task explicitly requests it.

Inspect the upstream handlers and request/response types rather than relying
only on its generated API documentation. Record the upstream commit used for
the comparison and call out any differences from the pinned local contract. Do
not silently update the pinned schema or commit; use the OpenAPI update workflow
below when intentionally adopting upstream changes.

Upstream `GET /routes/search` returns route segments (subsets of full routes)
using the same shape as list-routes; a single route may appear with fewer hops
in a segment result. `GET /stats/series` silently rounds `since` and `until` to
whole-hour boundaries and has a rolling ~90-day retention measured from the
current time; windows beyond the retention window are rejected with a
misleading "epoch milliseconds" error.

The last comparison was made against release `v2.0.3`, commit
`768889243c4f8ecb685751aa1b9af231eab07513`, on 2026-10-07. Its Swagger document
is byte-identical to the vendored contract. Compared with `v2.0.2`, it adds the
public `GET /info` endpoint for the server version and optional minimum BEACON
Mobile version; this is exposed as `beacon_get_server_info`. The upstream
handler returns `minAppVersion: null` when unset, although Swagger does not mark
the field nullable, and Swagger's `serverVersion` example remains `2.0.2`; the
runtime value comes from Swagger's current API version (`2.0.3`). The MCP output
schema intentionally reflects the handler behavior. The upstream
handlers currently ignore the documented `region` and `regionId` parameters on
`GET /messages` and `GET /channels/{channelID}/messages`; the corresponding MCP
tools intentionally expose only `iatas` and `scope` until upstream implements
those region filters.

Upstream `GET /routes/search` and `cross` endpoints use `hex.DecodeString` on
the hash parameters; odd-length hexadecimal input (e.g. `e72ba`) reaches the
handler and produces an unhandled panic that surfaces as HTTP 500 instead of 400. The local tool input schemas reject odd-length hex at the Zod layer to
avoid this upstream defect.

`GET /routes/cross` builds the entire result list in memory — for every hop of
every stored route containing the source node it queries cross-IATA neighbors —
so realistic hash pairs routinely produce multi-megabyte responses (one pair
returned 5551 items / 14.5 MiB live) and can take seconds before the first
byte. Upstream exposes no limit parameter, so the MCP layer reads the response
incrementally and stops as soon as the tool's limit is satisfied (see the
streaming contract below); the operation shares the analytics timeout budget.

Further pinned-commit findings verified live against `beacon.meshat.se`:

- Node list items (`api.NodeSummary` projection in `db/nodes.go`) never carry
  `defaultScope`, `supportsMultibytePaths`, or `supportsMultibyteTraces`;
  `neighborIds` is populated only when the list request opts in with
  `?neighbors=true`, which the MCP tool does not expose. The node detail
  endpoint (`api.Node`) carries the multibyte flags and, when set,
  `defaultScope`, `observerId`, and `radio`, but never `neighborIds` (detail
  exposes full `neighbors` objects instead). The output schemas mirror this.
- Time-window bounds are endpoint-specific. Packets: `firstHeardAt >= since`
  and `lastHeardAt < until` on the unfiltered path, but only
  `firstHeardAt <= until` on the IATA-filtered path, so the exact `until`
  boundary is inconsistent upstream. Traces: `firstHeardAt >= since` and
  `firstHeardAt <= until` — an inclusive upper bound. Stats series:
  hours in `[since, until)`. Observer comparison: `heard_at` in
  `[since, until)`. Messages: `sentAt >= since` only. Packet
  `firstToLastMs` is `max(observations[].heardAt) - min(...)` over
  observer-reported clocks, not the server `firstHeardAt`/`lastHeardAt`
  delta; fewer than two observations leave it zero.
- Scope filters compare with SQL equality, so scope names are case-sensitive
  (`#se13` matches, `#SE13` does not) across nodes, packets, traces, and
  messages — unlike IATA inputs, which the MCP layer normalizes to uppercase.
- Exact-ID lookups return HTTP 404 with an upstream `not_found` code for
  well-formed but unknown node, packet, and trace identifiers.

The official web UI lives in
[`MeshCore-Beacon/beacon-web`](https://github.com/MeshCore-Beacon/beacon-web).
Clone it when a question concerns how the official UI displays data (field
semantics, labels, formats) rather than what the REST API returns; for
example it labels packet `firstToLastMs` as "Heard over" next to the
first/last server timestamps. It is an upstream consumer, not part of this
repository, and never blocks a change here.

## Run locally

```sh
cp .env.example .env
# Edit BEACON_BASE_URL.
npm ci --ignore-scripts
npm run openapi:check
npm test
npm run dev
```

Production-style execution always pulls the published image from GitHub
Packages:

```sh
docker compose pull
BEACON_BASE_URL=https://beacon.example.org docker compose up -d
```

The Compose port binds to loopback by default. Put a TLS reverse proxy in front
when exposing it. A regular public deployment routes a dedicated subdomain to
the application, whose MCP endpoint is `/`.

Every successful push to `main` publishes multi-architecture `edge` and Git SHA
images to GitHub Packages. A `vX.Y.Z` tag also publishes `X.Y.Z` and `X.Y`:

```sh
docker pull ghcr.io/bjorkan/beacon-mcp:edge
```

## Container deployment

Deployment and Compose must always use an image published at
`ghcr.io/bjorkan/beacon-mcp`; never build the deployment image on the target
host. Pull and run:

```sh
docker pull ghcr.io/bjorkan/beacon-mcp:2.0.3

docker run --rm -p 127.0.0.1:3000:3000 \
  --stop-timeout 15 \
  -e BEACON_BASE_URL=https://beacon.example.org \
  ghcr.io/bjorkan/beacon-mcp:2.0.3
```

Hardened example:

```sh
docker run --rm --name beacon-mcp \
  --stop-timeout 15 \
  --user 1000:1000 \
  --read-only --tmpfs /tmp:rw,noexec,nosuid,size=16m \
  --cap-drop=ALL --security-opt=no-new-privileges:true \
  --memory=256m --cpus=0.5 --pids-limit=100 \
  -p 127.0.0.1:3000:3000 \
  -e BEACON_BASE_URL=https://beacon.example.org \
  ghcr.io/bjorkan/beacon-mcp:2.0.3
```

The final `node:24-bookworm-slim` stage contains compiled JavaScript,
production dependencies, package metadata, and the Node healthcheck only. The
official base image is pinned to its multi-architecture digest and maintained
by Renovate. BuildKit cache mounts accelerate npm downloads without adding the
cache to an image layer. The runtime runs as the image's `node` user (UID/GID
1000), uses exec-form `CMD`, and writes no application files. Logs go to
stdout/stderr; Compose uses the rotating local log driver. Set
`BEACON_MCP_IMAGE` to an immutable semantic-version tag, Git SHA tag, or image
digest in production; Compose defaults to the matching `2.0.3` release tag. Use
`edge` only to evaluate the latest `main` branch.

All direct dependencies are JavaScript-only and support both targets. CI builds
both platforms, publishes only from `main` or `vX.Y.Z` tags, and attaches SBOM
and provenance attestations. Local image builds are reserved for development,
CI, and `tests/container/run.sh` verification—not deployment.

## Configuration

Configuration is parsed once with Zod during startup. Invalid values stop the
process.

| Variable                  |   Default | Meaning                                                   |
| ------------------------- | --------: | --------------------------------------------------------- |
| `HOST`                    | `0.0.0.0` | Listen address                                            |
| `PORT`                    |    `3000` | Unprivileged listen port                                  |
| `BEACON_BASE_URL`         |  required | Fixed upstream base; credentials/query/fragment forbidden |
| `BEACON_TIMEOUT_MS`       |   `10000` | Normal request timeout, 100–120000 ms                     |
| `BEACON_STATS_TIMEOUT_MS` |   `20000` | Analytics and cross-IATA route timeout, 100–300000 ms     |
| `LOG_LEVEL`               |    `info` | Log level setting                                         |
| `SHUTDOWN_GRACE_MS`       |   `10000` | Bounded shutdown grace period                             |

The upstream response limit is fixed at 5 MiB. MCP is always public and
stateless; these are service invariants rather than deployment settings. The
legacy-era fallback adds no session state: every POST is served by a fresh MCP
instance, and legacy GET/DELETE session operations are answered with 405.

## MCP client and discovery

Point a modern MCP client at the root of a dedicated subdomain, such as
`https://mcp.example.org`. A discovery request is:

```sh
curl -sS https://mcp.example.org \
  -H 'Content-Type: application/json' \
  -H 'Accept: application/json, text/event-stream' \
  -H 'MCP-Protocol-Version: 2026-07-28' \
  -H 'MCP-Method: server/discover' \
  -d '{"jsonrpc":"2.0","id":1,"method":"server/discover","params":{"_meta":{"io.modelcontextprotocol/protocolVersion":"2026-07-28","io.modelcontextprotocol/clientInfo":{"name":"curl","version":"1.0.0"},"io.modelcontextprotocol/clientCapabilities":{}}}}'
```

## Tools

| Area              | Tools                                                                                  |
| ----------------- | -------------------------------------------------------------------------------------- |
| Server            | `beacon_get_server_info`                                                               |
| Geography         | `beacon_list_iatas`, `beacon_list_regions`, `beacon_list_scopes`                       |
| Nodes             | `beacon_search_nodes`, `beacon_get_node`                                               |
| Observers         | `beacon_search_observers`, `beacon_get_observer`, `beacon_get_observer_activity`       |
| Packets           | `beacon_search_packets`, `beacon_get_packet`                                           |
| Messages/channels | `beacon_search_messages`, `beacon_list_channels`, `beacon_get_channel_messages`        |
| Routes            | `beacon_list_routes`, `beacon_search_routes`, `beacon_find_cross_iata_routes`          |
| Traces            | `beacon_search_traces`, `beacon_get_trace`                                             |
| Analytics         | `beacon_get_network_overview`, `beacon_get_network_series`, `beacon_compare_observers` |

Times are strict RFC3339 UTC inputs and become Beacon epoch milliseconds. IATAs
are uppercased. `since` must be earlier than `until`. Scope names are
case-sensitive upstream and are passed through as written. Lists default to 20
and reject limits above 50. A list response is never allowed to grow beyond the
requested bound.

Paginated results use `{ items, pagination: { hasMore, nextCursor? } }`. The
shape of `nextCursor` varies by endpoint:

- Numeric `cursor` for `list_scopes`, `search_nodes`, `search_observers`,
  `search_packets`, `search_messages`, and `get_channel_messages`.
- Compound `{ cursor, cursorId }` for `list_routes`.
- Compound `{ cursor, cursorTag }` for `search_traces`.
- Opaque `pageCursor` for `list_channels` (Beacon's legacy numeric channel
  cursor is intentionally not exposed).

Endpoints backed by upstream arrays with no pagination mechanism
(`list_iatas`, `list_regions`, `search_routes`, `find_cross_iata_routes`)
return `{ hasMore: false, truncated?: true }` with no `nextCursor`; clients
can narrow the filters but cannot request a nonexistent next page. These four
tools also read the upstream array incrementally: the HTTP transfer is
cancelled as soon as the requested limit of complete elements has been parsed,
so the limit bounds the actual work (upstream exposes no limit parameter on
`/routes/search` or `/routes/cross`, and both can return tens of megabytes).
`truncated: true` therefore also appears when the read stopped before the
upstream array terminator was seen — including when the fixed 5 MiB upstream
body cap was hit after at least one complete element; hitting the cap with no
complete element remains a `RESPONSE_TOO_LARGE` error. Time-window semantics
are per endpoint and documented in each tool description; see the upstream
comparison section for the exact upstream bounds.

Tool errors carry a canonical classification in `structuredContent.error.code`
following the gRPC code vocabulary: `INVALID_ARGUMENT` (local validation and
upstream 400), `NOT_FOUND` (upstream 404), `DEADLINE_EXCEEDED` (upstream
timeout), `RESPONSE_TOO_LARGE` (bounded response cap),
`RESOURCE_EXHAUSTED` (upstream 429), `UNAVAILABLE` (other upstream failures),
and `INTERNAL` (unexpected gateway errors). `type` keeps the exception class
name, `status` the upstream HTTP status when present, `upstreamCode` the
upstream error-envelope code when present, and `retryAfter` the 429 header.

Every tool publishes an output schema for its structured result. Cross-field
input constraints are enforced at runtime and documented in plain English in
each tool description; the emitted MCP input JSON Schema uses only core
keywords (`type`, `properties`, `required`, `enum`, `pattern`, numeric bounds)
to stay compatible with older clients that cannot interpret `not`, `allOf`,
`if`, or `dependentRequired`.

Packet hashes are exactly 16 hexadecimal characters, trace tags exactly 8, and
exact node public keys exactly 64; `pubkeyPrefix` remains available for partial
node-key matching. Route hash parameters (`from`, `to`, `fromHash`, `toHash`) are exact-match on
stored hop hashes, not general-purpose prefixes. Accepted lengths are 2 hex
characters (1-byte hash), 4 (2-byte), or 6 (3-byte); only these three exact
lengths currently exist in Beacon. Shorter hashes do not match as prefixes of
longer ones.
`payloadTypeName` uses the same canonical names returned in packet responses,
while `txt_msg`, `grp_txt`, and `anon_req` remain accepted as legacy aliases.
The canonical `reserved` filter covers numeric types 12–14.

In packet details, packet-level `firstHeardAt` and `lastHeardAt` are Beacon
server receive/upsert times. Each `observations[].heardAt` is the timestamp
reported by that observer (with Beacon's drift clamp), so the extrema are not
required to match. `firstToLastMs` is the span between the earliest and latest
observer-reported `heardAt` values — an observer clock span, not the
difference between the two packet-level timestamps — and is absent below two
observations.

## Security model

- The client accepts an operation enum, not a caller-provided URL or path. Only
  compiled public `GET` operations exist; `/api/v1/admin/*` is excluded at the
  generated-type boundary and runtime boundary.
- Every tool is explicitly annotated as read-only, non-destructive, idempotent,
  and open-world. The open-world hint reflects access to a public external
  Beacon deployment; it does not imply write access.
- Callers cannot choose the upstream protocol, host, port, path, headers, or
  credentials.
- The MCP endpoint is public by design. Apply access control, rate limiting, or
  geographic policy at the reverse proxy when a deployment requires it.
- One retry with jitter is used only for network failures and HTTP 502/503/504.
  HTTP 429 is never retried and `Retry-After` is returned to the client.
- Pino logs Fastify request IDs, MCP tool names, HTTP status and latency, plus
  upstream operation, status, retry attempt, latency, and result count. Headers,
  message bodies, packet bodies, response bodies, stack traces, and environment
  values are not logged.
- Upstream bodies are streamed into a bounded buffer and rejected before JSON
  parsing when they exceed the fixed 5 MiB limit.
- The root filesystem may be read-only, all Linux capabilities may be dropped,
  and no Docker socket or privileged namespace is used.

The service does not trust `X-Forwarded-*` headers. A reverse proxy should
terminate TLS, apply any deployment-specific client policy, and forward the
dedicated subdomain to the application without rewriting its path. The
`/healthz` and `/readyz` routes may be exposed separately to trusted monitoring.

## Health, failure, and lifecycle

- `GET /healthz` is a local liveness check and never contacts Beacon.
- `GET /readyz` confirms that configuration loaded and the HTTP service started,
  not upstream reachability.
- Beacon outages produce tool errors while the gateway stays healthy; there is
  no polling or runaway retry loop.
- SIGTERM/SIGINT stops acceptance, waits up to `SHUTDOWN_GRACE_MS`, closes active
  HTTP work, and exits cleanly. The Node process is PID 1.

The Docker `HEALTHCHECK` uses a small Node script rather than adding curl. A
reasonable starting allocation is 128–256 MiB and 0.25–0.5 CPU; tune from
measurements.

## Development and tests

```sh
npm ci --ignore-scripts
npm run openapi:check
npm run format:check
npm run typecheck
npm run lint
npm run test:unit
npm run test:contract
npm run test:mcp
npm run build
tests/container/run.sh
```

Normal tests are offline. Contract tests use a local fetch mock with pinned
Beacon response shapes. MCP tests exercise the real HTTP handler. The container
suite builds the final image and checks health, readiness,
MCP-to-mocked-Beacon connectivity, UID, read-only filesystem, dropped
capabilities, no-new-privileges, resource bounds, and SIGTERM exit. Compose
demonstrates the same hardening.

CI also runs a production dependency audit. Workflow actions are pinned to
full commit SHAs, checkout credentials are not persisted, token permissions
are least-privilege, and jobs have bounded runtimes. Renovate maintains npm,
Docker, and GitHub Actions dependencies using its best-practices preset;
updates must be at least 14 days old before Renovate creates them.
Release systems can additionally scan with Trivy/Grype and sign/verify with
Cosign without changing the application.

## Updating Beacon OpenAPI

Identify the latest published Beacon release, review that release's handlers,
request/response types, Swagger contract, and matching docs changes, then pin
the release tag's full immutable commit:

```sh
npm run openapi:sync -- 0123456789abcdef0123456789abcdef01234567
npm run openapi:generate
npm run openapi:check
npm test
```

Commit the vendor document, generated declarations, and updated
`vendor/UPSTREAM.md` together. Production startup never contacts GitHub.

## Known limitations

- The service is tools-only and read-only.
- Beacon `/ws` is deliberately unsupported; there are no subscriptions,
  persistent sockets, reconnect loops, or REST backfill state.
- No admin endpoints, direct PostgreSQL/Redis/MQTT access, generic proxy
  operation, OAuth server, persistent cache, or local sessions are implemented.
- Readiness intentionally does not report transient Beacon availability.
- `swagger2openapi` is intentionally retained despite upstream abandonment
  (last release 2021). It is the only maintained-enough Swagger 2.0 to
  OpenAPI 3 converter for the build-time `openapi:generate`/`openapi:check`
  scripts, is dev-only and absent from production images, is pinned exactly,
  runs offline against the vendored document, and its output is verified
  byte-identical by `openapi:check`. Renovate's abandoned-dependency report
  for it is informational and requires no action; replace it only if upstream
  Beacon ever publishes OpenAPI 3 directly.
