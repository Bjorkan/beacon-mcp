# AI-first baseline (PR 01, #121)

Frozen, behavior-preserving baseline. No catalog change.

- [01-baseline-inventory.md](01-baseline-inventory.md) — 55 tools, 7 resources,
  3 prompts, errors/limits/cursors/subscriptions, future operation keys.
- [01-upstream-comparison.md](01-upstream-comparison.md) — pin resolution method.
- [01-upstream-diff.md](01-upstream-diff.md) — per-handler diff, signed no-delta.
- [01-capability-matrix.md](01-capability-matrix.md) — rmcp 3.10.0, protocol
  revisions, extension support, SDK upgrade decision.
- [01-eval-baseline.md](01-eval-baseline.md) — bilingual SV/EN eval seed
  (EVAL-01..06). Executable offline harness: `crates/beacon-mcp/tests/baseline_eval.rs`.

- [02-workspace.md](02-workspace.md) — four-crate split, dependency rule, module map (no behavior change).
