# AI-first 01 — Upstream handler diff (signed no-delta)

Refs #121. Method: vendored `vendor/beacon-openapi.yaml` paths vs
`Operation::endpoint()` in `rust/src/client.rs:86-145`.
Re-resolved 2026-11-28: tag object `7370576c1442ac880fbe7e45f6aaa9d5e7b56403`
→ commit `4c391b849ade3e35884315126d61a468d51b24a6` (published `v2.3.0`).

Vendored public GETs: 54 paths (52 `^  /xxx:` + `/messages` + `/messages backfill`
with space in key). Gateway `Operation` variants: 54 (53 public + internal `WsTicket`).

| Operation | Gateway `api/v1/…` | Vendored path | Match |
|-----------|---------------------|---------------|-------|
| `ServerInfo` | `info` | `/info` | yes |
| `Iatas` | `iatas` | `/iatas` | yes |
| `IataDetail` | `iatas/{iata}` | `/iatas/{iata}` | yes |
| `IataBorder` | `iatas/{iata}/border` | `/iatas/{iata}/border` | yes |
| `Regions` | `regions` | `/regions` | yes |
| `RegionDetail` | `regions/{id}` | `/regions/{regionId}` | yes (param renamed) |
| `Scopes` | `scopes` | `/scopes` | yes |
| `ScopeDetail` | `scopes/{scope}` | `/scopes/{name}` | yes (param renamed) |
| `Nodes` | `nodes` | `/nodes` | yes |
| `Node` | `nodes/{uuid}` | `/nodes/{nodeId}` | yes (param renamed) |
| `NodeActivity` | `nodes/{uuid}/activity` | `/nodes/{nodeId}/activity` | yes |
| `NodeNeighbors` | `nodes/{uuid}/neighbors` | `/nodes/{nodeId}/neighbors` | yes |
| `NodeObservations` | `nodes/{uuid}/observations` | `/nodes/{nodeId}/observations` | yes |
| `NodePoller` | `nodes/{uuid}/poller` | `/nodes/{nodeId}/poller` | yes |
| `NodePolling` | `nodes/{uuid}/polling` | `/nodes/{nodeId}/polling` | yes |
| `NodeTelemetry` | `nodes/{uuid}/telemetry` | `/nodes/{nodeId}/telemetry` | yes |
| `NodeLatestTelemetry` | `nodes/telemetry/latest` | `/nodes/telemetry/latest` | yes |
| `Observers` | `observers` | `/observers` | yes |
| `Observer` | `observers/{uuid}` | `/observers/{observerId}` | yes |
| `ObserverActivity` | `observers/{uuid}/activity` | `/observers/{observerId}/activity` | yes |
| `ObserverAdverts` | `observers/{uuid}/adverts` | `/observers/{observerId}/adverts` | yes |
| `ObserverTelemetry` | `observers/{uuid}/telemetry` | `/observers/{observerId}/telemetry` | yes |
| `ObserverDirectory` | `observers/directory` | `/observers/directory` | yes |
| `Packets` | `packets` | `/packets` | yes |
| `Packet` | `packets/{hash}` | `/packets/{packetHash}` | yes |
| `PacketsBackfill` | `packets/backfill` | `/packets/backfill` | yes |
| `Messages` | `messages` | `/messages` | yes |
| `MessagesBackfill` | `messages/backfill` | `/messages backfill` | yes (vendored key has space) |
| `Channels` | `channels` | `/channels` | yes |
| `ChannelDetail` | `channels/{number}` | `/channels/{channelID}` | yes |
| `ChannelMessages` | `channels/{number}/messages` | `/channels/{channelID}/messages` | yes |
| `Routes` | `routes` | `/routes` | yes |
| `RouteSearch` | `routes/search` | `/routes/search` | yes |
| `CrossRoutes` | `routes/cross` | `/routes/cross` | yes |
| `RouteObservations` | `routes/{iata}/{pathKey}/observations` | `/routes/{iata}/{pathKey}/observations` | yes |
| `Traces` | `traces` | `/traces` | yes |
| `Trace` | `traces/{tag}` | `/traces/{tag}` | yes |
| `TopAdvertisers` | `stats/top-advertisers` | `/stats/top-advertisers` | yes |
| `Overview` | `stats/overview` | `/stats/overview` | yes |
| `Series` | `stats/series` | `/stats/series` | yes |
| `ObserverComparison` | `stats/observer-comparison` | `/stats/observer-comparison` | yes |
| `Brokers` | `brokers` | `/brokers` | yes |
| `ClockDrift` | `stats/clock-drift` | `/stats/clock-drift` | yes |
| `NodeTypes` | `stats/node-types` | `/stats/node-types` | yes |
| `ObservationStats` | `stats/observations` | `/stats/observations` | yes |
| `PathStats` | `stats/paths` | `/stats/paths` | yes |
| `PayloadBreakdown` | `stats/payload-breakdown` | `/stats/payload-breakdown` | yes |
| `RadioPresets` | `stats/radio-presets` | `/stats/radio-presets` | yes |
| `ScopeStats` | `stats/scopes` | `/stats/scopes` | yes |
| `SignalStats` | `stats/signal` | `/stats/signal` | yes |
| `TopNodes` | `stats/top-nodes` | `/stats/top-nodes` | yes |
| `TopObservers` | `stats/top-observers` | `/stats/top-observers` | yes |
| `TopTalkers` | `stats/top-talkers` | `/stats/top-talkers` | yes |
| `WsTicket` (internal only) | `ws-ticket` | `/ws-ticket` | yes (no MCP tool) |

Excluded by design (vendored but never mapped): `/admin/*`, `/auth/*`, `/me/*`
(incl. alerts, Atlas, telemetry requests, sessions, devices). Verified by
`grep ^Operation client.rs` containing no admin/auth/me variants and
`request()` accepting only the `Operation` enum.

**Signed no-delta (PR 01):** all 53 public reads + internal ticket resolve to a
vendored public GET with only param-name differences; no missing, extra, or
renamed upstream handler. Pin stays at `4c391b84…`. Any future published
release requires a deliberate reviewed re-pin.
