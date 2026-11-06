use crate::{
    client::{BeaconClient, ErrorCode, GatewayError, Operation},
    packet_tools::timestamp,
};
use rmcp::model::{JsonObject, Tool, ToolAnnotations};
use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::{Value, json};
use std::sync::Arc;
use uuid::{Uuid, Variant};

const NAMES: [&str; 4] = [
    "beacon_get_top_advertisers",
    "beacon_get_network_overview",
    "beacon_get_network_series",
    "beacon_compare_observers",
];
#[derive(Clone)]
pub struct AnalyticsTools {
    client: BeaconClient,
}

#[derive(Default, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Args {
    iatas: Option<Vec<String>>,
    region: Option<String>,
    region_id: Option<u32>,
    sort: Option<String>,
    since: Option<String>,
    until: Option<String>,
    limit: Option<u32>,
    observer_a: Option<String>,
    observer_b: Option<String>,
}
fn fields(name: &str) -> Option<&'static [&'static str]> {
    Some(match name {
        "beacon_get_top_advertisers" => &["iatas", "region", "regionId", "sort", "since", "limit"],
        "beacon_get_network_overview" => &["iatas", "region", "regionId"],
        "beacon_get_network_series" => &["iatas", "region", "regionId", "since", "until"],
        "beacon_compare_observers" => &[
            "iatas",
            "region",
            "regionId",
            "observerA",
            "observerB",
            "since",
            "until",
        ],
        _ => return None,
    })
}
fn invalid(message: impl Into<String>) -> GatewayError {
    GatewayError::new(ErrorCode::InvalidArgument, message)
}
fn observer(value: Option<String>) -> Result<String, GatewayError> {
    let value = value.ok_or_else(|| invalid("observer UUID is required"))?;
    let parsed = Uuid::parse_str(&value).map_err(|_| invalid("invalid observer UUID"))?;
    if value.len() != 36
        || parsed.get_variant() != Variant::RFC4122
        || !(1..=8).contains(&parsed.get_version_num())
    {
        return Err(invalid("invalid observer UUID"));
    }
    Ok(parsed.to_string())
}

impl AnalyticsTools {
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
        let allowed = fields(name)?;
        let mut schema = serde_json::to_value(schemars::schema_for!(Args)).ok()?;
        schema
            .get_mut("properties")?
            .as_object_mut()?
            .retain(|key, _| allowed.contains(&key.as_str()));
        match name {
            "beacon_get_top_advertisers" => schema["required"] = json!(["sort"]),
            "beacon_get_network_series" => schema["required"] = json!(["since", "until"]),
            "beacon_compare_observers" => {
                schema["required"] = json!(["observerA", "observerB", "since", "until"])
            }
            _ => {}
        }
        let object: JsonObject = schema.as_object()?.clone();
        let mut tool = Tool::new(
            name.to_owned(),
            format!("Read Beacon analytics: {name}"),
            Arc::new(object),
        );
        tool.output_schema =
            crate::tools::output_schema(include_str!("analytics-output-schemas.json"), name);
        tool.annotations = Some(
            ToolAnnotations::new()
                .read_only(true)
                .destructive(false)
                .idempotent(true)
                .open_world(true),
        );
        Some(tool)
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
        if args.region.is_some() && args.region_id.is_some()
            || args
                .region_id
                .is_some_and(|id| id == 0 || id > i32::MAX as u32)
        {
            return Err(invalid("invalid region filter"));
        }
        if args
            .region
            .as_deref()
            .is_some_and(|value| value.trim().is_empty() || value.len() > 128)
        {
            return Err(invalid("invalid region slug"));
        }
        let limit = args.limit.unwrap_or(20);
        if !(1..=50).contains(&limit) {
            return Err(invalid("limit must be from 1 to 50"));
        }
        let mut query = Vec::new();
        let mut put = |key, value: Option<String>| {
            if let Some(value) = value {
                query.push((key, value));
            }
        };
        if let Some(iatas) = args.iatas {
            if iatas.is_empty()
                || iatas.len() > 20
                || iatas.iter().any(|code| {
                    code.len() != 3 || !code.bytes().all(|byte| byte.is_ascii_alphanumeric())
                })
            {
                return Err(invalid("invalid IATA codes"));
            }
            put(
                "iatas",
                Some(
                    iatas
                        .iter()
                        .map(|value| value.to_ascii_uppercase())
                        .collect::<Vec<_>>()
                        .join(","),
                ),
            );
        }
        put("region", args.region);
        put("regionId", args.region_id.map(|value| value.to_string()));
        let since = args.since.as_deref().map(timestamp).transpose()?;
        let until = args.until.as_deref().map(timestamp).transpose()?;
        if (name == "beacon_get_network_series" || name == "beacon_compare_observers")
            && (since.is_none() || until.is_none())
        {
            return Err(invalid("since and until are required"));
        }
        if since.zip(until).is_some_and(|(start, end)| start >= end) {
            return Err(invalid("since must be before until"));
        }
        put("since", since.map(|value| value.to_string()));
        put("until", until.map(|value| value.to_string()));
        if name == "beacon_compare_observers" {
            let a = observer(args.observer_a)?;
            let b = observer(args.observer_b)?;
            if a == b {
                return Err(invalid("observerA and observerB must be different"));
            }
            put("observerA", Some(a));
            put("observerB", Some(b));
        }
        let operation = match name {
            "beacon_get_network_overview" => Operation::Overview,
            "beacon_get_network_series" => Operation::Series,
            "beacon_compare_observers" => Operation::ObserverComparison,
            "beacon_get_top_advertisers" => {
                if !matches!(args.sort.as_deref(), Some("flood" | "direct")) {
                    return Err(invalid("sort must be flood or direct"));
                }
                put("sort", args.sort);
                put("limit", Some(limit.to_string()));
                let page = self
                    .client
                    .array_page(Operation::TopAdvertisers, &query, limit as usize + 1)
                    .await?;
                let truncated = !page.complete || page.items.len() > limit as usize;
                let mut pagination = json!({"hasMore":false});
                if truncated {
                    pagination["truncated"] = json!(true);
                }
                return Ok(
                    json!({"items":page.items.into_iter().take(limit as usize).collect::<Vec<_>>(),"pagination":pagination}),
                );
            }
            _ => unreachable!("checked by fields"),
        };
        self.client.request(operation, None, &query).await
    }
}
