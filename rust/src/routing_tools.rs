use crate::{
    client::{BeaconClient, ErrorCode, GatewayError, Operation},
    packet_tools::timestamp,
};
use rmcp::model::{JsonObject, Tool};
use serde::Deserialize;
use serde_json::{Value, json};

const NAMES: [&str; 5] = [
    "beacon_list_routes",
    "beacon_search_routes",
    "beacon_find_cross_iata_routes",
    "beacon_search_traces",
    "beacon_get_trace",
];

#[derive(Clone)]
pub struct RoutingTools {
    client: BeaconClient,
}

#[derive(Default, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Args {
    iata: Option<String>,
    iatas: Option<Vec<String>>,
    hop_count: Option<u32>,
    cursor: Option<u64>,
    cursor_id: Option<u32>,
    cursor_tag: Option<String>,
    limit: Option<u32>,
    from: Option<String>,
    to: Option<String>,
    from_hash: Option<String>,
    to_hash: Option<String>,
    from_iata: Option<String>,
    to_iata: Option<String>,
    region: Option<String>,
    region_id: Option<u32>,
    scope: Option<String>,
    since: Option<String>,
    until: Option<String>,
    r#type: Option<String>,
    tag: Option<String>,
}

fn fields(name: &str) -> Option<&'static [&'static str]> {
    Some(match name {
        "beacon_list_routes" => &["iata", "iatas", "hopCount", "cursor", "cursorId", "limit"],
        "beacon_search_routes" => &["iata", "iatas", "from", "to", "limit"],
        "beacon_find_cross_iata_routes" => {
            &["fromHash", "toHash", "fromIata", "toIata", "iatas", "limit"]
        }
        "beacon_search_traces" => &[
            "iatas",
            "region",
            "regionId",
            "scope",
            "since",
            "until",
            "cursor",
            "cursorTag",
            "type",
            "limit",
        ],
        "beacon_get_trace" => &["tag"],
        _ => return None,
    })
}
fn invalid(message: impl Into<String>) -> GatewayError {
    GatewayError::new(ErrorCode::InvalidArgument, message)
}
fn hash(value: &str, min: usize, max: usize) -> bool {
    (min..=max).contains(&value.len())
        && value.len().is_multiple_of(2)
        && value.bytes().all(|c| c.is_ascii_hexdigit())
}
fn iata(value: &str) -> bool {
    let value = value.trim();
    value.len() == 3 && value.bytes().all(|c| c.is_ascii_alphanumeric())
}
fn required_hash(value: Option<&str>) -> Result<String, GatewayError> {
    let value = value.ok_or_else(|| invalid("missing route hash"))?;
    if !hash(value, 2, 8) {
        return Err(invalid("route hop hash must be 1-4 bytes of hex"));
    }
    Ok(value.to_ascii_lowercase())
}

impl RoutingTools {
    pub fn new(client: BeaconClient) -> Self {
        Self { client }
    }
    pub fn list(&self) -> Vec<Tool> {
        NAMES
            .iter()
            .filter_map(|name| self.definition(name))
            .collect()
    }
    pub fn definition(&self, name: &str) -> Option<Tool> {
        fields(name)?;
        crate::tools::tool_definition(include_str!("routing-tool-definitions.json"), name)
    }
    pub async fn execute(
        &self,
        name: &str,
        arguments: Option<JsonObject>,
    ) -> Result<Value, GatewayError> {
        let allowed = fields(name).ok_or_else(|| invalid("unknown Beacon tool"))?;
        let raw = arguments.unwrap_or_default();
        if raw.keys().any(|key| !allowed.contains(&key.as_str())) {
            return Err(invalid("unknown filter"));
        }
        let args: Args = serde_json::from_value(Value::Object(raw))
            .map_err(|_| invalid("invalid tool arguments"))?;
        let limit = args.limit.unwrap_or(20);
        if !(1..=50).contains(&limit) {
            return Err(invalid("limit must be from 1 to 50"));
        }
        if args.iata.is_some() && args.iatas.is_some() {
            return Err(invalid("iata and iatas are mutually exclusive"));
        }
        if args.from_iata.is_some() != args.to_iata.is_some() {
            return Err(invalid("fromIata and toIata must be given together"));
        }
        if args.from_iata.is_some() && args.iatas.is_some()
            || args.from_iata.is_some() && args.iatas.is_some()
            || args.cursor_id.is_some() && args.cursor.is_none()
            || args.cursor_tag.is_some() && args.cursor.is_none()
            || args.region.is_some() && args.region_id.is_some()
        {
            return Err(invalid("mutually exclusive or incomplete filters"));
        }
        if args
            .region_id
            .is_some_and(|id| id == 0 || id > i32::MAX as u32)
            || args
                .cursor_id
                .is_some_and(|id| id == 0 || id > i32::MAX as u32)
            || args
                .hop_count
                .is_some_and(|count| count == 0 || count > i32::MAX as u32)
        {
            return Err(invalid("invalid positive integer"));
        }
        let mut query = Vec::new();
        let mut put = |key, value: Option<String>| {
            if let Some(value) = value {
                query.push((key, value));
            }
        };
        if let Some(value) = args.iata {
            if !iata(&value) {
                return Err(invalid("invalid iata"));
            }
            put("iata", Some(value.trim().to_ascii_uppercase()));
        }
        if let Some(values) = args.iatas {
            let bound = if name == "beacon_search_traces" {
                20
            } else {
                100
            };
            if values.is_empty() || values.len() > bound || values.iter().any(|code| !iata(code)) {
                return Err(invalid("invalid iatas"));
            }
            let codes = values
                .iter()
                .map(|value| value.trim().to_ascii_uppercase())
                .collect::<Vec<_>>();
            if name == "beacon_find_cross_iata_routes"
                && codes.iter().collect::<std::collections::HashSet<_>>().len() < 2
            {
                return Err(invalid("cross-IATA search requires two distinct codes"));
            }
            put("iatas", Some(codes.join(",")));
        }
        if let Some(value) = args.from_iata {
            if !iata(&value) {
                return Err(invalid("invalid fromIata"));
            }
            put("fromIata", Some(value.trim().to_ascii_uppercase()));
        }
        if let Some(value) = args.to_iata {
            if !iata(&value) {
                return Err(invalid("invalid toIata"));
            }
            put("toIata", Some(value.trim().to_ascii_uppercase()));
        }
        match name {
            "beacon_search_routes" => {
                put("from", Some(required_hash(args.from.as_deref())?));
                put("to", Some(required_hash(args.to.as_deref())?));
            }
            "beacon_find_cross_iata_routes" => {
                put("fromHash", Some(required_hash(args.from_hash.as_deref())?));
                put("toHash", Some(required_hash(args.to_hash.as_deref())?));
            }
            _ => {}
        }
        put("hopCount", args.hop_count.map(|value| value.to_string()));
        put("cursor", args.cursor.map(|value| value.to_string()));
        put("cursorId", args.cursor_id.map(|value| value.to_string()));
        if let Some(tag) = args.cursor_tag {
            if !hash(&tag, 8, 8) {
                return Err(invalid("invalid cursorTag"));
            }
            put("cursorTag", Some(tag));
        }
        if args
            .region
            .as_deref()
            .is_some_and(|value| value.trim().is_empty() || value.len() > 128)
            || args
                .scope
                .as_deref()
                .is_some_and(|value| value.trim().is_empty() || value.len() > 128)
        {
            return Err(invalid("invalid text filter"));
        }
        put("region", args.region);
        put("regionId", args.region_id.map(|value| value.to_string()));
        put("scope", args.scope);
        let since = args.since.as_deref().map(timestamp).transpose()?;
        let until = args.until.as_deref().map(timestamp).transpose()?;
        if since.zip(until).is_some_and(|(start, end)| start >= end) {
            return Err(invalid("since must be before until"));
        }
        put("since", since.map(|value| value.to_string()));
        put("until", until.map(|value| value.to_string()));
        if let Some(kind) = args.r#type {
            if !["TRACE", "PING"].contains(&kind.as_str()) {
                return Err(invalid("invalid trace type"));
            }
            put("type", Some(kind));
        }
        if name == "beacon_get_trace" {
            return self
                .client
                .request(Operation::Trace, args.tag.as_deref(), &query)
                .await;
        }
        if name == "beacon_search_routes" || name == "beacon_find_cross_iata_routes" {
            let op = if name == "beacon_search_routes" {
                Operation::RouteSearch
            } else {
                Operation::CrossRoutes
            };
            let result = self
                .client
                .array_page(op, &query, limit as usize + 1)
                .await?;
            let truncated = !result.complete || result.items.len() > limit as usize;
            let mut pagination = json!({"hasMore":false});
            if truncated {
                pagination["truncated"] = json!(true);
            }
            return Ok(
                json!({"items":result.items.into_iter().take(limit as usize).collect::<Vec<_>>(),"pagination":pagination}),
            );
        }
        put("limit", Some(limit.to_string()));
        let op = if name == "beacon_list_routes" {
            Operation::Routes
        } else {
            Operation::Traces
        };
        let response = self.client.request(op, None, &query).await?;
        let source = response.as_array().ok_or_else(|| {
            GatewayError::new(
                ErrorCode::Unavailable,
                "Beacon returned invalid route or trace list",
            )
        })?;
        let items = source
            .iter()
            .take(limit as usize)
            .cloned()
            .collect::<Vec<_>>();
        let mut pagination = json!({"hasMore":false});
        if let Some(last) = items.last().filter(|_| items.len() == limit as usize) {
            let cursor = if name == "beacon_list_routes" {
                json!({"cursor":last["lastSeen"],"cursorId":last["id"]})
            } else {
                json!({"cursor":last["lastHeardAt"],"cursorTag":last["traceTag"]})
            };
            if cursor["cursor"].is_number()
                && cursor
                    .as_object()
                    .is_some_and(|fields| !fields.values().any(Value::is_null))
            {
                pagination = json!({"hasMore":true,"nextCursor":cursor});
            }
        }
        Ok(json!({"items":items,"pagination":pagination}))
    }
}
