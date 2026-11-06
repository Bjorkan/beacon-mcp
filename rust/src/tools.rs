use chrono::{DateTime, Utc};
use rmcp::model::{JsonObject, Tool};
use serde::Deserialize;
use serde_json::{Value, json};

use crate::client::{BeaconClient, ErrorCode, GatewayError, Operation};

const NAMES: [&str; 9] = [
    "beacon_get_server_info",
    "beacon_list_iatas",
    "beacon_list_regions",
    "beacon_list_scopes",
    "beacon_search_nodes",
    "beacon_get_node",
    "beacon_search_observers",
    "beacon_get_observer",
    "beacon_get_observer_activity",
];

#[derive(Clone)]
pub struct CoreTools {
    client: BeaconClient,
}

#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Args {
    id: Option<String>,
    limit: Option<u32>,
    cursor: Option<u64>,
    page_cursor: Option<String>,
    iatas: Option<Vec<String>>,
    region: Option<String>,
    region_id: Option<u32>,
    scope: Option<String>,
    name: Option<String>,
    r#type: Option<Value>,
    type_name: Option<String>,
    pubkey: Option<String>,
    pubkey_prefix: Option<String>,
    supports_multibyte_paths: Option<bool>,
    supports_multibyte_traces: Option<bool>,
    sort: Option<String>,
    order: Option<String>,
    neighbors: Option<bool>,
    broker: Option<String>,
    status: Option<String>,
    range: Option<String>,
    interval: Option<String>,
    until: Option<String>,
}

fn invalid(message: impl Into<String>) -> GatewayError {
    GatewayError::new(ErrorCode::InvalidArgument, message)
}

fn fields(name: &str) -> Option<&'static [&'static str]> {
    Some(match name {
        "beacon_get_server_info" => &[],
        "beacon_list_iatas" | "beacon_list_regions" => &["limit"],
        "beacon_list_scopes" => &["limit", "cursor", "iatas", "region", "regionId"],
        "beacon_search_nodes" => &[
            "limit",
            "cursor",
            "pageCursor",
            "iatas",
            "region",
            "regionId",
            "scope",
            "name",
            "type",
            "typeName",
            "pubkey",
            "pubkeyPrefix",
            "supportsMultibytePaths",
            "supportsMultibyteTraces",
            "sort",
            "order",
            "neighbors",
        ],
        "beacon_get_node" | "beacon_get_observer" => &["id"],
        "beacon_search_observers" => &[
            "limit",
            "cursor",
            "pageCursor",
            "iatas",
            "region",
            "regionId",
            "scope",
            "name",
            "type",
            "broker",
            "status",
        ],
        "beacon_get_observer_activity" => &["id", "range", "interval", "until"],
        _ => return None,
    })
}

/// Frozen tool contract including descriptions, input/output schemas and hints.
/// Runtime and CI parse this checked-in data without a separate schema generator.
pub(crate) fn tool_definition(document: &str, name: &str) -> Option<Tool> {
    let definitions: Value = serde_json::from_str(document).expect("valid frozen tool definitions");
    serde_json::from_value(definitions.get(name)?.clone()).ok()
}

impl CoreTools {
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
        tool_definition(include_str!("core-tool-definitions.json"), name)
    }

    pub async fn execute(
        &self,
        name: &str,
        arguments: Option<JsonObject>,
    ) -> Result<Value, GatewayError> {
        let allowed = fields(name).ok_or_else(|| invalid("unknown Beacon tool"))?;
        let args = arguments.unwrap_or_default();
        if let Some(key) = args.keys().find(|key| !allowed.contains(&key.as_str())) {
            return Err(invalid(format!("unknown filter: {key}")));
        }
        let args: Args = serde_json::from_value(Value::Object(args))
            .map_err(|_| invalid("invalid tool arguments"))?;
        let limit = args.limit.unwrap_or(20);
        if !(1..=50).contains(&limit) {
            return Err(invalid("limit must be from 1 to 50"));
        }
        if args.region.is_some() && args.region_id.is_some() {
            return Err(invalid("region and regionId are mutually exclusive"));
        }
        if args
            .region_id
            .is_some_and(|id| id == 0 || id > i32::MAX as u32)
        {
            return Err(invalid("regionId must be a positive 32-bit integer"));
        }
        if args.r#type.is_some() && args.type_name.is_some() {
            return Err(invalid("type and typeName are mutually exclusive"));
        }
        if args
            .sort
            .as_deref()
            .is_some_and(|sort| !["lastSeen", "name", "type", "neighbors"].contains(&sort))
            || args
                .order
                .as_deref()
                .is_some_and(|order| !["asc", "desc"].contains(&order))
        {
            return Err(invalid("invalid node sort or order"));
        }
        if args.cursor.is_some_and(|cursor| cursor != 0)
            && (args.order.as_deref() == Some("asc")
                || args.sort.as_deref().is_some_and(|sort| sort != "lastSeen"))
        {
            return Err(invalid(
                "numeric cursor works only with default lastSeen descending order",
            ));
        }
        if args
            .status
            .as_deref()
            .is_some_and(|status| !["online", "offline"].contains(&status))
        {
            return Err(invalid("invalid observer status"));
        }
        if args
            .type_name
            .as_deref()
            .is_some_and(|kind| !["companion", "repeater", "room_server", "sensor"].contains(&kind))
        {
            return Err(invalid("invalid node typeName"));
        }
        if [
            args.region.as_deref(),
            args.scope.as_deref(),
            args.broker.as_deref(),
        ]
        .into_iter()
        .flatten()
        .any(|value| value.trim().is_empty() || value.len() > 128)
            || args
                .name
                .as_deref()
                .is_some_and(|value| value.trim().is_empty() || value.len() > 256)
        {
            return Err(invalid("invalid text filter"));
        }
        if let Some(kind) = args.r#type.as_ref() {
            let valid = match name {
                "beacon_search_nodes" => kind.as_u64().is_some_and(|n| (1..=4).contains(&n)),
                "beacon_search_observers" => kind
                    .as_str()
                    .is_some_and(|text| !text.trim().is_empty() && text.len() <= 128),
                _ => false,
            };
            if !valid {
                return Err(invalid("invalid type filter"));
            }
        }
        if args
            .pubkey
            .as_deref()
            .is_some_and(|key| !hex_key(key, 64, 64))
            || args
                .pubkey_prefix
                .as_deref()
                .is_some_and(|key| !hex_key(key, 1, 64))
        {
            return Err(invalid("invalid public key or prefix"));
        }
        if args
            .page_cursor
            .as_deref()
            .is_some_and(|cursor| cursor.is_empty() || cursor.len() > 512)
        {
            return Err(invalid("invalid pageCursor"));
        }
        let mut query = Vec::new();
        let mut put = |key, value: Option<String>| {
            if let Some(value) = value {
                query.push((key, value));
            }
        };
        if let Some(iatas) = args.iatas {
            if iatas.is_empty() || iatas.len() > 20 {
                return Err(invalid("iatas must contain 1-20 codes"));
            }
            let normalized = iatas
                .iter()
                .map(|code| code.trim().to_ascii_uppercase())
                .collect::<Vec<_>>();
            if normalized
                .iter()
                .any(|code| code.len() != 3 || !code.bytes().all(|c| c.is_ascii_alphanumeric()))
            {
                return Err(invalid("invalid IATA code"));
            }
            put("iatas", Some(normalized.join(",")));
        }
        put("region", args.region);
        put("regionId", args.region_id.map(|n| n.to_string()));
        put("scope", args.scope);
        put("name", args.name);
        put(
            "type",
            args.r#type.map(|value| {
                value
                    .as_str()
                    .map(str::to_owned)
                    .unwrap_or_else(|| value.to_string())
            }),
        );
        put("typeName", args.type_name);
        put("pubkey", args.pubkey);
        put("pubkeyPrefix", args.pubkey_prefix);
        put(
            "supportsMultibytePaths",
            args.supports_multibyte_paths.map(|n| n.to_string()),
        );
        put(
            "supportsMultibyteTraces",
            args.supports_multibyte_traces.map(|n| n.to_string()),
        );
        put("sort", args.sort);
        put("order", args.order);
        put("neighbors", args.neighbors.map(|n| n.to_string()));
        put("broker", args.broker);
        put("status", args.status);
        put("cursor", args.cursor.map(|n| n.to_string()));
        put("pageCursor", args.page_cursor);
        let id = args.id.as_deref();
        match name {
            "beacon_get_server_info" => {
                self.client
                    .request(Operation::ServerInfo, None, &query)
                    .await
            }
            "beacon_list_iatas" | "beacon_list_regions" => {
                let op = if name == "beacon_list_iatas" {
                    Operation::Iatas
                } else {
                    Operation::Regions
                };
                let result = self
                    .client
                    .array_page(op, &query, limit as usize + 1)
                    .await?;
                let truncated = !result.complete || result.items.len() > limit as usize;
                let mut pagination = json!({"hasMore": false});
                if truncated {
                    pagination["truncated"] = json!(true);
                }
                Ok(
                    json!({"items": result.items.into_iter().take(limit as usize).collect::<Vec<_>>(), "pagination": pagination}),
                )
            }
            "beacon_list_scopes" => {
                let offset = args.cursor.unwrap_or(0) as usize;
                let values = self
                    .client
                    .request(
                        Operation::Scopes,
                        None,
                        &query
                            .iter()
                            .filter(|(k, _)| *k != "cursor")
                            .cloned()
                            .collect::<Vec<_>>(),
                    )
                    .await?;
                let values = values.as_array().ok_or_else(|| {
                    GatewayError::new(ErrorCode::Unavailable, "Beacon returned invalid scopes")
                })?;
                let items = values
                    .iter()
                    .skip(offset)
                    .take(limit as usize)
                    .cloned()
                    .collect::<Vec<_>>();
                let next = offset + items.len();
                Ok(
                    json!({"items": items, "pagination": if next < values.len() { json!({"hasMore":true,"nextCursor":{"cursor":next}}) } else { json!({"hasMore":false}) }}),
                )
            }
            "beacon_search_nodes" | "beacon_search_observers" => {
                put("limit", Some(limit.to_string()));
                let operation = if name == "beacon_search_nodes" {
                    Operation::Nodes
                } else {
                    Operation::Observers
                };
                let result = self.client.request(operation, None, &query).await?;
                page(result, limit as usize, name == "beacon_search_nodes")
            }
            "beacon_get_node" => self.client.request(Operation::Node, id, &query).await,
            "beacon_get_observer" => self.client.request(Operation::Observer, id, &query).await,
            "beacon_get_observer_activity" => {
                if let Some(range) = args.range {
                    let (amount, unit) = range.split_at(range.len().saturating_sub(1));
                    let count = amount
                        .parse::<u32>()
                        .map_err(|_| invalid("invalid observer range"))?;
                    let minutes = match unit {
                        "h" => count
                            .checked_mul(60)
                            .ok_or_else(|| invalid("invalid observer range"))?,
                        "m" => count,
                        _ => return Err(invalid("invalid observer range")),
                    };
                    if minutes == 0
                        || minutes > 43_200
                        || (minutes > 2_880
                            && matches!(args.interval.as_deref().unwrap_or("15m"), "5m" | "15m"))
                    {
                        return Err(invalid("range must be 48h or less for intervals under 1h"));
                    }
                    put("range", Some(range));
                }
                if let Some(interval) = args.interval {
                    if !["5m", "15m", "1h", "6h", "24h"].contains(&interval.as_str()) {
                        return Err(invalid("invalid observer interval"));
                    }
                    put("interval", Some(interval));
                }
                if let Some(until) = args.until {
                    let parsed = DateTime::parse_from_rfc3339(&until)
                        .map_err(|_| invalid("invalid UTC timestamp"))?;
                    if parsed.offset().local_minus_utc() != 0 || !until.ends_with('Z') {
                        return Err(invalid("timestamp must be UTC"));
                    }
                    put(
                        "until",
                        Some(parsed.with_timezone(&Utc).timestamp_millis().to_string()),
                    );
                }
                self.client
                    .request(Operation::ObserverActivity, id, &query)
                    .await
            }
            _ => Err(invalid("unknown Beacon tool")),
        }
    }
}

fn hex_key(key: &str, min: usize, max: usize) -> bool {
    (min..=max).contains(&key.len()) && key.bytes().all(|c| c.is_ascii_hexdigit())
}

fn page(value: Value, limit: usize, include_sort: bool) -> Result<Value, GatewayError> {
    let source = value
        .get("items")
        .and_then(Value::as_array)
        .cloned()
        .ok_or_else(|| GatewayError::new(ErrorCode::Unavailable, "Beacon returned invalid page"))?;
    let items = source.iter().take(limit).cloned().collect::<Vec<_>>();
    let has_more = value["hasMore"].as_bool().unwrap_or(false) || source.len() > limit;
    let mut pagination = json!({"hasMore": has_more});
    if has_more {
        let mut cursor = serde_json::Map::new();
        if let Some(token) = value.get("nextPageCursor") {
            cursor.insert("pageCursor".into(), token.clone());
        }
        if let Some(number) = value.get("nextCursor") {
            cursor.insert("cursor".into(), number.clone());
        }
        if !cursor.is_empty() {
            pagination["nextCursor"] = Value::Object(cursor);
        }
    }
    let mut result = json!({"items": items, "pagination": pagination});
    if include_sort {
        if let Some(sort) = value.get("sort") {
            result["sort"] = sort.clone();
        }
        if let Some(order) = value.get("order") {
            result["order"] = order.clone();
        }
    }
    Ok(result)
}
