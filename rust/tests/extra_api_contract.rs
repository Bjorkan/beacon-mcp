use beacon_mcp_rust::{client::BeaconClient, extra_tools::ExtraTools};
use serde_json::Value;
use std::time::Duration;

/// Fail if a new public GET parameter has not been added to MCP discovery.
/// Path arguments use the MCP `id` convention; synthetic local limits are okay.
#[test]
fn additional_tool_queries_cover_every_pinned_upstream_parameter() {
    let swagger = include_str!("../../vendor/beacon-openapi.yaml");
    let client = BeaconClient::new(
        "http://127.0.0.1:9/".parse().unwrap(),
        Duration::from_secs(1),
        Duration::from_secs(1),
    );
    let tools = ExtraTools::new(client);
    let routes = [
        ("beacon_list_brokers", "/brokers"),
        ("beacon_get_channel", "/channels/{channelID}"),
        ("beacon_get_iata", "/iatas/{iata}"),
        ("beacon_get_iata_border", "/iatas/{iata}/border"),
        ("beacon_backfill_messages", "/messages/backfill"),
        ("beacon_get_node_activity", "/nodes/{nodeId}/activity"),
        ("beacon_get_node_neighbors", "/nodes/{nodeId}/neighbors"),
        ("beacon_get_node_observations", "/nodes/{nodeId}/observations"),
        ("beacon_get_observer_adverts", "/observers/{observerId}/adverts"),
        ("beacon_get_observer_telemetry", "/observers/{observerId}/telemetry"),
        ("beacon_list_observer_directory", "/observers/directory"),
        ("beacon_backfill_packets", "/packets/backfill"),
        ("beacon_get_region", "/regions/{regionId}"),
        ("beacon_get_route_observations", "/routes/{iata}/{pathKey}/observations"),
        ("beacon_get_scope", "/scopes/{name}"),
        ("beacon_get_clock_drift", "/stats/clock-drift"),
        ("beacon_get_node_types", "/stats/node-types"),
        ("beacon_get_observation_stats", "/stats/observations"),
        ("beacon_get_path_stats", "/stats/paths"),
        ("beacon_get_payload_breakdown", "/stats/payload-breakdown"),
        ("beacon_get_radio_presets", "/stats/radio-presets"),
        ("beacon_get_scope_stats", "/stats/scopes"),
        ("beacon_get_signal_stats", "/stats/signal"),
        ("beacon_get_top_nodes", "/stats/top-nodes"),
        ("beacon_get_top_observers", "/stats/top-observers"),
        ("beacon_get_top_talkers", "/stats/top-talkers"),
    ];
    assert_eq!(routes.len(), 26);
    for (name, route) in routes {
        let needle = format!("\n  {route}:\n");
        let index = swagger.find(&needle).expect("route must exist in pinned Swagger");
        let remainder = &swagger[index + needle.len()..];
        let end = remainder.find("\n  /").unwrap_or(remainder.len());
        let section = &remainder[..end];
        assert!(section.contains("\n    get:\n"), "{route} must be a GET route");

        let tool = tools.definition(name).expect("MCP tool must be registered");
        let schema: Value = serde_json::to_value(tool).unwrap();
        let mut next_is_query_name = false;
        for line in section.lines() {
            let line = line.trim();
            if line == "in: query" {
                next_is_query_name = true;
            } else if next_is_query_name && line.starts_with("name: ") {
                let param = line.trim_start_matches("name: ");
                assert!(
                    schema["inputSchema"]["properties"].get(param).is_some(),
                    "{name} missing upstream query filter {param} on {route}"
                );
                next_is_query_name = false;
            }
        }
    }
}
