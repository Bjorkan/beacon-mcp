//! Stateless composite analysis over a strictly bounded number of public Beacon GETs.
use crate::{
    client::{BeaconClient, ErrorCode, GatewayError, Operation},
    tools::tool_definition,
};
use rmcp::model::{JsonObject, Tool};
use serde::Deserialize;
use serde_json::{Value, json};
use uuid::{Uuid, Variant};

const NAMES: [&str; 2] = ["beacon_diagnose_node", "beacon_compare_regions"];

#[derive(Clone)]
pub struct CompositeTools {
    client: BeaconClient,
}

#[derive(Default, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Args {
    id: Option<String>,
    range: Option<String>,
    neighbor_limit: Option<u8>,
    iata_a: Option<String>,
    iata_b: Option<String>,
}

fn invalid(reason: &str) -> GatewayError {
    GatewayError::new(ErrorCode::InvalidArgument, reason)
}

fn iata(raw: &str) -> Result<String, GatewayError> {
    let text = raw.trim();
    if text.len() != 3 || !text.bytes().all(|byte| byte.is_ascii_alphanumeric()) {
        return Err(invalid("IATA must be exactly three ASCII letters or digits"));
    }
    Ok(text.to_ascii_uppercase())
}

fn node_id(raw: &str) -> Result<String, GatewayError> {
    let id = Uuid::parse_str(raw).map_err(|_| invalid("invalid node UUID"))?;
    if raw.len() != 36
        || id.get_variant() != Variant::RFC4122
        || !(1..=8).contains(&id.get_version_num())
    {
        return Err(invalid("invalid node UUID"));
    }
    Ok(id.to_string())
}

impl CompositeTools {
    pub fn new(client: BeaconClient) -> Self {
        Self { client }
    }

    pub fn list(&self) -> Vec<Tool> {
        NAMES.iter().filter_map(|name| self.definition(name)).collect()
    }

    pub fn definition(&self, name: &str) -> Option<Tool> {
        if !NAMES.contains(&name) {
            return None;
        }
        tool_definition(include_str!("composite-tool-definitions.json"), name)
    }

    pub async fn execute(
        &self,
        name: &str,
        arguments: Option<JsonObject>,
    ) -> Result<Value, GatewayError> {
        if !NAMES.contains(&name) {
            return Err(invalid("unknown composite tool"));
        }
        let args: Args = serde_json::from_value(Value::Object(arguments.unwrap_or_default()))
            .map_err(|_| invalid("invalid composite tool arguments"))?;
        match name {
            "beacon_diagnose_node" => {
                if args.iata_a.is_some() || args.iata_b.is_some() {
                    return Err(invalid("unsupported node diagnostic filter"));
                }
                let id = node_id(args.id.as_deref().ok_or_else(|| invalid("id is required"))?)?;
                let range = args.range.unwrap_or_else(|| "24h".to_owned());
                if !["24h", "48h", "168h"].contains(&range.as_str()) {
                    return Err(invalid("unsupported diagnostic range"));
                }
                let limit = usize::from(args.neighbor_limit.unwrap_or(10));
                if !(1..=20).contains(&limit) {
                    return Err(invalid("neighborLimit must be 1-20"));
                }
                let get_node = self.client.request(Operation::Node, Some(&id), &[]);
                let get_activity = self.client.request(
                    Operation::NodeActivity,
                    Some(&id),
                    &[("range", range.clone())],
                );
                let get_neighbors = self.client.array_page_with_id(
                    Operation::NodeNeighbors,
                    Some(&id),
                    &[],
                    limit + 1,
                );
                let (node, activity, neighbors) =
                    tokio::try_join!(get_node, get_activity, get_neighbors)?;
                let truncated = !neighbors.complete || neighbors.items.len() > limit;
                Ok(json!({
                    "node": node,
                    "activity": activity,
                    "neighbors": neighbors.items.into_iter().take(limit).collect::<Vec<_>>(),
                    "neighborsTruncated": truncated,
                    "requestedRange": range,
                    "note": "Read-only snapshot; Beacon may have incomplete geographic coverage."
                }))
            }
            "beacon_compare_regions" => {
                if args.id.is_some() || args.range.is_some() || args.neighbor_limit.is_some() {
                    return Err(invalid("unsupported regional comparison filter"));
                }
                let a = iata(args.iata_a.as_deref().ok_or_else(|| invalid("iataA is required"))?)?;
                let b = iata(args.iata_b.as_deref().ok_or_else(|| invalid("iataB is required"))?)?;
                if a == b {
                    return Err(invalid("iataA and iataB must be different"));
                }
                let left = self.client.request(Operation::Overview, None, &[("iatas", a.clone())]);
                let right = self.client.request(Operation::Overview, None, &[("iatas", b.clone())]);
                let (overview_a, overview_b) = tokio::try_join!(left, right)?;
                Ok(json!({
                    "regionA": {"iata": a, "overview": overview_a},
                    "regionB": {"iata": b, "overview": overview_b},
                    "note": "Independent snapshots, not synchronized measurements."
                }))
            }
            _ => Err(invalid("unknown composite tool")),
        }
    }
}
