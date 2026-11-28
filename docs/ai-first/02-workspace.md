# AI-first 02 — Cargo workspace (no behavior change)

Refs #122, epic #120. Base: `ai-first/01-baseline` @ `193eeebc4136f4f630cc812dcbf9ee958c8dc395`.
Status: mechanical move only; 01 baseline remains the behavior baseline.

## Crate responsibilities

| Crate | Role | Contains |
|-------|------|----------|
| `beacon-api` | Typed upstream DTOs, operation registry transport, allowlisted REST/WS | `client.rs` (`BeaconClient`, `Operation` 54 variants, `ErrorCode`, `GatewayError`) |
| `beacon-domain` | Pure MeshCore semantics, no networking, no MCP | `resources.rs` (canonical URI validation, uuid only) |
| `beacon-workflows` | Resolver/planner/execution orchestration (tool handlers) | `tools.rs`, `packet_tools.rs`, `routing_tools.rs`, `analytics_tools.rs`, `extra_tools.rs`, `composite_tools.rs` + 6 frozen `*-tool-definitions.json` |
| `beacon-mcp` | MCP transport, resources/prompts, production binary | `server.rs`, `config.rs`, `request_logging.rs`, `subscriptions.rs`, `prompts.rs`, `result.rs`, `main.rs` + all integration tests |

One binary (`beacon-mcp`), one image (`/usr/local/bin/beacon-mcp`), same
`--healthcheck`, `/healthz`, `/readyz`, MCP `/` behavior.

## Dependency rule

```text
beacon-mcp → beacon-workflows → beacon-api
beacon-mcp → beacon-domain, beacon-api
beacon-workflows → beacon-api (+ rmcp model for Tool/JsonObject)
beacon-api ↛ beacon-mcp, rmcp transport
beacon-domain ↛ beacon-mcp, rmcp, networking
```

No MCP transport dependency in API or domain; no service boundaries or network
calls between crates (in-process function calls only). `beacon-api` exposes
`GatewayError::new` and `packet timestamp` as `pub` for cross-crate orchestration;
`tool_definition`/`page` stay `pub(crate)` inside workflows.

Enforced by `crates/beacon-mcp/tests/workspace_boundaries.rs`: parses the three
member manifests and asserts `beacon-api`/`beacon-domain` have no `rmcp` or
`beacon-mcp` dependency and no `axum` transport dependency.

## Module-to-crate map

- `rust/src/client.rs` → `crates/beacon-api/src/client.rs`
- `rust/src/resources.rs` → `crates/beacon-domain/src/resources.rs`
- `rust/src/{tools,packet_tools,routing_tools,analytics_tools,extra_tools,composite_tools}.rs` + JSON → `crates/beacon-workflows/src/`
- `rust/src/{server,config,request_logging,subscriptions,prompts,result}.rs` + `main.rs` → `crates/beacon-mcp/src/`
- `rust/tests/*.rs` → `crates/beacon-mcp/tests/` (import/path-only changes)
- `rust/Cargo.toml` + `rust/Cargo.lock` → root `Cargo.toml` + `Cargo.lock`
- `Dockerfile` build context `/app/rust` → `/app` with `crates/`; binary
  `beacon-mcp-rust` → `beacon-mcp` (same install path `/usr/local/bin/beacon-mcp`)
- CI manifest paths → workspace root commands; `AGENTS.md`/`README.md` updated

No tool names, schemas, response shapes, error mapping, limits, cursors,
truncation, subscription, or upstream paths changed. Frozen JSON byte-identical
apart from new location.
