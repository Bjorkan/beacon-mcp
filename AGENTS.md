# Beacon MCP Server: maintainer guide

The end-user introduction is in [README.md](README.md). **Beacon MCP Server**
is an unofficial, standalone, read-only Python MCP gateway to public MeshCore
Beacon REST data. It is not affiliated with or endorsed by Beacon or MeshCore.

## Architecture and pinned upstream contract

```text
MCP client -> TLS proxy -> Beacon MCP Server (/) -> Beacon /api/v1 public GETs
                                                -> Beacon /ws (optional listen)
```

The MCP endpoint is public and unauthenticated. No database, persistent cache,
volume, replay queue, admin operation, writable endpoint or arbitrary HTTP
proxy exists. MCP request contexts and stateless transports are owned by the official SDK. Resource notifications
use one process-local WebSocket and broadcast bus per replica; listeners must
stay on that replica, and updates may be lost. The gateway never advertises
MCP Events. The project permanently forbids client authentication and
persistent disk, database or external event storage; Events webhooks and
replay/outbox are therefore out of scope. See the
[non-implementation policy](docs/mcp-events-migration.md).

Pinned upstream API/release: `v2.2.0` at
`265a835e36931a2c39b9b08e7a78837b9cf0e343` (re-cut annotated tag).
Vendored contract: `vendor/beacon-openapi.yaml`; docs commit
`008f988863014c32e673c5c0176ea86db2868ccf`.

**Before reviewing or changing API integration**, identify the latest
published `MeshCore-Beacon/beacon-server` GitHub release, resolve the tag to
its full immutable commit, and compare its handlers and request/response types
with the gateway. Do not use `main` or `dev` as the adoption target unless the
task explicitly requests an unreleased branch. Record differences and update
the pin deliberately; never silently overwrite the vendored document.

Relevant upstream quirks: route search returns _segments_ rather than entire
routes. Route hash matching uses exact stored hop hashes (2, 4 or 6 hex
characters exist); packet hash prefixes permit odd lengths of 4–16 hex
characters. Cross-IATA search can build multi-megabyte arrays in memory, so
the gateway stops reading at its result limit and uses the analytics timeout.
Stats series rounds times to whole hours, has a rolling ~90-day retention
window and reports misleading errors for expired windows. Scope names are
case-sensitive; IATA inputs are uppercased. Node list fields differ from node
details: neighbor IDs belong to requested lists, while detail carries full
neighbor objects and multibyte flags. Packet server timestamps differ from
observer-reported `heardAt`; `firstToLastMs` is the observer-clock span.
Packet time bounds vary by IATA filter; traces have inclusive `until`, while
stats hours and observer comparison use `[since,until)`. Message filters use
`sentAt >= since`. Unknown well-formed node/packet/trace IDs return 404.
Upstream `/info` can return null minimum client versions, regardless of its
Swagger nullability. Upstream 503 `deadline_exceeded` with `Retry-After` maps to
`DEADLINE_EXCEEDED`. Messages' documented `region` and `regionId` filters on the ordinary
`/messages` and `/channels/{id}/messages` reads are deliberately not exposed:
the pinned Go handlers only call `parseIATAs` and ignore both parameters.
`/messages/backfill` does resolve region filters and exposes them.

## Development

The Python package lives in `src/beacon_mcp/`; the complete offline tests live
in `tests/python/`, with language-independent fixtures in `tests/fixtures/`.
Use Python 3.13.14, uv 0.11.29 and the committed `uv.lock`:

```sh
uv sync --locked
uv run --locked ruff check src tests/python
uv run --locked ruff format --check src tests/python
uv run --locked pyright
uv run --locked pytest
uv build --no-sources
tests/container/run.sh
```

Set `BEACON_BASE_URL=https://beacon.example.org`, then run
`uv run --locked beacon-mcp`. The root MCP endpoint
supports modern `server/discover`, `tools/list` and `tools/call` and stateless
legacy initialization back through `2024-11-05`, as supported by official MCP Python SDK v2.3.0.
No custom protocol adapters are maintained. Health routes are `/healthz`
and `/readyz`; they do not probe the upstream server. The binary supports
`--healthcheck` for the Docker and Compose health checks.

Only the fixed public GET `Operation` enum in `src/beacon_mcp/client.py` may contact
upstream; never accept a caller-controlled host, path, credentials, headers or
admin route. URL and body lengths are bounded. One jittered retry is allowed
only for transient network failures or HTTP 502/503/504 **without**
`Retry-After`; rate limits must not be retried immediately. Error messages
from upstream 5xx must be redacted. Never log packet/message bodies, auth
headers, upstream response bodies, stack traces or environment values.

Each of the 51 tools publishes an input/output schema, description and
read-only/idempotent/open-world hints. The checked-in JSON documents in
`src/beacon_mcp/contracts/*-tool-definitions.json` freeze the MCP discovery contract; update
them with a reviewed contract change, not with a build-time download.
Lists default to 20 and reject limits above 50; they never return more than
requested. Numeric, opaque and compound cursors vary per endpoint. Four
upstream-array tools return `hasMore:false` and only set `truncated:true` if
the response was cut short; they have no fake continuation cursor.
Beacon 2.2 adds node-originated hourly activity (`/nodes/{id}/activity`)
over a 1m–720h range; results draw on rollups plus the raw tail, so a
freshly migrated server may initially have sparse historical points while
migrations 062/063 re-roll the last three days. Node-originated observation
pages (`/nodes/{id}/observations`) use an observation-ID cursor but are
ordered by packet first-heard time then observation ID: the IDs are not
globally monotonic across pages. Preserve and reuse the exact nextCursor.

The additional public GET tools expose all 49 documented non-admin GET
operations from the pinned Swagger specification (except the internal
ws-ticket operation, which is used automatically by subscriptions).
All filters the handlers actually support are included in the public tool
schemas; upstream Swagger-documented but ignored ordinary-message region
filters are intentionally excluded. A frozen Swagger parity test tracks these
exceptions.
Large server-side arrays without a cursor are truncated to the MCP limit
with pagination.hasMore=false and an explicit truncated flag.

Beacon application failures use the canonical codes `INVALID_ARGUMENT`, `NOT_FOUND`,
`DEADLINE_EXCEEDED`, `RESPONSE_TOO_LARGE`, `RESOURCE_EXHAUSTED`, `UNAVAILABLE`,
and `INTERNAL`. Success text is a brief confirmation; structured data is
never duplicated in the text block. Low-level SDK request parsing is native
Pydantic; frozen tool arguments use standard JSON Schema validation. Exact
legacy validation text is not maintained; unexpected handler exceptions use
SDK protocol errors.

## Configuration and deployment

`BEACON_BASE_URL` is mandatory (HTTP(S), no credentials/query/fragment).
`HOST` defaults to `0.0.0.0`, `PORT` to `3000`; `LOG_LEVEL` accepts
`fatal|error|warn|info|debug|trace|silent` and defaults to `info`.
At `info`, one structured line is emitted for every HTTP request to the
MCP root (`/`): HTTP method, sanitized MCP method/tool name (when provided
in the `Mcp-Method`/`Mcp-Name` headers), HTTP status and response-header
latency in milliseconds. Health/readiness probes are excluded. Older clients
without MCP headers are reported as `mcp_method=unspecified` rather than
buffering or inspecting their JSON body. Long-lived SSE streams are timed only
until their response headers, not until disconnection. HTTP 200 does not
guarantee tool-level success: an MCP result can carry `isError: true`.
Normal SDK lifecycle chatter is suppressed. Transport diagnostic details are
filtered at every log level to prevent payloads, headers or stack traces in logs.
Never include arguments, messages, packet data, user headers or query strings
in access logs.
`BEACON_TIMEOUT_MS` defaults to 10000 (100–120000),
`BEACON_STATS_TIMEOUT_MS` to 20000 (100–300000), and `SHUTDOWN_GRACE_MS`
to 10000 (100–60000). `BEACON_SUBSCRIPTIONS_ENABLED` defaults to true;
`BEACON_WS_URL` is an optional ws/wss URL ending in `/ws`, derived from the
Beacon origin otherwise. Stream limit and debounce defaults are 64 and
500 ms (ranges 1–1024 and 0–5000).

Subscriptions request a fresh `/api/v1/ws-ticket` before each `/ws`
connection; older servers returning 404 use a ticketless socket. The ticket
and socket must reach the same upstream instance when origin checks are
enabled. The listener validates the hello/subscribe handshake, limits frames
to 128 KiB, reconnects, debounces updates and sends URI-filtered MCP SSE
notifications via the SDK's direct `ListenHandler`. SDK subscription requests
are not rewritten: URI matches are exact and there is no 20-URI cap. Only
canonical, enabled Beacon URI events are published. The SDK bounds MCP request
bodies to 128 KiB, listen streams to the configured count and each stream's
backlog to 1024 events. HTTP methods and cancellation follow SDK defaults,
including handshake-era standalone GET streams; the listen-stream limit does
not cover those legacy GETs. See `docs/python-migration.md` for proxy limits
and deliberate changes. Legacy `resources/subscribe` is unsupported. Clients must
re-read resources and recreate streams after disruptions.

Build a local image only for development/CI verification; production deployment
must **pull a published image** from `ghcr.io/bjorkan/beacon-mcp`, never build on
the target host. Set `BEACON_MCP_IMAGE` to an immutable published SHA/digest
when using `compose.yaml`. The Dockerfile installs the locked Python package for amd64, runs as
UID/GID 1000 with no application writes, and contains no development tools.
The Compose example drops capabilities, sets a read-only rootfs, no-new-
privileges, bounded memory/CPU/PIDs, rotates logs and terminates gracefully.
Terminate TLS and apply any client access policy at the reverse proxy; the
gateway does not trust `X-Forwarded-*`. Do not buffer subscription SSE streams.

The offline tests mock upstream REST/WebSocket behavior. Live checks against
`beacon.meshat.se` must remain small and read-only; do not print message or
packet contents or commit production smoke scripts. Keep stacked PRs buildable; satisfy the complete test, container and CI gates
before merge. PR readiness follows the maintainer's explicit instructions; do not merge
automatically. An upstream outage produces a tool error, not an unhealthy
gateway or a runaway retry loop.
