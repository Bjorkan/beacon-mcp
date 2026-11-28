//! Expert gateway: list / describe / execute over the 53-read registry.
//! Additive in PR06; legacy 55 tools retained. No selectable profiles.

use beacon_api::{
    client::{BeaconClient, ErrorCode, GatewayError, Operation},
    registry::{REGISTRY, find_by_key},
};
use serde_json::{Value, json};

pub const MAX_LIST: usize = 50;
pub const DEFAULT_LIST: usize = 20;

fn invalid(message: impl Into<String>) -> GatewayError {
    GatewayError::new(ErrorCode::InvalidArgument, message)
}

pub fn categories() -> Vec<&'static str> {
    let mut categories = std::collections::BTreeSet::new();
    for entry in REGISTRY {
        if let Some((category, _)) = entry.key.split_once('.') {
            categories.insert(category);
        }
    }
    categories.into_iter().collect()
}

pub fn list(category: Option<&str>, limit: usize) -> Result<Value, GatewayError> {
    if limit == 0 || limit > MAX_LIST {
        return Err(invalid("limit must be from 1 to 50"));
    }
    let mut items = Vec::new();
    for entry in REGISTRY {
        let entry_category = entry
            .key
            .split_once('.')
            .map(|(category, _)| category)
            .unwrap_or("");
        if category.is_some_and(|wanted| wanted != entry_category) {
            continue;
        }
        items.push(json!({
            "key": entry.key,
            "category": entry_category,
            "path": format!("GET /api/v1/{}", entry.path),
            "summary": format!("{} ({}; min {})", entry.key, if entry.cost == beacon_api::registry::CostClass::Expensive { "expensive" } else { "normal" }, entry.min_version),
        }));
        if items.len() >= limit {
            break;
        }
    }
    Ok(json!({"items": items, "count": items.len()}))
}

fn tool_document(name: &str) -> Option<(&'static str, Value)> {
    macro_rules! load {
        ($file:expr) => {
            serde_json::from_str::<Value>(include_str!($file)).ok()
        };
    }
    for (file, document) in [
        ("core", load!("core-tool-definitions.json")),
        ("packet", load!("packet-tool-definitions.json")),
        ("routing", load!("routing-tool-definitions.json")),
        ("analytics", load!("analytics-tool-definitions.json")),
        ("extra", load!("extra-tool-definitions.json")),
    ]
    .into_iter()
    {
        if let Some(document) = document
            && let Some(tool) = document.get(name)
        {
            let _ = file;
            return Some((file, tool.clone()));
        }
    }
    None
}

pub fn tool_for_key(key: &str) -> Option<&'static str> {
    Some(match key {
        "server.info" => "beacon_get_server_info",
        "iata.list" => "beacon_list_iatas",
        "iata.get" => "beacon_get_iata",
        "iata.border" => "beacon_get_iata_border",
        "region.list" => "beacon_list_regions",
        "region.get" => "beacon_get_region",
        "scope.list" => "beacon_list_scopes",
        "scope.get" => "beacon_get_scope",
        "node.search" => "beacon_search_nodes",
        "node.get" => "beacon_get_node",
        "node.activity" => "beacon_get_node_activity",
        "node.neighbors" => "beacon_get_node_neighbors",
        "node.observations" => "beacon_get_node_observations",
        "node.poller" => "beacon_get_node_poller",
        "node.polling" => "beacon_get_node_polling",
        "node.telemetry" => "beacon_get_node_telemetry",
        "node.telemetry_latest" => "beacon_list_latest_node_telemetry",
        "observer.search" => "beacon_search_observers",
        "observer.get" => "beacon_get_observer",
        "observer.activity" => "beacon_get_observer_activity",
        "observer.adverts" => "beacon_get_observer_adverts",
        "observer.telemetry" => "beacon_get_observer_telemetry",
        "observer.directory" => "beacon_list_observer_directory",
        "packet.search" => "beacon_search_packets",
        "packet.get" => "beacon_get_packet",
        "packet.backfill" => "beacon_backfill_packets",
        "message.search" => "beacon_search_messages",
        "message.backfill" => "beacon_backfill_messages",
        "channel.list" => "beacon_list_channels",
        "channel.get" => "beacon_get_channel",
        "channel.messages" => "beacon_get_channel_messages",
        "route.list" => "beacon_list_routes",
        "route.search" => "beacon_search_routes",
        "route.cross" => "beacon_find_cross_iata_routes",
        "route.observations" => "beacon_get_route_observations",
        "trace.search" => "beacon_search_traces",
        "trace.get" => "beacon_get_trace",
        "stats.top_advertisers" => "beacon_get_top_advertisers",
        "stats.overview" => "beacon_get_network_overview",
        "stats.series" => "beacon_get_network_series",
        "stats.observer_comparison" => "beacon_compare_observers",
        "broker.list" => "beacon_list_brokers",
        "stats.clock_drift" => "beacon_get_clock_drift",
        "stats.node_types" => "beacon_get_node_types",
        "stats.observation_stats" => "beacon_get_observation_stats",
        "stats.path_stats" => "beacon_get_path_stats",
        "stats.payload_breakdown" => "beacon_get_payload_breakdown",
        "stats.radio_presets" => "beacon_get_radio_presets",
        "stats.scope_stats" => "beacon_get_scope_stats",
        "stats.signal" => "beacon_get_signal_stats",
        "stats.top_nodes" => "beacon_get_top_nodes",
        "stats.top_observers" => "beacon_get_top_observers",
        "stats.top_talkers" => "beacon_get_top_talkers",
        _ => return None,
    })
}

pub fn describe(key: &str) -> Result<Value, GatewayError> {
    let meta = find_by_key(key).ok_or_else(|| invalid("unknown operation"))?;
    let tool_name = tool_for_key(key).ok_or_else(|| invalid("unknown operation"))?;
    let (_, tool) = tool_document(tool_name).ok_or_else(|| invalid("unknown operation"))?;
    Ok(json!({
        "key": meta.key,
        "tool": tool_name,
        "path": format!("GET /api/v1/{}", meta.path),
        "inputSchema": tool.get("inputSchema").cloned().unwrap_or(Value::Null),
        "outputSchema": tool.get("outputSchema").cloned().unwrap_or(Value::Null),
        "example": meta.example_query,
        "capabilities": {
            "cost": format!("{:?}", meta.cost),
            "minVersion": meta.min_version,
            "pagination": meta.pagination,
            "timeWindow": meta.time_window,
        },
        "limitations": meta.error_recovery,
    }))
}

pub async fn execute(client: &BeaconClient, key: &str, args: Value) -> Result<Value, GatewayError> {
    let meta = find_by_key(key).ok_or_else(|| invalid("unknown operation"))?;
    if meta.operation == Operation::WsTicket {
        return Err(invalid("prohibited operation"));
    }
    // Reject path injection before network: args must be an object with bounded sizes.
    let object = args
        .as_object()
        .ok_or_else(|| invalid("invalid arguments"))?;
    for (field, value) in object {
        if field.len() > 64 || field.chars().any(char::is_control) {
            return Err(invalid("invalid argument name"));
        }
        if let Some(text) = value.as_str()
            && (text.len() > 8192 || text.contains("..") || text.chars().any(char::is_control))
        {
            return Err(invalid("invalid argument value"));
        }
    }
    // Route to the fixed Operation; BeaconClient enforces IDs, URL bounds, allowlist.
    // For PR06 parity, object endpoints use request(); array endpoints use array_page
    // when the operation is in the streaming allowlist. Here we use request() with
    // query pairs derived from args for offline fixture parity; tool handlers remain
    // the reference for exact filter mapping until the PR07 cutover.
    let query: Vec<(String, String)> = object
        .iter()
        .map(|(key, value)| {
            (
                key.clone(),
                match value {
                    Value::String(text) => text.clone(),
                    other => other.to_string(),
                },
            )
        })
        .collect();
    let query_refs: Vec<(&str, String)> = query
        .iter()
        .map(|(key, value)| (key.as_str(), value.clone()))
        .collect();
    // Note: id-carrying operations need an identifier; PR06 execute supports
    // id-less reads and list/search/stats paths fully; id paths are validated
    // by BeaconClient (invalid IDs rejected before network).
    let id = [
        "id",
        "uuid",
        "hash",
        "tag",
        "iata",
        "scope",
        "regionId",
        "channelID",
    ]
    .into_iter()
    .filter_map(|field| object.get(field))
    .find_map(|value| match value {
        Value::String(text) => Some(text.clone()),
        Value::Number(number) => Some(number.to_string()),
        _ => None,
    });
    // Special-case route.observations composite id (iata + pathKey).
    if meta.operation == Operation::RouteObservations {
        let iata = object.get("iata").and_then(Value::as_str).unwrap_or("");
        let path_key = object.get("pathKey").and_then(Value::as_str).unwrap_or("");
        let route_id = format!("{iata}:{path_key}");
        return client
            .request(meta.operation, Some(&route_id), &query_refs)
            .await
            .map(|data| json!({"data": data, "key": key}));
    }
    // Special-case channel.messages number id.
    if meta.operation == Operation::ChannelMessages
        && let Some(number) = object
            .get("id")
            .and_then(|value| value.as_u64().or_else(|| value.as_str()?.parse().ok()))
    {
        return client
            .request(meta.operation, Some(&number.to_string()), &query_refs)
            .await
            .map(|data| json!({"data": data, "key": key}));
    }
    client
        .request(meta.operation, id.as_deref(), &query_refs)
        .await
        .map(|data| json!({"data": data, "key": key}))
}

#[cfg(test)]
mod tests {
    use super::{describe, list, tool_for_key};
    use beacon_api::registry::REGISTRY;

    #[test]
    fn all_53_keys_have_tool_mapping() {
        assert_eq!(REGISTRY.len(), 53);
        for entry in REGISTRY {
            assert!(tool_for_key(entry.key).is_some(), "{}", entry.key);
            let described = describe(entry.key).unwrap();
            assert!(described.get("inputSchema").is_some());
            assert!(described.get("outputSchema").is_some());
        }
    }

    #[test]
    fn list_bounded_and_ws_ticket_absent() {
        let page = list(None, 20).unwrap();
        assert_eq!(page["count"], 20);
        assert!(list(None, 51).is_err());
        let text = serde_json::to_string(&page).unwrap();
        assert!(!text.contains("ws-ticket"));
        let empty = list(Some("no-such-category"), 20).unwrap();
        assert_eq!(empty["count"], 0);
    }
}

use rmcp::model::{JsonObject, Tool};

const EXPLORE_NAME: &str = "beacon_explore";

fn explore_definition() -> Option<Tool> {
    let document = include_str!("explore-tool-definitions.json");
    let value: Value = serde_json::from_str(document).ok()?;
    serde_json::from_value(value.get(EXPLORE_NAME)?.clone()).ok()
}

#[derive(Clone)]
pub struct ExploreTools {
    client: BeaconClient,
}

impl ExploreTools {
    pub fn new(client: BeaconClient) -> Self {
        Self { client }
    }

    pub fn list(&self) -> Vec<Tool> {
        explore_definition().into_iter().collect()
    }

    pub fn definition(&self, name: &str) -> Option<Tool> {
        if name == EXPLORE_NAME {
            explore_definition()
        } else {
            None
        }
    }

    pub async fn execute(
        &self,
        name: &str,
        arguments: Option<JsonObject>,
    ) -> Result<Value, GatewayError> {
        if name != EXPLORE_NAME {
            return Err(invalid("unknown Beacon tool"));
        }
        let args = arguments.unwrap_or_default();
        let mode = args
            .get("mode")
            .and_then(Value::as_str)
            .ok_or_else(|| invalid("mode is required"))?;
        match mode {
            "list" => {
                let category = args.get("category").and_then(Value::as_str);
                let limit = args
                    .get("limit")
                    .and_then(Value::as_u64)
                    .unwrap_or(DEFAULT_LIST as u64) as usize;
                list(category, limit)
            }
            "describe" => {
                let operation = args
                    .get("operation")
                    .and_then(Value::as_str)
                    .ok_or_else(|| invalid("operation is required"))?;
                describe(operation)
            }
            "execute" => {
                let operation = args
                    .get("operation")
                    .and_then(Value::as_str)
                    .ok_or_else(|| invalid("operation is required"))?;
                let operation_args = args
                    .get("args")
                    .cloned()
                    .unwrap_or(Value::Object(Default::default()));
                execute(&self.client, operation, operation_args).await
            }
            _ => Err(invalid("unknown mode")),
        }
    }
}
