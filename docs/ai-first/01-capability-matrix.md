# AI-first 01 — SDK / protocol / extension capability matrix

Refs #121, epic #120. Pinned SDK: `rmcp = "=3.10.0"` (`rust/Cargo.toml:14`).
MCP spec reference: `2026-07-28` (verify negotiated revisions before integration work).

## 1. Protocol revisions

| Revision | SDK (`KNOWN_VERSIONS`, oldest first) | Gateway | Evidence |
|----------|--------------------------------------|---------|----------|
| `2024-10-07` | **not** in SDK list | manual stateless fallback in `server.rs::initialize` | `rust/tests/protocol_parity.rs` negotiates legacy |
| `2024-11-05` | known | supported | SDK `model.rs:204-210`, parity tests |
| `2025-03-26` | known | supported | SDK + parity tests |
| `2025-06-18` | known | supported | SDK + parity tests |
| `2025-11-25` | known (`LATEST_WITH_INITIALIZE`) | supported; unknown versions fall back here | `protocol_parity.rs` asserts `1999-01-01` → `2025-11-25` |
| `2026-07-28` | known (`LATEST`, `NO_INITIALIZE`, SEP-2567 per-request `_meta`, no `initialize`) | wire-verified via `server/discover`, `tools/list`, `tools/call`; SSE streams | `server.rs`, `http.rs` tests |

Gateway advertises: tools + resources (+subscribe flag iff live) + prompts.
No `Instructions`, icons, `_meta` discovery extras. Legacy
`resources/subscribe` capability is explicitly nulled (`server.rs:228-230`)
because only modern `subscriptions/listen` is supported.

## 2. Extensions and host features

| Capability | Status | Notes |
|------------|--------|-------|
| Instructions | **absent** | no field in `server.rs`; planned PR 08 |
| Cache hints / versioned discovery | **absent** | planned PR 08; no server-side cache today |
| Resource links / embedded content | **absent** (plain text + structured content only) | planned PR 09 |
| Prompts (3) | **wire-verified** | `prompts.rs`, `resources/prompts` tests |
| Skills extension (beta) | **absent** | planned PR 10; do not advertise |
| MCP Apps | **absent** | planned PR 12; do not advertise |
| Completion (`completion/complete`) | **absent** | planned PR 11; do not advertise |
| MRTR / elicitation (`input_required`) | **absent** | planned PR 11; plain `ambiguous_entity`-style results only |
| Sampling, Roots | **absent** | never advertised |
| Tasks, Events (incl. webhooks/replay) | **absent by policy** | `docs/mcp-events-migration.md` non-implementation; do not advertise |
| Progress notifications | **absent** (bounded fan-out exists internally in composites, no wire progress) | planned PR 05/07 |
| Cancellation | **supported** (HTTP timeout, retry sleeps, streaming; no background continuation) | `client.rs`, `http.rs` tests |
| Subscriptions (`subscriptions/listen`) | **wire-verified** (modern only) | `subscriptions.rs`, `live_subscriptions.rs`; ticket + 404 fallback, debounce, 20-URI filter, traffic opt-in |

Rule: SDK Implements ≠ Host Supports. The matrix above is gateway wire support.
Host (client) support stays **unknown** until tested against named clients;
PR 14 must verify fallbacks (tool-only clients complete all tasks).

## 3. SDK upgrade decision

Stay on `rmcp =3.10.0`. Rationale: pinned toolchain/CI green, `KNOWN_VERSIONS`
covers all required revisions, no missing fix requires an upgrade for the
baseline. Re-evaluate only if a later PR needs an SDK-implemented extension
(Skills/Apps/MRTR) that 3.10.0 lacks; any upgrade is a reviewed contract change
with full gate re-run.
