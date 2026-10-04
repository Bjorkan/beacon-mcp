# beacon-mcp

`beacon-mcp` is a standalone, read-only Model Context Protocol gateway for the public MeshCore Beacon REST API. It translates intentionally designed MCP tools into fixed `GET /api/v1/*` operations; it is not an arbitrary HTTP proxy and cannot access Beacon admin routes.

```text
MCP client -> Streamable HTTP /mcp -> beacon-mcp -> HTTPS/JSON -> Beacon /api/v1
```

The service has no database, cache, queue, WebSocket, volume, or other persistent state. Every MCP request gets a fresh MCP server instance, so replicas can sit behind a round-robin load balancer.

## Compatibility and pinned contracts

- Node.js 24 LTS, TypeScript, ESM, Fastify
- MCP TypeScript SDK `@modelcontextprotocol/server` 2.3.0
- MCP revision `2026-07-28`; 2025-era stateless compatibility is enabled by default
- Beacon server commit `c7209b70433b8b127a5b1062fdfb17d4a676245c`
- Beacon docs commit `5a60f1e00b3e7c13c416382d4a53f4ea84b7b0f4`

The exact upstream Swagger 2.0 document is committed at `vendor/beacon-openapi.yaml`. Build-time tooling converts it to OpenAPI 3 and generates `src/generated/beacon-api.d.ts`; production never downloads a schema.

## Run locally

```sh
cp .env.example .env
# Edit BEACON_BASE_URL, MCP_AUTH_TOKEN, and allowed hosts.
npm ci --ignore-scripts
npm run openapi:check
npm test
npm run dev
```

Production-style local execution needs only Docker:

```sh
BEACON_BASE_URL=https://beacon.example.org \
MCP_AUTH_TOKEN="$(openssl rand -hex 32)" \
docker compose up --build
```

The Compose port binds to loopback by default. Put a TLS reverse proxy in front when exposing it.

Every successful push to `main` publishes a multi-architecture edge image to GitHub Packages:

```sh
docker pull ghcr.io/bjorkan/beacon-mcp:edge
```

## Container deployment

Build and run:

```sh
docker build \
  --build-arg APP_VERSION=0.1.0 \
  -t beacon-mcp:0.1.0 .

docker run --rm -p 127.0.0.1:3000:3000 \
  -e BEACON_BASE_URL=https://beacon.example.org \
  -e MCP_AUTH_TOKEN=replace-with-a-long-random-token \
  -e MCP_ALLOWED_HOSTS=localhost,127.0.0.1,mcp.example.org \
  beacon-mcp:0.1.0
```

Hardened example:

```sh
docker run --rm --name beacon-mcp \
  --user 1000:1000 \
  --read-only --tmpfs /tmp:rw,noexec,nosuid,size=16m \
  --cap-drop=ALL --security-opt=no-new-privileges:true \
  --memory=256m --cpus=0.5 --pids-limit=100 \
  -p 127.0.0.1:3000:3000 \
  -e BEACON_BASE_URL=https://beacon.example.org \
  -e MCP_AUTH_TOKEN=replace-with-a-long-random-token \
  -e MCP_ALLOWED_HOSTS=localhost,127.0.0.1,mcp.example.org \
  beacon-mcp:0.1.0
```

The final `node:24-bookworm-slim` stage contains compiled JavaScript, production dependencies, package metadata, and the Node healthcheck only. It runs as the image's `node` user (UID/GID 1000), uses exec-form `CMD`, and writes no application files. Logs go to stdout/stderr. Prefer an immutable semantic-version, Git SHA tag, or image digest in production.

Multi-architecture build:

```sh
docker buildx build --platform linux/amd64,linux/arm64 \
  -t registry.example/beacon-mcp:0.1.0 --push .
```

All direct dependencies are JavaScript-only and support both targets. CI builds both platforms for every change and publishes the `edge` manifest after successful pushes to `main`.

## Configuration

Configuration is parsed once with Zod during startup. Invalid values stop the process.

| Variable                    |     Default | Meaning                                                   |
| --------------------------- | ----------: | --------------------------------------------------------- |
| `HOST`                      |   `0.0.0.0` | Listen address                                            |
| `PORT`                      |      `3000` | Unprivileged listen port                                  |
| `BEACON_BASE_URL`           |    required | Fixed upstream base; credentials/query/fragment forbidden |
| `BEACON_TIMEOUT_MS`         |     `10000` | Normal request timeout, 100–120000 ms                     |
| `BEACON_STATS_TIMEOUT_MS`   |     `20000` | Analytics timeout, 100–300000 ms                          |
| `BEACON_MAX_RESPONSE_BYTES` |   `5242880` | Maximum upstream JSON response, 65536–52428800 bytes      |
| `MCP_AUTH_TOKEN`            |       unset | Optional inbound bearer token, minimum 16 characters      |
| `MCP_LEGACY_MODE`           | `stateless` | `stateless` or modern-only `reject`                       |
| `MCP_ALLOWED_HOSTS`         | local hosts | Comma-separated, port-independent Host allowlist          |
| `MCP_ALLOWED_ORIGINS`       | local hosts | Comma-separated browser Origin hostname allowlist         |
| `LOG_LEVEL`                 |      `info` | Log level setting                                         |
| `SHUTDOWN_GRACE_MS`         |     `10000` | Bounded shutdown grace period                             |
| `APP_VERSION`               |     `0.1.0` | Version advertised by MCP                                 |

Do not place secrets in image build arguments or source-controlled `.env` files. Use the runtime environment, Docker secrets injected as environment, or the platform secret manager. When `MCP_AUTH_TOKEN` is unset the service emits a warning and `/mcp` is public. The inbound Authorization header is never forwarded to Beacon.

## MCP client

Point a Streamable HTTP client at `https://mcp.example.org/mcp` and, when configured, send `Authorization: Bearer <token>`. The server supports modern discovery and stateless legacy clients. A minimal request is:

```sh
curl -sS https://mcp.example.org/mcp \
  -H 'Authorization: Bearer replace-with-a-long-random-token' \
  -H 'Content-Type: application/json' \
  -H 'Accept: application/json, text/event-stream' \
  -d '{"jsonrpc":"2.0","id":1,"method":"tools/list"}'
```

## Tools

| Area              | Tools                                                                                  |
| ----------------- | -------------------------------------------------------------------------------------- |
| Geography         | `beacon_list_iatas`, `beacon_list_regions`, `beacon_list_scopes`                       |
| Nodes             | `beacon_search_nodes`, `beacon_get_node`                                               |
| Observers         | `beacon_search_observers`, `beacon_get_observer`, `beacon_get_observer_activity`       |
| Packets           | `beacon_search_packets`, `beacon_get_packet`                                           |
| Messages/channels | `beacon_search_messages`, `beacon_list_channels`, `beacon_get_channel_messages`        |
| Routes            | `beacon_list_routes`, `beacon_search_routes`, `beacon_find_cross_iata_routes`          |
| Traces            | `beacon_search_traces`, `beacon_get_trace`                                             |
| Analytics         | `beacon_get_network_overview`, `beacon_get_network_series`, `beacon_compare_observers` |

Times are strict RFC3339 UTC inputs and become Beacon epoch milliseconds. IATAs are uppercased. `since` must be earlier than `until`. Lists default to 20 and reject limits above 50. Results use `{ items, pagination: { hasMore, nextCursor } }` when Beacon supplies a cursor or the gateway can derive one. For upstream array endpoints that have no pagination mechanism, an over-limit response instead reports `{ hasMore: false, truncated: true }`; clients can narrow the filters but cannot request a nonexistent next page. A list response is never allowed to grow beyond the requested bound.

## Security model

- The client accepts an operation enum, not a caller-provided URL or path. Only compiled public `GET` operations exist; `/api/v1/admin/*` is excluded at the generated-type boundary and runtime boundary.
- Callers cannot choose protocol, host, port, path, headers, or upstream credentials. MCP bearer credentials never leave this process.
- Host and Origin validation use the official MCP Fastify integration. Set the public proxy hostname in `MCP_ALLOWED_HOSTS`; non-browser clients without `Origin` continue to work.
- One retry with jitter is used only for network failures and HTTP 502/503/504. HTTP 429 is never retried and `Retry-After` is returned to the client.
- Pino logs Fastify request IDs, MCP tool names, HTTP status and latency, plus upstream operation, status, retry attempt, latency, and result count. Tokens, headers, message bodies, packet bodies, response bodies, stack traces, and environment values are not logged.
- Upstream bodies are streamed into a bounded buffer and rejected before JSON parsing when they exceed `BEACON_MAX_RESPONSE_BYTES`.
- The root filesystem may be read-only, all Linux capabilities may be dropped, and no Docker socket or privileged namespace is used.

The service does not trust `X-Forwarded-*` headers. A reverse proxy should terminate TLS, replace the inbound `Host` header with an allowed value (or add the public hostname to the allowlist), apply its own client-IP policy, and forward `/mcp`, `/healthz`, and `/readyz`. No direct public exposure is required.

## Health, failure, and lifecycle

- `GET /healthz` is a local liveness check and never contacts Beacon.
- `GET /readyz` confirms that configuration loaded and the HTTP service started, not upstream reachability.
- Beacon outages produce tool errors while the gateway stays healthy; there is no polling or runaway retry loop.
- SIGTERM/SIGINT stops acceptance, waits up to `SHUTDOWN_GRACE_MS`, closes active HTTP work, and exits cleanly. The Node process is PID 1.

The Docker `HEALTHCHECK` uses a small Node script rather than adding curl. A reasonable starting allocation is 128–256 MiB and 0.25–0.5 CPU; tune from measurements.

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

Normal tests are offline. Contract tests use a local fetch mock with pinned Beacon response shapes. MCP tests exercise the real HTTP handler. The container suite builds the final image and checks health, readiness, MCP-to-mocked-Beacon connectivity, UID, read-only filesystem, dropped capabilities, no-new-privileges, resource bounds, and SIGTERM exit. Compose demonstrates the same hardening.

CI also runs a production dependency audit. Release systems can add standard OCI steps such as `docker buildx build --sbom=true --provenance=true`, scan with Trivy/Grype, and sign/verify with Cosign without changing the application.

## Updating Beacon OpenAPI

Review the server and docs changes first, then pin a full immutable commit:

```sh
npm run openapi:sync -- 0123456789abcdef0123456789abcdef01234567
npm run openapi:generate
npm run openapi:check
npm test
```

Commit the vendor document, generated declarations, and updated `vendor/UPSTREAM.md` together. Production startup never contacts GitHub.

## Known limitations

- Version 1 is tools-only and read-only.
- Beacon `/ws` is deliberately unsupported; there are no subscriptions, persistent sockets, reconnect loops, or REST backfill state.
- No admin endpoints, direct PostgreSQL/Redis/MQTT access, generic proxy operation, OAuth server, persistent cache, or local sessions are implemented.
- Readiness intentionally does not report transient Beacon availability.
