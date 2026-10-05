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

If you run into a connection error, check that you are using the dedicated
subdomain without an extra path and that your client supports current MCP
servers. Access rules, if any, are set by the service operator.

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
