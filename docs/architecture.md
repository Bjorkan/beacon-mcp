# Reading and extending the gateway

Beacon MCP is a read-only translator between MCP clients and public Beacon
data. It keeps only in-flight data and subscription hints in process memory.
There is no database, authentication layer or persistent cache.

## Source layout

All application code is in `src/beacon_mcp/`, grouped by responsibility:

```text
beacon_mcp/
├── __main__.py            # CLI and container healthcheck
├── app.py                 # Wire components and lifespans
├── asgi.py                # HTTP routes and access logs
├── config.py              # Environment configuration
├── errors.py              # Shared gateway errors
├── validation.py          # Shared input/JSON validation
├── upstream/              # Beacon REST transport and parsing
│   ├── operations.py      # Allowed GET operations
│   ├── client.py          # Safe HTTP requests and retries
│   └── json_stream.py     # Bounded JSON and array decoding
├── mcp/                   # MCP protocol-facing features
│   ├── runtime.py         # Tool registry, schemas, dispatch
│   ├── resources.py       # Resource URI reads
│   ├── prompts.py         # Analysis prompts
│   └── subscriptions.py   # WebSocket-to-MCP updates
├── handlers/              # Tools grouped by Beacon subject
│   ├── core.py, packet.py, routing.py, ...
│   ├── tool_support.py    # Filters and pagination helpers
│   └── extra_support.py   # Shared execution of simple GETs
└── contracts/             # Frozen public MCP tool JSON schemas
```

Start at `__main__.py` to follow startup, `app.py` for wiring,
`mcp/runtime.py` for MCP tool dispatch, `handlers/` for endpoint behavior
or `upstream/client.py` for outbound Beacon requests. `errors.py` and
`validation.py` remain at the package root because all layers use them.
`__init__.py` defines the package version.

The `handlers/` package groups tools by subject. Each module exposes `register()`:

| Handler | Tools |
| --- | --- |
| `core.py` | Server info, locations, nodes, observers and observer activity |
| `packet.py` | Packets, channels and messages, including payload aliases |
| `routing.py` | Saved routes, route segments, cross-IATA search and traces |
| `details.py` | Broker, channel, IATA, region, scope and observer directory reads |
| `activity.py` | Node activity, observations, backfills and route evidence |
| `statistics.py` | Additional statistics endpoints |
| `analytics.py` | Overview, series, advertisers and observer comparisons |
| `composite.py` | Node diagnosis and comparison of two IATA areas |

`OPERATIONS` maps public tool names to Beacon operations. In the additional
endpoint families it also specifies the response shape; `handlers/extra_support.py`
handles their genuinely shared behavior. Composites have named handlers because
they combine several requests. `handlers/__init__.py` is just a package marker.

## Follow one MCP request

1. `__main__.main()` loads `Config`, calls `create_app()` and starts Uvicorn.
2. `asgi.py` mounts the SDK endpoint at `/`. The official SDK handles HTTP,
   protocol negotiation, request parsing and dispatch.
3. `mcp.runtime.create_server()` connects the SDK's tool callback to `Registry.call()`.
4. The registry validates arguments against the checked-in JSON Schema, then
   normalizes integer-valued JSON numbers. It calls the registered handler with
   a fresh argument dictionary.
5. The handler checks Beacon-specific filter combinations, prepares the query
   and calls `BeaconClient.request()` or `array_page()` with an `Operation`.
6. The client fetches Beacon data. The handler transforms pagination if needed.
   The registry returns structured data with a short text confirmation.

For example, find `beacon_search_packets` in `handlers/packet.py`, then follow
`execute()` to `prepare_arguments()`, `query()` and `BeaconClient`.

## HTTP reads and failures

`BeaconClient.url()` accepts only a fixed `Operation` and validates path
identifiers before encoding them. Hosts, arbitrary paths, credentials, headers
and admin operations are never tool arguments. Redirects, environment proxies
and upstream cookies are disabled.

`_get()` applies one deadline to the whole request, including body reading and
the optional retry delay. It calls `read_response()` inside that deadline.
Only transient transport failures or eligible 502/503/504 responses can retry
once; `Retry-After` prevents an immediate HTTP retry. Timeouts do not retry.

`upstream/json_stream.py` enforces byte and nesting limits. For array endpoints it stops
after enough items have arrived. This is why an incomplete array can retain
complete items and report truncation. Array results have no invented cursor.
Paged responses preserve the exact upstream cursor, including opaque text.

`errors.py` defines application failures. `client.classify()` maps HTTP failures
and hides upstream messages for 5xx responses. `runtime.upstream_error()` converts
these failures to tool results. JSON Schema errors become invalid-argument tool
results; malformed MCP requests and unexpected exceptions remain SDK errors.
Resources and prompts translate their own application errors to MCP errors.

## Type boundaries

Raw JSON from `decode_json()` and `BeaconClient.request()` is `object`: the
caller must inspect its shape before accessing fields. `ArrayPage.items` is
`list[object]`; knowing that a response is an array does not establish its item
types. The client's overloads distinguish ordinary reads from bounded array reads
without casting an unknown result to `ArrayPage`.

`Page` and `Pagination` in `handlers/tool_support.py` describe the dictionaries the gateway
builds itself. They are `TypedDict` annotations, so the actual results remain
ordinary dictionaries. Optional cursor and sort values stay opaque to preserve
upstream behavior. Additional endpoint registration uses the three literal
`ResponseShape` values: `array`, `page` and `object`.

Handlers return `object`, and composite handlers keep individually typed tasks
until all related reads finish. `wait_for_tasks()` handles cancellation and
cleanup without erasing the tasks' result types. SDK server/request contexts
and application lifespans use `dict[str, object]` for their shared context.

`Any` remains in schema-driven tool arguments and JSON Schema documents because
their fields vary across tools. The frozen schema validates arguments before a
handler sees them. Test RPC/SSE helpers also leave nested payloads dynamic so
tests can exercise malformed protocol shapes. These boundaries do not weaken
the strict type-checking settings or introduce runtime coercion of Beacon data.

### Explicit variable annotations

The project uses `name: Type = value` for first declarations throughout application
code, tests and scripts, even for obvious types such as `str`, `int` and `bool`.
Annotate a variable once in its scope. Later assignments and assignments to an
already annotated parameter keep ordinary assignment syntax. Use one type covering
all valid branches; do not introduce extra runtime assignments for typing.

Unpacking, loop, context-manager and exception bindings cannot carry an annotation
directly. Declare their names separately before binding them. Existing container
annotations cover item updates such as `headers[key] = value`.

The deliberate exceptions preserve Python and library behavior:

- Enum members retain member assignment syntax; member annotations conflict with
  strict enum typing.
- Comprehension and lambda bindings use their iterable/callback types. Adding
  outer declarations would describe a different scope.
- Attributes on SDK/Starlette/logging objects retain their owner's declarations
  or dynamic attribute API. BasedPyright rejects annotations on expressions such
  as `app.state.server`; no replacement state model is introduced for typing.
- The SDK stream tuple in `test_subscriptions.py` uses its typed context-manager
  return. Naming its exact tuple type would require importing SDK-private stream
  protocols, so that binding keeps SDK inference.

## Add a tool

1. Follow the upstream release review in `AGENTS.md`; inspect the released
   handler and request/response types before adopting an endpoint or filter.
2. Add a fixed public GET `Operation` if needed. Mark its timeout class and,
   for streamed arrays, include it in `ARRAY_OPERATIONS`.
3. Add the reviewed discovery contract under `contracts/` and its name to
   `contracts/tool-order.json`. These files are reviewed source, never downloaded
   at build time. An intentional contract change also needs reviewed updates to
   the frozen fixtures and parity assertions.
4. Register the tool in the appropriate handler module. Reuse shared execution
   for matching response shapes; give endpoint-specific behavior a named handler.
5. Cover queries, invalid inputs, limits, pagination and errors with synthetic
   upstream responses. Run the complete checks below.

## Live resource notifications

`app.py` creates one `Live` bus and one `bridge()` task when enabled. Each
connection obtains a fresh ticket, with ticketless fallback only for a 404.
`connect_once()` opens the socket; `listen_connection()` waits for hello, sends
the subscription and requires the matching acknowledgment before publishing
events. It sends application pings and checks matching pongs. Frame limits,
handshake timeouts and reconnect backoff remain bounded.

`Live.received()` validates enabled events and retains only canonical resource
URIs in its bounded debounce queue. The SDK's `ListenHandler` delivers matching
updates to listening clients. URI matching is exact. The client must read the
resource again to get data; notifications cannot replay missed updates.

The SDK owns MCP transport and listener cleanup. The application lifespan
cancels the bridge, clears pending URI tasks and closes the Beacon HTTP client.
See [SDK behavior](python-migration.md) and [deployment](deployment.md) for proxy
and shutdown details.

## Tests and local checks

`tests/python/helpers.py` provides the mocked ASGI gateway and JSON-RPC helper.
`subscription_helpers.py` provides a real local HTTP server and SSE helpers.
There are no shared pytest fixtures requiring a `conftest.py` currently.
`tests/fixtures/` freezes tool contracts, reference results and upstream requests.
All Python tests use local synthetic data. Run from the repository root:

```sh
uv sync --locked
uv run --locked ruff check src tests/python
uv run --locked ruff format --check src tests/python
uv run --locked basedpyright
uv run --locked pytest
uv build --no-sources
tests/container/run.sh
```

Use the Python and uv versions documented in `AGENTS.md` and the committed lock.
The final command needs Docker and builds a local verification image. Production
deployments must pull a published image instead.
