# Python MCP SDK v2 ownership and verification

The gateway exposes 51 tools (49 REST reads and two composites), seven resource
templates and three prompts. Frozen input/output schemas, descriptions,
annotations and inventory order remain unchanged. Beacon is still pinned to
v2.2.0 at `265a835e36931a2c39b9b08e7a78837b9cf0e343`; the published release/tag
was rechecked during this review. No upstream API adoption is included.

## Official SDK APIs

The latest stable SDK was rechecked as **mcp 2.3.0**; the existing exact pin is
retained. The official documentation and installed release source were reviewed:

- [Low-level Server](https://github.com/modelcontextprotocol/python-sdk/blob/v2.3.0/docs/advanced/low-level-server.md)
- [ASGI composition](https://github.com/modelcontextprotocol/python-sdk/blob/v2.3.0/docs/run/asgi.md)
- [Lifespan](https://github.com/modelcontextprotocol/python-sdk/blob/v2.3.0/docs/handlers/lifespan.md)
- [Subscriptions](https://github.com/modelcontextprotocol/python-sdk/blob/v2.3.0/docs/handlers/subscriptions.md)

`Server` owns MCP dispatch, request parsing through its Pydantic models,
initialization/version negotiation, discovery, result serialization, protocol
errors and resource/prompt methods. It receives frozen `Tool` models and typed
`CallToolResult` values. `get_tool_input_schema` supplies the advertised schema
through the public SDK hook used for header validation.

`streamable_http_app()` owns HTTP methods, transport validation, JSON/SSE
selection, request body admission and cancellation. The generated app is mounted
at `/` in an ordinary Starlette host alongside health routes. The host lifespan
enters the documented `server.session_manager.run()` context; the low-level
`Server(lifespan=...)` hook owns the Beacon HTTP client's and socket task's
startup/cleanup. No SDK route or router lifespan attribute is modified.
Uvicorn runs through its normal public API, without signal/shutdown overrides.

`ListenHandler` is passed directly as `on_subscriptions_listen`. It owns
acknowledgment, subscription IDs, exact event filtering, SSE frames, per-stream
buffers, stream limits, disconnect cleanup and graceful `close()`. Its bus is
`InMemorySubscriptionBus`. MCP Events and legacy `resources/subscribe` are not
registered. No protocol middleware, dispatcher, transport or compatibility
adapter is implemented by this project.

## Application logic that remains

The gateway implements fixed Beacon GET operations, identifier/path safety,
filters, dates, cursors, pagination, bounded streaming, deadlines/retries and
redacted Beacon error metadata. These are application rules, not MCP transport
rules. Low-level Server deliberately does **not** apply advertised JSON Schemas
to arbitrary tool arguments (the official low-level guide documents this).
The standard `jsonschema.Draft202012Validator` applies the frozen schemas;
Pydantic `TypeAdapter` normalizes valid integer-valued numeric arguments for
Beacon handlers. The former hand-written schema checker and Rust-style error
formatter are removed. Beacon calendar/window failures retain application
`INVALID_ARGUMENT` metadata; unexpected handler exceptions go to the SDK's
normal protocol error handling.

The upstream socket still fetches fresh tickets (404 fallback), validates
Beacon's hello/subscribe handshake, handles heartbeat/reconnect and bounds
frames to 128 KiB. Beacon event selection and opt-in packet/channel traffic
happen on the **publication side**. A bounded debounce queue contains only
canonical resource URIs. Resource reads validate Beacon identifiers and fetch
fresh data. None of these components changes MCP subscription parameters.

## Deliberate differences from the earlier migration

- The custom `subscriptions/listen` parameter wrapper is removed. SDK ack
  honors requested kinds/URIs, including unknown or unsupported resources;
  those receive no event unless the application actually publishes one.
- `Live.aliases` and alternate-spelling fan-out are removed. SDK URI matching
  is exact. Subscribe using lowercase UUIDs and packet hashes, and canonical
  decimal channel IDs. A resource read may accept/canonicalize an identifier;
  this does not rewrite the client's subscription.
- There is no per-listen 20-URI cap: SDK 2.3.0 has no public setting for it.
  SDK request bodies are limited to **128 KiB before parsing** to bound retained
  filters. Configured SDK listen streams (default 64, range 1–1024), 1024
  buffered events per stream, 1024 pending canonical debounce URIs, HTTP/WS
  limits and container CPU/memory/PID limits remain. No new service/storage or
  authentication is introduced.
- Generated HTTP routes are untouched. Modern standalone GET/DELETE receive
  the SDK's rejection; handshake-era GET may open the SDK's standalone SSE
  stream. Stateless DELETE follows SDK behavior. `MAX_STREAMS` counts native
  `subscriptions/listen` streams, not legacy standalone GET streams. Reverse
  proxies retain responsibility for deployment-wide connection/access policy.
- Uvicorn drains active requests until its configured grace deadline, then the
  SDK cancels requests and application lifespan cleans up Beacon resources.
  An active SSE stream can end abruptly during SIGTERM; clients must refetch
  and reconnect. CLI signal exit status follows Uvicorn/OS defaults rather
  than a forced zero exit status. SDK `ListenHandler.close()` still provides
  its normal graceful stream completion.
- Exact Rust validation wording is no longer reproduced. Schema errors use
  the standard JSON Schema validator's messages in SDK tool results. Beacon
  input errors use Beacon error metadata; malformed MCP parameters and
  unexpected exceptions use SDK protocol errors. Integer-valued JSON numbers
  follow JSON Schema/Pydantic semantics.
- Official revisions are 2024-11-05, 2025-03-26, 2025-06-18, 2025-11-25 and
  2026-07-28. There is no custom 2024-10-07 support. Envelope metadata,
  capabilities, status codes and stream-limit messages follow SDK v2.

The public unauthenticated proxy deployment keeps the existing explicit
`TransportSecuritySettings` configuration. This is a supported SDK setting,
not a custom Host/Origin implementation; TLS/access policy stays at the proxy.

## Test changes and preserved parity

`tests/fixtures/behavior-baseline.json` remains the unchanged synthetic reference
captured from main `d12b0ba94868e03ad049e14596172d765c056274`. All 51 successful
tool results plus five upstream-error results retain exact data/text parity
(**56 cases**). All 86 reference cases still check normalized upstream requests.
The 30 local-negative cases verify rejection before any GET; they intentionally
stop requiring old validation text/metadata. Frozen tool document hashes and
all 49 public Swagger GET/query mappings are still verified.

Tests deliberately changed for SDK ownership:

- `test_foundation`/`conftest`: standard ASGI lifespan driving using the test-only
  `asgi-lifespan` package, with no router lifespan access.
- `test_runtime`: SDK/Pydantic malformed-param errors, SDK unexpected-exception
  handling, JSON Schema rejection and integer normalization.
- `test_packet_routing`, `test_analytics_composite`, `test_behavior_parity`,
  `test_release_gates`: retain Beacon safety/data checks, remove exact legacy
  validation formatting and the special calendar-error rewrite.
- `test_subscriptions`: direct SDK handler registration, ack/IDs, exact
  canonical matching, unmodified unknown-URI acknowledgment, more than 20
  accepted URIs, publication-side traffic opt-in, stream limit/disconnect
  cleanup, SDK close, native HTTP methods and the 128 KiB request cap.
- `test_lifecycle`: SDK/Uvicorn cancellation may end chunked SSE abruptly and
  return the OS signal status; no custom signal adapter is required.

REST/WebSocket safety, heartbeat/tickets/reconnect, resources/prompts, bounded
transfer, contract hashes, image hardening and rollback remain covered. Checks
use synthetic local data; they do not claim delivery of real production Beacon
notifications. All fourteen PRs remain in native stack 49, unmerged; layer 14
continues to contain only Rust retirement and related cleanup.
