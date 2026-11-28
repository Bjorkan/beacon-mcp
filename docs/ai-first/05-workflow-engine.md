# AI-first 05 — Workflow engine, resolver, budgets

Refs #125, epic #120. Base: `ai-first/04-domain` @ `dbbab90`.
Status: new engine alongside legacy 55 tools; no catalog change.

Upstream re-verified 2026-11-28: latest published release still `v2.3.0`
(tag `7370576c…` → `4c391b84…`); no delta adopted.

## Resolver

`crates/beacon-workflows/src/resolver.rs`: one shared entry point accepting
name, UUID, full public key, supported prefix, entity type (node vs observer),
area (IATA). Normalizes (IATA upper, hex lower, name trim/lower), resolves
exact/normalized matches deterministically, keeps node/observer distinct
(cross-use `Invalid`), never guesses from recency/truncation/collisions.
Ambiguity → bounded `MAX_CANDIDATES=10` + coverage, never single identity.
Canonical identity (`Resolved::Node/Observer/Key`) shared by all workflows.

## Context and budgets

`context.rs`: immutable start, absolute deadline, `CancellationToken`,
capabilities (`server_version`, `activity_v22`, `polling_v23`), budget counters.
Tiers summary/standard/full: 6/12/20 attempts, 3/4/4 concurrent, 12/20/30s.
Retries, resolution, pagination, fan-out all count via `try_attempt`.
Global anonymous cap: process-wide 16-permit semaphore (`global_permit`).
Byte limits and `Retry-After` preserved (no immediate retry); tuning here,
load verification deferred to #134.

## Planner and envelope

`planner.rs`: deterministic sorted+dedeuped plans, capability gating
(`polling_v23`, `activity_v22` → `Unsupported` with zero upstream calls),
parallel via `buffer_unordered(max_concurrent)`, per-section `Result`
(no `try_join!` total failure), real progress only (completed section keys,
no percentages), cancellation through HTTP/retry/pagination/CPU with permits
released and no further attempts. Stateless: no cross-request memoization;
replica-independence tested.

`envelope.rs`: separates completion (`Complete/Partial/AmbiguousEntity/
NotFound/Unsupported/Unavailable`) vs coverage vs query vs evidence vs
warnings vs limitations vs sources vs deterministic next actions. Brief text
never duplicates structured data; canonical codes only.

## Compatibility

Legacy 55 tools unchanged (baseline fixtures until #127). New engine additive.
`workspace_boundaries` still green (workflows may use `rmcp` model + `tokio-util`;
api/domain stay MCP-free).
