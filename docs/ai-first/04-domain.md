# AI-first 04 — MeshCore domain semantics, metrics, evidence, coverage

Refs #124, epic #120. Base: `ai-first/03-operation-registry` @ `fc99413`.
Status: pure typed semantics in `beacon-domain`; no networking/MCP; additive.

Upstream source of truth: Beacon Server `v2.3.0` @ `4c391b84…` (docs `6660c56b…`),
vendored `vendor/beacon-openapi.yaml`. Re-verified 2026-11-28: latest published
release still `v2.3.0`; no delta adopted.

## Entities and identities

| Concept | Type | Rule |
|---------|------|------|
| Node | `identities::NodeId` (UUID v1–8 RFC4122) | distinct from observer; cross-use is parse/`INVALID_ARGUMENT` error |
| Observer | `identities::ObserverId` (UUID) | distinct from node; same string, different types |
| Pubkey | `identities::PublicKey` (64 hex) | full Ed25519 hex; ADVERT matching is exact |
| Prefix | `identities::HashPrefix` (1–64 hex) | TEXT/PATH/ANON 1-byte prefixes; collisions → `Ambiguous`, never `High` |
| IATA | `geography::Iata` (3 alnum, uppercased) | Beacon partition, not airport; `arn→ARN`; scopes case-sensitive |
| Scope | `geography::Scope` (≤128, no `/\%` control) | `#bc ≠ #BC` preserved |
| Position | lat/lng + `LocationSource` + `possiblyForeign` | foreign never trusted |
| Radio | `radio::RadioConfig` (`freqMhz,bwKhz,sf[/cr]`, sf 7–12) | preset source observer\|node; invalid rejected |

## Metric registry

`telemetry::REGISTRY`: canonical name, unit, source, aggregation, range,
missing rule, limitation. Repeater `lastRssiDb/lastSnrDb` labeled
repeater-measured, not observer-received. Drift signed device-minus-server.
`minFirmwareVersion` is a floor (`semantics::firmware_satisfies`), not exact.
No SOC, compliance, reachability, or health-score derivation (tested absent).

## Polling

`polling::{Assignment, LiveClaim, PollTarget}`: assignment (`poller/standbys/rank`)
vs live claim (`live/receivedAt/slots/state`) vs `requested`. `polled` =
stored reading exists. `effective_free` counts only when live+accepting.
`latest_trusted` = newest trusted or null; `sampled_at` = later of
battery/temperature timestamps or null; never invented.

## Time and coverage

`time::{to_utc_iso, bucket_label, window_contains, retention_allows}`: UTC ISO
+ epoch-ms; bucket `t` is bucket-start; sparse = missing not zero;
`rolledUntil/rawFrom` gap = uncovered-not-quiet; `[since,until)` vs inclusive
`until` vs `sentAt>=since` per operation; retention (activity 1–720h, telemetry
24/72/168/720h, route-obs 30d, series ~90d) enforced as `INVALID_ARGUMENT`.
Rates use `counterSpanSecs`; wall-clock division and duty-cycle claims absent.

## Evidence

`evidence::{EvidenceKind, Conclusion, route_shape, comparison_union}`: advert /
neighbor / relay / poll / observer-status distinct; advert ⇏ bidirectional
link; neighbor ⇏ reachability; relay ⇏ delivery; poll ⇏ live assignment;
`online` ⇏ currently reachable. Segments trimmed to span, never full
end-to-end future routes; TRACE widths excluded; short hashes collidable;
packet counts ≠ observation counts; `onlyA+onlyB+both=total`.

## Same semantics downstream

These types drive workflows/schemas/docs/skills (PR 05–10). No tool renames or
shape changes in PR04.
