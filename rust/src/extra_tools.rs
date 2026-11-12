//! Additional fixed public Beacon v2.2 REST reads.
//! No caller-supplied URL, HTTP method, credentials or administrative paths.
use crate::{
    client::{BeaconClient, ErrorCode, GatewayError, Operation},
    packet_tools::timestamp,
    tools::page,
};
use rmcp::model::{JsonObject, Tool};
use serde_json::{Value, json};

const NAMES: [&str; 26] = [
    "beacon_list_brokers",
    "beacon_get_channel",
    "beacon_get_iata",
    "beacon_get_iata_border",
    "beacon_backfill_messages",
    "beacon_get_node_activity",
    "beacon_get_node_neighbors",
    "beacon_get_node_observations",
    "beacon_get_observer_adverts",
    "beacon_get_observer_telemetry",
    "beacon_list_observer_directory",
    "beacon_backfill_packets",
    "beacon_get_region",
    "beacon_get_route_observations",
    "beacon_get_scope",
    "beacon_get_clock_drift",
    "beacon_get_node_types",
    "beacon_get_observation_stats",
    "beacon_get_path_stats",
    "beacon_get_payload_breakdown",
    "beacon_get_radio_presets",
    "beacon_get_scope_stats",
    "beacon_get_signal_stats",
    "beacon_get_top_nodes",
    "beacon_get_top_observers",
    "beacon_get_top_talkers",
];

#[derive(Clone)]
pub struct ExtraTools {
    client: BeaconClient,
}

#[derive(Clone, Copy)]
enum Shape {
    Object,
    Page,
    Array,
}

fn invalid(message: impl Into<String>) -> GatewayError {
    GatewayError::new(ErrorCode::InvalidArgument, message)
}

fn operation(name: &str) -> Option<(Operation, Shape)> {
    use Operation::*;
    use Shape::*;
    Some(match name {
        "beacon_list_brokers" => (Brokers, Array),
        "beacon_get_channel" => (ChannelDetail, Object),
        "beacon_get_iata" => (IataDetail, Object),
        "beacon_get_iata_border" => (IataBorder, Object),
        "beacon_backfill_messages" => (MessagesBackfill, Array),
        "beacon_get_node_activity" => (NodeActivity, Object),
        "beacon_get_node_neighbors" => (NodeNeighbors, Array),
        "beacon_get_node_observations" => (NodeObservations, Page),
        "beacon_get_observer_adverts" => (ObserverAdverts, Page),
        "beacon_get_observer_telemetry" => (ObserverTelemetry, Object),
        "beacon_list_observer_directory" => (ObserverDirectory, Page),
        "beacon_backfill_packets" => (PacketsBackfill, Array),
        "beacon_get_region" => (RegionDetail, Object),
        "beacon_get_route_observations" => (RouteObservations, Object),
        "beacon_get_scope" => (ScopeDetail, Object),
        "beacon_get_clock_drift" => (ClockDrift, Array),
        "beacon_get_node_types" => (NodeTypes, Object),
        "beacon_get_observation_stats" => (ObservationStats, Object),
        "beacon_get_path_stats" => (PathStats, Object),
        "beacon_get_payload_breakdown" => (PayloadBreakdown, Object),
        "beacon_get_radio_presets" => (RadioPresets, Object),
        "beacon_get_scope_stats" => (ScopeStats, Object),
        "beacon_get_signal_stats" => (SignalStats, Object),
        "beacon_get_top_nodes" => (TopNodes, Array),
        "beacon_get_top_observers" => (TopObservers, Array),
        "beacon_get_top_talkers" => (TopTalkers, Array),
        _ => return None,
    })
}

fn duration(value: &str) -> Result<(), GatewayError> {
    let (amount, unit) = value.split_at(value.len().saturating_sub(1));
    let amount: u32 = amount.parse().map_err(|_| invalid("invalid duration"))?;
    let minutes = match unit {
        "m" => amount,
        "h" => amount.checked_mul(60).ok_or_else(|| invalid("invalid duration"))?,
        _ => return Err(invalid("duration must use whole minutes or hours")),
    };
    if minutes == 0 || minutes > 43_200 {
        return Err(invalid("range must be between 1 minute and 720 hours"));
    }
    Ok(())
}

fn iata(value: &str) -> Result<String, GatewayError> {
    let value = value.trim();
    if value.len() != 3 || !value.bytes().all(|b| b.is_ascii_alphanumeric()) {
        return Err(invalid("invalid IATA code"));
    }
    Ok(value.to_ascii_uppercase())
}

impl ExtraTools {
    pub fn new(client: BeaconClient) -> Self {
        Self { client }
    }

    pub fn list(&self) -> Vec<Tool> {
        NAMES.iter().filter_map(|name| self.definition(name)).collect()
    }

    pub fn definition(&self, name: &str) -> Option<Tool> {
        operation(name)?;
        crate::tools::tool_definition(include_str!("extra-tool-definitions.json"), name)
    }

    pub async fn execute(
        &self,
        name: &str,
        arguments: Option<JsonObject>,
    ) -> Result<Value, GatewayError> {
        let (op, shape) = operation(name).ok_or_else(|| invalid("unknown Beacon tool"))?;
        let definition = self.definition(name).ok_or_else(|| invalid("unknown Beacon tool"))?;
        let raw = arguments.unwrap_or_default();
        let properties = definition.input_schema["properties"]
            .as_object()
            .ok_or_else(|| invalid("invalid tool contract"))?;
        if raw.keys().any(|key| !properties.contains_key(key)) {
            return Err(invalid("unknown filter"));
        }
        for required in definition.input_schema["required"].as_array().into_iter().flatten() {
            if let Some(key) = required.as_str() {
                if !raw.contains_key(key) {
                    return Err(invalid(format!("missing {key}")));
                }
            }
        }
        if raw.contains_key("region") && raw.contains_key("regionId")
            || raw.contains_key("iata") && raw.contains_key("iatas")
        {
            return Err(invalid("mutually exclusive location filters"));
        }
        if name == "beacon_get_route_observations" {
            if raw.contains_key("range") && (raw.contains_key("since")
                || raw.contains_key("until") || raw.contains_key("pageCursor"))
                || raw.contains_key("since") != raw.contains_key("until")
            {
                return Err(invalid("range is exclusive with the explicit window and cursor; since and until must be paired"));
            }
        }
        let limit = match raw.get("limit") {
            Some(value) => value.as_u64().ok_or_else(|| invalid("invalid limit"))?,
            None => 20,
        };
        if !(1..=50).contains(&limit) {
            return Err(invalid("limit must be from 1 to 50"));
        }

        let identifier = match op {
            Operation::ChannelDetail | Operation::RegionDetail
            | Operation::IataDetail | Operation::IataBorder
            | Operation::NodeActivity | Operation::NodeNeighbors
            | Operation::NodeObservations | Operation::ObserverAdverts
            | Operation::ObserverTelemetry | Operation::ScopeDetail => {
                Some(raw.get("id").and_then(Value::as_str)
                    .ok_or_else(|| invalid("id must be a string"))?.to_owned())
            }
            Operation::RouteObservations => {
                let code = iata(raw.get("iata").and_then(Value::as_str)
                    .ok_or_else(|| invalid("missing route IATA"))?)?;
                let hash = raw.get("pathKey").and_then(Value::as_str)
                    .ok_or_else(|| invalid("missing pathKey"))?;
                Some(format!("{code}:{hash}"))
            }
            _ => None,
        };
        let mut query: Vec<(&str, String)> = Vec::new();
        let mut start = None;
        let mut end = None;
        for (key, value) in &raw {
            if key == "id" || key == "pathKey" || key == "limit"
                || key == "iata" && name == "beacon_get_route_observations"
            {
                continue;
            }
            let converted = match key.as_str() {
                "iatas" => {
                    let codes = value.as_array().ok_or_else(|| invalid("invalid iatas"))?;
                    if codes.is_empty() || codes.len() > 20 {
                        return Err(invalid("iatas must contain 1-20 values"));
                    }
                    codes.iter().map(|code| {
                        iata(code.as_str().ok_or_else(|| invalid("invalid IATA"))?)
                    }).collect::<Result<Vec<_>, _>>()?.join(",")
                }
                "iata" => iata(value.as_str().ok_or_else(|| invalid("invalid iata"))?)?,
                "since" | "until" => {
                    let ms = timestamp(value.as_str().ok_or_else(|| invalid("invalid timestamp"))?)?;
                    if key == "since" { start = Some(ms); } else { end = Some(ms); }
                    ms.to_string()
                }
                "range" => {
                    let text = value.as_str().ok_or_else(|| invalid("invalid duration"))?;
                    duration(text)?;
                    text.to_owned()
                }
                "cursor" | "afterId" | "afterObservationId" | "regionId"
                | "payloadType" | "routeType" => {
                    let number = value.as_u64().ok_or_else(|| invalid("invalid integer"))?;
                    if number > 9_007_199_254_740_991
                        || key == "regionId" && (number == 0 || number > i32::MAX as u64)
                        || key == "payloadType" && number > 32767
                        || key == "routeType" && number > 3
                    {
                        return Err(invalid("integer outside supported range"));
                    }
                    number.to_string()
                }
                "keyKnown" => value.as_bool().ok_or_else(|| invalid("invalid boolean"))?.to_string(),
                _ => {
                    let text = value.as_str().ok_or_else(|| invalid("invalid text filter"))?;
                    if text.trim().is_empty() || text.len() > 512
                        || text.chars().any(char::is_control)
                    {
                        return Err(invalid("invalid text filter"));
                    }
                    if key == "sort" && !["traffic", "name"].contains(&text)
                        || key == "status" && !["online", "offline"].contains(&text)
                    {
                        return Err(invalid("unsupported sort or status"));
                    }
                    text.to_owned()
                }
            };
            query.push((key.as_str(), converted));
        }
        if start.zip(end).is_some_and(|(since, until)| since >= until) {
            return Err(invalid("since must be before until"));
        }
        // The Beacon server applies a hard page cap, but this gateway is stricter.
        if properties.contains_key("limit") {
            query.push(("limit", limit.to_string()));
        }
        let response = self.client.request(op, identifier.as_deref(), &query).await?;
        match shape {
            Shape::Page => {
                let mut result = page(response.clone(), limit as usize, false)?;
                if name == "beacon_list_observer_directory" {
                    for key in ["windowStart", "windowEnd", "coverage", "sort", "order"] {
                        if let Some(value) = response.get(key) {
                            result[key] = value.clone();
                        }
                    }
                }
                Ok(result)
            }
            Shape::Array => {
                let values = response.as_array().ok_or_else(|| {
                    GatewayError::new(ErrorCode::Unavailable, "Beacon returned invalid list")
                })?;
                let truncated = values.len() > limit as usize;
                let mut pagination = json!({"hasMore": false});
                if truncated {
                    pagination["truncated"] = json!(true);
                }
                Ok(json!({
                    "items": values.iter().take(limit as usize).collect::<Vec<_>>(),
                    "pagination": pagination
                }))
            }
            Shape::Object => {
                if response.is_null() {
                    Ok(json!({"available": false}))
                } else if response.is_object() {
                    Ok(response)
                } else if let Some(items) = response.as_array() {
                    // A few aggregation endpoints legitimately return arrays.
                    Ok(json!({"items":items}))
                } else {
                    Err(GatewayError::new(ErrorCode::Unavailable, "Beacon returned an invalid result"))
                }
            }
        }
    }
}
