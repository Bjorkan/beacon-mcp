# MCP Events: deliberately out of scope

Beacon MCP is a read-only public gateway with **no application authentication**
and **no persistence outside process RAM**. This is a hard project constraint,
not a missing configuration option.

MCP Events subscriptions, outgoing webhook registrations, callback secrets,
durable event outboxes and polling/replay storage are **not supported and must
not be implemented** under this policy. Persistent delivery and ownership of
subscriptions cannot be safely guaranteed without authentication and storage.

Supported live functionality uses Beacon Server's existing public WebSocket
and MCP `subscriptions/listen`: a bounded process-local broadcast of canonical
resource-update URIs. No payload, subscription, user profile, account,
credential or event history is stored on disk or in another service.
Updates can be lost during restarts or WebSocket disconnections; no delivery
guarantee or replay is advertised.

Beacon `nodeUpdate` and `observerStatus` remain enabled by default;
`packetObservation` and `channelMessage` change hints are opt-in due
to traffic volume. No Beacon Server change is required.

Historical issue: [#17](https://github.com/Bjorkan/beacon-mcp/issues/17).
The earlier speculative migration plan is retired because it conflicts
with the project's non-negotiable security and statelessness rules.
