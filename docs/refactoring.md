# Readability review

This behavior-preserving review started from `349adfe` on `main`. The baseline
passed 305 offline tests, Ruff, strict BasedPyright and the distribution build.
The work is split into local commits for naming, shared validation, runtime,
HTTP/JSON separation, handlers, WebSocket processing, tests and documentation.

## Inventory and decisions

The [architecture guide](architecture.md) inventories every application module
and handler family and describes their call relationships. The remaining Python
files were reviewed as follows:

| Files under `tests/python/` | Responsibility |
| --- | --- |
| `test_core.py`, `test_details.py`, `test_activity.py`, `test_statistics.py` | Endpoint families, bounds and continuation behavior |
| `test_packet_routing.py`, `test_analytics_composite.py` | Packet/route filters, analytics and bounded composite requests |
| `test_behavior_parity.py`, `test_openapi.py` | Frozen bytes/results/requests and released Swagger parity |
| `test_client.py`, `test_deadline.py`, `test_json_stream.py` | Safe HTTP reads, retry budget, truncation and JSON boundaries |
| `test_runtime.py`, `test_tool_support.py` | SDK dispatch, schema validation, integer normalization and exact cursors |
| `test_foundation.py`, `test_release_gates.py` | Configuration, health, redaction, cancellation and release invariants |
| `test_resources_prompts.py` | Fresh canonical resource reads and pure analysis prompts |
| `test_websocket.py`, `test_subscriptions.py`, `test_lifecycle.py` | Tickets, heartbeat/reconnect, SDK SSE behavior and process shutdown |
| `helpers.py`, `subscription_helpers.py` | Shared mocked HTTP and real local SSE test support |
| `__init__.py` and `tests/__init__.py` | Test package markers |

`tests/container/mock-beacon.py` and `smoke.py` are the synthetic upstream and
host smoke client for the hardened image. `scripts/sync-openapi.py` explicitly
updates the vendored specification from an immutable commit. They already have
clear, narrow responsibilities and retain their implementations.

The principal changes address nested conditional expressions, repeated
short names, mixed transport/parsing responsibilities, endpoint execution
interleaved with validation, and test modules importing helpers from other
test modules or `conftest.py`.

`Operation` and its array allowlist now use direct enum references in their own
module. Shared errors and identifier validation no longer require importing
the HTTP client. JSON reading has its own module, while HTTP classification
stays with the transport. The request deadline still encloses all decoding and
retry delays. Existing imports from `client.py` used by Python tests remain
available; application modules import definitions from their owning modules.

Endpoint-family registration tables remain small and explicit. Separate
validation/preparation functions clarify the main execution paths without
creating one wrapper for every tool. Composite cleanup still cancels and awaits
sibling tasks on failure; replacing it with another concurrency model could
change exception behavior and was unnecessary.

The WebSocket receive loop still keeps handshake and heartbeat state together.
Ticket acquisition and frame validation are separate named functions; no new
state-machine framework is introduced. The incremental JSON parser still needs
string/escape/depth state across chunks. Those are protocol constraints, not
abstractions to remove. New tests exercise split escape state, nesting limits,
exact cursors, ignored malformed frames, valid pongs and wrong pong IDs.

Configuration, application lifespan and access logging retain their existing
structure: they already have clear boundaries and use supported SDK APIs.
Local configuration names, log level constants, prompt text layout and module
docstrings received small readability improvements. Schema-driven tool
arguments retain `Any` where their JSON Schema boundary warrants it; opaque
inputs and results use `object`. No dependencies or type-checking rules changed.

Ruff's naming and complexity rules were evaluated without changing the required
lint configuration. Naming checks passed. A blanket complexity threshold would
encourage splitting necessary validation/parser branches into more helpers.
The existing rules remain; named control flow and review determine useful
function boundaries.

## Upstream contract review

On 2026-11-14, GitHub's latest published release was
[v2.2.0](https://github.com/MeshCore-Beacon/beacon-server/releases/tag/v2.2.0).
Its annotated tag object `0856dfbf257ea0a12fdcd0c5045644bade9f6f1f` resolves to
`265a835e36931a2c39b9b08e7a78837b9cf0e343`, exactly the existing pin.
The released public handlers and request/response types were compared with
the operation tables, query handling and frozen contracts. No new release or
API adoption is involved, and neither the vendored specification nor the
generated discovery contracts changed.

Existing deliberate differences remain: MCP limits are bounded to 50, arrays
can truncate, node lists differ from details, route search returns segments,
cursor forms vary, and nullable minimum client versions are accepted. Ordinary
message handlers only use IATA filters and ignore documented region filters;
backfill resolves regions. Route evidence carries an opaque window-bound
cursor. The gateway continues to pass those values through unchanged.
The Swagger parity and synthetic baseline tests retain these exceptions.

## Initial refactoring verification

Every implementation stage passed the locked sync, Ruff lint/format checks,
strict BasedPyright, the complete offline suite and the distribution build
before the next stage. Final verification passed 312 tests (the original 305
plus seven regression cases), with zero type errors and warnings. The hardened
container integration script also passed, including read-only filesystem,
non-root execution, resource limits, health, MCP smoke calls and shutdown.

The 51-tool inventory, frozen discovery document hashes, synthetic result and
request parity, vendored specification, dependency lock, lint/type settings and
deployment configuration are unchanged. No production traffic or deployment
was needed for verification. No known behavior regression remains from review.

## Typing follow-up

The follow-up starts from `873e271` and reviews all 51 Python files, including
tests, container helpers and scripts. Every ordinary function now annotates its
parameters and return value (except the conventional `self`/`cls` parameters).
Missing return annotations in constructors and async tests were completed.
Shared operation tables, callback aliases, ASGI byte headers, lifecycle tasks
and externally useful instance attributes have explicit types. That follow-up
retained inference for obvious locals; the explicit annotation pass below
supersedes that convention.

HTTP and JSON decoding now return `object`, array items are `list[object]`, and
handlers return opaque results rather than propagating `Any`. Two overloads
describe the client's ordinary/array read modes. `Page` and `Pagination` are
TypedDicts for the gateway's known result structure, with optional opaque cursor
and sort metadata. `ResponseShape` limits registration to the existing three
shape strings without changing runtime dispatch.

Composite requests retain their individual tasks and retrieve typed results
after `wait_for_tasks()` finishes. The helper preserves the existing gather,
cancellation and sibling cleanup behavior without heterogeneous `list[Any]`
results or new generic machinery. The SDK's generic server/request contexts
carry the actual dictionary lifespan state. Remaining `Any` uses are limited
to varying schema-driven arguments/contracts, SDK keyword forwarding and nested
test RPC/SSE payloads. Existing guarded casts at JSON container boundaries
remain; no new casts, ignores, dependencies or relaxed diagnostics were added.

All 312 existing cases remain, with two additional regressions for complete
composite cancellation cleanup and pass-through of opaque pagination metadata.
The latest published upstream release/tag was rechecked and still resolves to
the same immutable commit recorded above; integration rules and contracts are
unchanged. Ruff naming checks pass. Complexity checks were evaluated again;
remaining branches describe endpoint/protocol constraints and do not justify
additional helpers solely to meet a numerical threshold.

Final follow-up verification passed the locked sync, Ruff lint/format checks,
strict BasedPyright with zero errors and warnings, all 314 offline cases,
distribution builds and hardened container integration. Frozen contracts,
fixtures, upstream pins, dependencies, deployment files and CI gates remain
unchanged. No known behavior regression remains from this follow-up review.

## Explicit annotation pass

Starting from `96d1213`, all 51 tracked Python files were inventoried again for
individual variable bindings. The requested style now explicitly annotates even
obvious locals, constants, intermediate values, callbacks and tasks. Forty-three
files needed edits; the others already had complete declarations or contain only
package markers, registration tables or enum members. The existing ijson stub
also already has a fully annotated public surface.

The final binding audit finds 663 annotated declarations, no uncovered ordinary
bindings and no repeated declarations in one scope. Reassignments reuse existing
parameter/variable types. Unpacking, loops, context managers and exception targets
use separate declarations without changing evaluation order or introducing values.
Closure variables retain their original `nonlocal` behavior.

The 50 `Operation` enum members retain ordinary enum syntax. Comprehension/lambda
bindings, container item updates and attributes owned by other objects retain
their existing typing and scope. The single SDK stream-tuple context binding uses
the SDK's return type without importing its private protocol classes. See the
architecture guide for the precise exceptions and the project's new style rule.

Annotation types were checked against the existing schemas, SDK/library APIs and
all assignment branches. Dynamic JSON test envelopes retain `dict[str, Any]`,
unknown upstream data stays `object`, and schema property maps use explicit nested
dictionary types. No new models, casts, ignores or dependencies were introduced.

As a second behavior check, the before/after ASTs of all 51 files match exactly
after erasing variable annotations and imports. Calls, expressions, control flow,
assignment targets/order and all test assertions are unchanged. Added imports
provide annotation types from the standard library or existing dependencies;
no runtime SDK implementation or protocol adaptation was added.

Final verification passed locked dependency sync, Ruff lint/format checks,
BasedPyright strict with zero errors and warnings, all 314 existing test cases,
distribution builds and hardened container integration. The 51-tool discovery
contracts, fixtures, upstream pin, dependency lock, quality configuration and
deployment files are unchanged.
