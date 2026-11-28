# AI-first 01 — Baseline inventory and migration map

Refs #121, epic #120. Base: `main` @ `5af17545640c0d84c1669ee1201557c117e7ccce`.
Status: frozen baseline, no behavior change.

## 1. Tool inventory (55 tools = 53 public REST reads + 2 composites)

Source of truth: `crates/beacon-workflows/src/*-tool-definitions.json`, `tests/fixtures/tool-names.json`,
`Operation` enum in `crates/beacon-api/src/client.rs` (moved to workspace in PR 02; content unchanged) (54 variants = 53 public + internal `WsTicket`).

| # | MCP tool | Operation | Upstream `GET /api/v1/…` | Future operation key |
|---|----------|-----------|---------------------------|----------------------|
| 1 | `beacon_get_server_info` | `ServerInfo` | `info` | `server.info` |
| 2 | `beacon_list_iatas` | `Iatas` | `iatas` | `iata.list` |
| 3 | `beacon_get_iata` | `IataDetail` | `iatas/{iata}` | `iata.get` |
| 4 | `beacon_get_iata_border` | `IataBorder` | `iatas/{iata}/border` | `iata.border` |
| 5 | `beacon_list_regions` | `Regions` | `regions` | `region.list` |
| 6 | `beacon_get_region` | `RegionDetail` | `regions/{regionId}` | `region.get` |
| 7 | `beacon_list_scopes` | `Scopes` | `scopes` | `scope.list` |
| 8 | `beacon_get_scope` | `ScopeDetail` | `scopes/{name}` | `scope.get` |
| 9 | `beacon_search_nodes` | `Nodes` | `nodes` | `node.search` |
| 10 | `beacon_get_node` | `Node` | `nodes/{nodeId}` | `node.get` |
| 11 | `beacon_get_node_activity` | `NodeActivity` | `nodes/{nodeId}/activity` | `node.activity` |
| 12 | `beacon_get_node_neighbors` | `NodeNeighbors` | `nodes/{nodeId}/neighbors` | `node.neighbors` |
| 13 | `beacon_get_node_observations` | `NodeObservations` | `nodes/{nodeId}/observations` | `node.observations` |
| 14 | `beacon_get_node_poller` | `NodePoller` | `nodes/{nodeId}/poller` | `node.poller` |
| 15 | `beacon_get_node_polling` | `NodePolling` | `nodes/{nodeId}/polling` | `node.polling` |
| 16 | `beacon_get_node_telemetry` | `NodeTelemetry` | `nodes/{nodeId}/telemetry` | `node.telemetry` |
| 17 | `beacon_list_latest_node_telemetry` | `NodeLatestTelemetry` | `nodes/telemetry/latest` | `node.telemetry_latest` |
| 18 | `beacon_search_observers` | `Observers` | `observers` | `observer.search` |
| 19 | `beacon_get_observer` | `Observer` | `observers/{observerId}` | `observer.get` |
| 20 | `beacon_get_observer_activity` | `ObserverActivity` | `observers/{observerId}/activity` | `observer.activity` |
| 21 | `beacon_get_observer_adverts` | `ObserverAdverts` | `observers/{observerId}/adverts` | `observer.adverts` |
| 22 | `beacon_get_observer_telemetry` | `ObserverTelemetry` | `observers/{observerId}/telemetry` | `observer.telemetry` |
| 23 | `beacon_list_observer_directory` | `ObserverDirectory` | `observers/directory` | `observer.directory` |
| 24 | `beacon_search_packets` | `Packets` | `packets` | `packet.search` |
| 25 | `beacon_get_packet` | `Packet` | `packets/{packetHash}` | `packet.get` |
| 26 | `beacon_backfill_packets` | `PacketsBackfill` | `packets/backfill` | `packet.backfill` |
| 27 | `beacon_search_messages` | `Messages` | `messages` | `message.search` |
| 28 | `beacon_backfill_messages` | `MessagesBackfill` | `messages/backfill` (query `afterId`) | `message.backfill` |
| 29 | `beacon_list_channels` | `Channels` | `channels` | `channel.list` |
| 30 | `beacon_get_channel` | `ChannelDetail` | `channels/{channelID}` | `channel.get` |
| 31 | `beacon_get_channel_messages` | `ChannelMessages` | `channels/{channelID}/messages` | `channel.messages` |
| 32 | `beacon_list_routes` | `Routes` | `routes` | `route.list` |
| 33 | `beacon_search_routes` | `RouteSearch` | `routes/search` | `route.search` |
| 34 | `beacon_find_cross_iata_routes` | `CrossRoutes` | `routes/cross` | `route.cross` |
| 35 | `beacon_get_route_observations` | `RouteObservations` | `routes/{iata}/{pathKey}/observations` | `route.observations` |
| 36 | `beacon_search_traces` | `Traces` | `traces` | `trace.search` |
| 37 | `beacon_get_trace` | `Trace` | `traces/{tag}` | `trace.get` |
| 38 | `beacon_get_top_advertisers` | `TopAdvertisers` | `stats/top-advertisers` | `stats.top_advertisers` |
| 39 | `beacon_get_network_overview` | `Overview` | `stats/overview` | `stats.overview` |
| 40 | `beacon_get_network_series` | `Series` | `stats/series` | `stats.series` |
| 41 | `beacon_compare_observers` | `ObserverComparison` | `stats/observer-comparison` | `stats.observer_comparison` |
| 42 | `beacon_list_brokers` | `Brokers` | `brokers` | `broker.list` |
| 43 | `beacon_get_clock_drift` | `ClockDrift` | `stats/clock-drift` | `stats.clock_drift` |
| 44 | `beacon_get_node_types` | `NodeTypes` | `stats/node-types` | `stats.node_types` |
| 45 | `beacon_get_observation_stats` | `ObservationStats` | `stats/observations` | `stats.observation_stats` |
| 46 | `beacon_get_path_stats` | `PathStats` | `stats/paths` | `stats.path_stats` |
| 47 | `beacon_get_payload_breakdown` | `PayloadBreakdown` | `stats/payload-breakdown` | `stats.payload_breakdown` |
| 48 | `beacon_get_radio_presets` | `RadioPresets` | `stats/radio-presets` | `stats.radio_presets` |
| 49 | `beacon_get_scope_stats` | `ScopeStats` | `stats/scopes` | `stats.scope_stats` |
| 50 | `beacon_get_signal_stats` | `SignalStats` | `stats/signal` | `stats.signal` |
| 51 | `beacon_get_top_nodes` | `TopNodes` | `stats/top-nodes` | `stats.top_nodes` |
| 52 | `beacon_get_top_observers` | `TopObservers` | `stats/top-observers` | `stats.top_observers` |
| 53 | `beacon_get_top_talkers` | `TopTalkers` | `stats/top-talkers` | `stats.top_talkers` |
| 54 | `beacon_diagnose_node` (composite) | `Node` + `NodeActivity` + `NodeNeighbors` | 3 bounded GETs, no extra upstream | `workflow.diagnose_node` |
| 55 | `beacon_compare_regions` (composite) | `Overview` × 2 | 2 bounded GETs (`stats/overview` per IATA) | `workflow.compare_regions` |
| — | *(internal, no MCP tool)* | `WsTicket` | `ws-ticket` | *(internal only, never in `beacon_explore`)* |

Prohibited surface (no `Operation`, no tool, verified by allowlist in `client.rs::request`):
accounts, auth (`/auth/*`, `/me/*`), personal alerts, Atlas, telemetry requests,
admin (`/admin/*`), writes, arbitrary host/path/credentials/headers.

## 2. Resources (7 templates)

`crates/beacon-domain/src/resources.rs::parse_uri`, advertised in `server.rs::list_resource_templates` (workspace locations since PR 02):

| Template | ID rule |
|----------|---------|
| `beacon://nodes/{id}` | UUID v1–8, RFC4122 variant, canonical lowercase |
| `beacon://observers/{id}` | same UUID rule |
| `beacon://packets/{hash}` | 16 hex, lowercased |
| `beacon://traces/{tag}` | 8 hex, lowercased |
| `beacon://channels/{id}` | u32 1..=i32::MAX |
| `beacon://regions/{id}` | u32 1..=i32::MAX |
| `beacon://iatas/{iata}` | 3 alnum, uppercased |

Path traversal, extra segments, query/fragment, `%`-encoding, wrong lengths are
rejected. Tests: `resources::tests`.

## 3. Prompts (3, stateless)

`crates/beacon-mcp/src/prompts.rs`: `beacon_node_diagnostics` (UUID),
`beacon_region_comparison` (`iataA`/`iataB`, must differ),
`beacon_route_review` (`iata` + 32-hex `pathKey`). No upstream call, no storage.
Newlines/control chars and overlong args rejected.

## 4. Errors, limits, cursors, subscriptions

- Canonical codes: `INVALID_ARGUMENT`, `NOT_FOUND`, `DEADLINE_EXCEEDED`,
  `RESPONSE_TOO_LARGE`, `RESOURCE_EXHAUSTED`, `UNAVAILABLE`, `INTERNAL`
  (`client.rs::ErrorCode` + `result.rs`; note `INTERNAL` is MCP-mapping only,
  not a `client.rs` variant).
- Lists default 20, reject `limit > 50`; never return more than requested.
- 4 upstream-array tools return `hasMore:false` + `truncated:true` only when cut;
  no fake continuation cursor. Others use numeric/opaque/compound cursors
  (`pageCursor`, `cursor`+`cursorId`, `nextCursor.pageCursor`, observation-ID
  cursors that are ordered by packet first-heard time then ID and are not
  globally monotonic).
- Subscriptions: modern `subscriptions/listen` only (revision `2026-07-28`);
  node/observer default, packet/channel traffic opt-in, max 20 URIs per filter,
  debounce/stream-limit env config, ticket via internal `WsTicket` with 404
  ticketless fallback. Legacy `resources/subscribe` is nulled; clients re-read.
