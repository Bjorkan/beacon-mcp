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
clients: revisions back to `2024-10-07` negotiate automatically, so an MCP
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

## Live resource subscriptions (opt-in)

Operators can enable MCP `subscriptions/listen` (revision `2026-07-28`) by setting
`BEACON_SUBSCRIPTIONS_ENABLED=true`. The gateway exposes the resource templates
`beacon://nodes/{id}` and `beacon://observers/{id}`; `{id}` is a Beacon UUID.
Clients can read them with `resources/read`, and if enabled, open a long-lived
`subscriptions/listen` POST with `notifications.resourceSubscriptions` containing
those URIs. Every matching upstream `nodeUpdate` / `observerStatus` causes a
`notifications/resources/updated` on the corresponding listening stream. The
client should re-read the resource after a notification. For legacy clients
resource reads still work but legacy `resources/subscribe` is **not** available.

A single process-wide WebSocket connection connects to Beacon's existing `/ws`
endpoint (derived from `BEACON_BASE_URL`, or specified using `BEACON_WS_URL`).
No changes to Beacon Server are needed. This uses an ephemeral, process-local
subscription bus; it cannot replay missed events or deliver offline alerts.
Configure `BEACON_SUBSCRIPTIONS_MAX_STREAMS` (default 64) and
`BEACON_SUBSCRIPTIONS_DEBOUNCE_MS` (default 500). Keep the feature disabled if
your MCP client does not support `subscriptions/listen`. Reverse proxies must
**not buffer** SSE responses, must allow long-lived streaming POST responses,
and must allow idle connections beyond the SDK's keepalive interval (15 s).
Always use `wss://` for public Beacon WebSocket connections.
