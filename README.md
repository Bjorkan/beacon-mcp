# Beacon MCP Server

Explore the public MeshCore network from your AI assistant.

**Beacon MCP Server** is an unofficial, community-built MCP server that connects
an MCP-compatible assistant to a MeshCore Beacon. It is not affiliated with or
endorsed by the MeshCore or Beacon projects.

Ask about nodes, observers, packets, routes, traces, channels, and network
activity in plain language—the assistant takes care of finding the right data.

The connection is read-only. It cannot change the network, access Beacon admin
features, or send messages.

## Things you can ask

- “Which observers are online in my region?”
- “Find nodes with this name or public-key prefix.”
- “Show recent public channel messages for this location.”
- “How did network activity change between these two times?”
- “Which routes connect these locations?”
- “Which nodes sent the most direct adverts in my region?”
- “Compare the packets heard by these two observers.”
- “Show the details for this packet or trace.”
- “Show hourly originated packet activity for this node over the last 48 hours.”
- “Show observations originating from this node, then continue with the next cursor.”
- “Which nodes have excessive clock drift, and which radio presets are used?”
- “Compare received signal distributions, path statistics and active talkers.”

Available results depend on the public data provided by the connected Beacon.

## Connect

You need the MCP address from the person or community running the service. A
regular deployment uses the root of its own subdomain, with no additional path:

```text
https://mcp.example.org
```

Add that address as a remote MCP server in an MCP client that supports
Streamable HTTP. Once connected, enable the Beacon tools and start asking
questions—there is no separate Beacon MCP Server interface to learn.

The service speaks the current MCP protocol revision and still accepts older
clients: revisions back to `2024-11-05` negotiate through the official Python MCP SDK v2, so an MCP
client that predates the current revision can usually connect as-is.

If you run into a connection error, check that you are using the dedicated
subdomain without an extra path and that your client supports MCP over
Streamable HTTP. Access rules, if any, are set by the service operator.

## What to expect

- **Safe exploration:** every operation is read-only and limited to Beacon's
  public API.
- **Useful boundaries:** searches are kept to manageable result sizes; add a
  place, time range, node, or channel when you need something more specific.
- **Fresh answers:** information comes from the connected Beacon when you ask,
  so availability and coverage follow that Beacon.
- **No history stored here:** the gateway has no database, cache, accounts, or
  sessions, and it does not log message or packet bodies.

Want to run the service, contribute, or understand the implementation? See
[AGENTS.md](AGENTS.md) for setup, deployment, architecture, security, and test
details.

The gateway currently exposes **51 read-only MCP tools** covering the published
Beacon Server **2.2.0** public REST GET API, including node activity,
node-originated observations, neighbors, observer telemetry and adverts,
broker status, regional metadata, route evidence and additional statistics.
The WebSocket ticket is fetched internally for resource subscriptions and
is not exposed as a user tool. Admin and write operations remain unavailable.
Large unpaged arrays may be truncated to 50 results; narrow the filters
for more precise answers.

The gateway runs on Python **3.13.14** with official MCP SDK **2.3.0**.
For local development install uv **0.11.29**, set `BEACON_BASE_URL`, and run:

```sh
uv sync --locked
uv run --locked pytest
uv run --locked beacon-mcp
```

Production deployments pull a published image from `ghcr.io/bjorkan/beacon-mcp`;
`compose.yaml` requires an explicit `BEACON_MCP_IMAGE` SHA or digest. Never build
on the production host. See [deployment and rollback](docs/deployment.md).
Protocol negotiation, discovery, Streamable HTTP and subscriptions use SDK v2
as shipped. No custom protocol compatibility adapters are provided; revision
`2024-10-07` is outside this SDK's supported revisions. SDK-owned protocol
metadata, HTTP methods, cancellation and protocol error statuses follow SDK v2.
Mounted HTTP and Beacon startup/cleanup use the documented SDK lifespans.

At the default `LOG_LEVEL=info`, the gateway logs one concise line per
incoming request to the MCP root, including the HTTP/MCP method, tool name
when available, HTTP status and response-header latency. Health checks are
not logged; SDK lifecycle INFO noise is filtered out. No request or response
body is logged. Legacy clients without MCP method headers are labelled
`unspecified`, and a 200 HTTP status can still contain an MCP tool error.
Set `LOG_LEVEL=debug` or `trace` for gateway troubleshooting; transport diagnostics
remain filtered to protect request data.

## Live resource subscriptions (on by default)

The gateway enables MCP `subscriptions/listen` (revision `2026-07-28`) by
default; set `BEACON_SUBSCRIPTIONS_ENABLED=false` to turn it off. The gateway
exposes seven public read-only resource templates: `beacon://nodes/{id}` and
`beacon://observers/{id}` (UUID), plus `beacon://packets/{hash}`,
`beacon://channels/{id}`, `beacon://traces/{tag}`, `beacon://iatas/{iata}`
and `beacon://regions/{id}`. Every resource read fetches fresh Beacon data
with no local cache. Clients can read them with
`resources/read`, and open a long-lived `subscriptions/listen` POST with
`notifications.resourceSubscriptions` containing those URIs. Every matching
upstream `nodeUpdate` / `observerStatus` causes a
`notifications/resources/updated` on the corresponding listening stream. The
client should re-read the resource after a notification. The SDK matches URI
strings exactly: subscribe with lowercase UUIDs/packet hashes and canonical
decimal channel IDs. Resource-read normalization does not rewrite subscriptions.
The SDK acknowledges all requested URIs, including unknown ones; Beacon only
publishes enabled canonical node/observer (and opt-in packet/channel) changes.
There is no 20-URI cap; SDK request bodies are limited to 128 KiB, with bounded
listen streams and event buffers. See [SDK behavior and limits](docs/python-migration.md). For legacy clients
resource reads still work but legacy `resources/subscribe` is **not** available.

Packet and channel change hints are **opt-in** because public Beacon instances
may carry high observation volumes. Set `BEACON_SUBSCRIPTIONS_TRAFFIC_ENABLED=true`
(along with subscriptions enabled) to also request Beacon's existing
`packetObservation` and `channelMessage` WebSocket events. These emit only
canonical packet/channel resource-URI updates; message contents and packet
payloads are not retained by the process-local bus. By default the gateway
subscribes only to node and observer changes. Traces, regions and IATAs are
readable resources without live subscriptions. No authentication, persistent
storage, callbacks, webhooks, MCP Events or replay are added. Enabling full
traffic may increase upstream bandwidth and load; leave it off on busy
public Beacon instances unless the operator has reviewed capacity.

A single process-wide WebSocket connection connects to Beacon's existing `/ws`
endpoint (derived from `BEACON_BASE_URL`, or specified using `BEACON_WS_URL`).
No changes to Beacon Server are needed. This uses an ephemeral, process-local
subscription bus; it cannot replay missed events or deliver offline alerts.
Configure `BEACON_SUBSCRIPTIONS_MAX_STREAMS` (default 64) and
`BEACON_SUBSCRIPTIONS_DEBOUNCE_MS` (default 500). Disable the feature if your
MCP client does not support `subscriptions/listen` or the Beacon deployment
should not carry the extra WebSocket. Reverse proxies must
**not buffer** SSE responses, must allow long-lived streaming POST responses,
and must allow idle connections beyond the SDK's keepalive interval (15 s).
Always use `wss://` for public Beacon WebSocket connections.

## Stateless analysis workflows

Two optional composite MCP tools use only bounded public Beacon GET requests:
`beacon_diagnose_node` gathers node details, originated activity and up to 20
neighbors; `beacon_compare_regions` compares independently sampled IATA network
overviews. Native MCP Prompts guide node diagnostics, IATA comparison and saved
route review. These are read-only functions with no authentication, persistent
storage, accounts, webhooks or MCP Events. Only in-flight data is held in RAM.
