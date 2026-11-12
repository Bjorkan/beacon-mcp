use chrono::DateTime;
use rmcp::model::{JsonObject, Tool};
use serde::Deserialize;
use serde_json::Value;

use crate::{
    client::{BeaconClient, ErrorCode, GatewayError, Operation},
    tools::page,
};

const NAMES: [&str; 5] = [
    "beacon_search_packets",
    "beacon_get_packet",
    "beacon_search_messages",
    "beacon_list_channels",
    "beacon_get_channel_messages",
];

#[derive(Clone)]
pub struct PacketTools {
    client: BeaconClient,
}

#[derive(Default, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Args {
    hash: Option<String>,
    limit: Option<u32>,
    cursor: Option<u64>,
    page_cursor: Option<String>,
    iatas: Option<Vec<String>>,
    iata: Option<String>,
    region: Option<String>,
    region_id: Option<u32>,
    scope: Option<String>,
    scopes: Option<Vec<String>>,
    since: Option<String>,
    until: Option<String>,
    payload_type: Option<u16>,
    payload_types: Option<Vec<u16>>,
    payload_type_name: Option<String>,
    route_type: Option<u8>,
    route_types: Option<Vec<u8>>,
    hash_prefix: Option<String>,
    channel_id: Option<u32>,
    channel_hash: Option<String>,
    key_known: Option<bool>,
}

fn fields(name: &str) -> Option<&'static [&'static str]> {
    Some(match name {
        "beacon_search_packets" => &[
            "limit",
            "cursor",
            "pageCursor",
            "iata",
            "iatas",
            "region",
            "regionId",
            "scope",
            "scopes",
            "since",
            "until",
            "payloadType",
            "payloadTypes",
            "payloadTypeName",
            "routeType",
            "routeTypes",
            "hashPrefix",
        ],
        "beacon_get_packet" => &["hash"],
        "beacon_search_messages" => &[
            "limit",
            "cursor",
            "iatas",
            "scope",
            "since",
            "channelId",
            "channelHash",
        ],
        "beacon_list_channels" => {
            &["limit", "cursor", "pageCursor", "iata", "iatas", "hash", "keyKnown"]
        },
        "beacon_get_channel_messages" => {
            &["channelId", "limit", "cursor", "iatas", "scope", "since"]
        }
        _ => return None,
    })
}

fn invalid(message: impl Into<String>) -> GatewayError {
    GatewayError::new(ErrorCode::InvalidArgument, message)
}

fn hex(text: &str, min: usize, max: usize) -> bool {
    (min..=max).contains(&text.len()) && text.bytes().all(|byte| byte.is_ascii_hexdigit())
}

pub(crate) fn timestamp(text: &str) -> Result<i64, GatewayError> {
    if text.len() > 24 || !text.ends_with('Z') || !text.contains('T') {
        return Err(invalid("timestamp must be RFC3339 UTC"));
    }
    let parsed =
        DateTime::parse_from_rfc3339(text).map_err(|_| invalid("invalid UTC calendar date"))?;
    if parsed.offset().local_minus_utc() != 0 {
        return Err(invalid("timestamp must be UTC"));
    }
    Ok(parsed.timestamp_millis())
}

fn payload_types(name: &str) -> Option<&'static [u16]> {
    Some(match name {
        "request" => &[0],
        "response" => &[1],
        "text_message" | "txt_msg" => &[2],
        "acknowledgement" => &[3],
        "advert" => &[4],
        "group_text" | "grp_txt" => &[5],
        "group_data" => &[6],
        "anonymous_request" | "anon_req" => &[7],
        "path" => &[8],
        "trace" => &[9],
        "multipart" => &[10],
        "control" => &[11],
        "reserved" => &[12, 13, 14],
        "raw_custom" => &[15],
        _ => return None,
    })
}

impl PacketTools {
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
        crate::tools::tool_definition(include_str!("packet-tool-definitions.json"), name)
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
        if args.region.is_some() && args.region_id.is_some()
            || args
                .region_id
                .is_some_and(|id| id == 0 || id > i32::MAX as u32)
        {
            return Err(invalid("invalid region filter"));
        }
        if args.iata.is_some() && args.iatas.is_some() {
            return Err(invalid("iata and iatas are mutually exclusive"));
        }
        if name == "beacon_list_channels"
            && args.cursor.is_some_and(|cursor| cursor > 0)
            && args.page_cursor.is_some()
        {
            return Err(invalid("positive cursor and pageCursor cannot be combined"));
        }
        if args.hash_prefix.is_some() && args.page_cursor.is_some() {
            return Err(invalid("hashPrefix pages with cursor, not pageCursor"));
        }
        if args.payload_type_name.is_some()
            && (args.payload_type.is_some() || args.payload_types.is_some())
        {
            return Err(invalid(
                "payloadTypeName cannot be combined with payloadType or payloadTypes",
            ));
        }
        if args.scope.is_some() && args.scopes.is_some()
            || args.payload_type.is_some()
                && (args.payload_types.is_some() || args.payload_type_name.is_some())
            || args.route_type.is_some() && args.route_types.is_some()
            || args.channel_id.is_some() && args.channel_hash.is_some()
        {
            return Err(invalid("mutually exclusive filters"));
        }
        if args.payload_type.is_some_and(|value| value > 32767)
            || args.route_type.is_some_and(|value| value > 3)
        {
            return Err(invalid("invalid type filter"));
        }
        if args.page_cursor.as_deref().is_some_and(|value| {
            value.is_empty()
                || value.len()
                    > if name == "beacon_list_channels" {
                        64
                    } else {
                        512
                    }
        }) {
            return Err(invalid("invalid pageCursor"));
        }
        let mut query = Vec::new();
        let mut put = |key, value: Option<String>| {
            if let Some(value) = value {
                query.push((key, value));
            }
        };
        if let Some(iata) = args.iata {
            let normalized = iata.trim().to_ascii_uppercase();
            if normalized.len() != 3
                || !normalized.bytes().all(|byte| byte.is_ascii_alphanumeric())
            {
                return Err(invalid("invalid IATA code"));
            }
            put("iata", Some(normalized));
        }
        if let Some(iatas) = args.iatas {
            if iatas.is_empty()
                || iatas.len() > 20
                || iatas.iter().any(|iata| {
                    let iata = iata.trim();
                    iata.len() != 3 || !iata.bytes().all(|byte| byte.is_ascii_alphanumeric())
                })
            {
                return Err(invalid("invalid IATA codes"));
            }
            put(
                "iatas",
                Some(
                    iatas
                        .iter()
                        .map(|iata| iata.trim().to_ascii_uppercase())
                        .collect::<Vec<_>>()
                        .join(","),
                ),
            );
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
        if let Some(scopes) = args.scopes {
            if scopes.is_empty()
                || scopes.len() > 20
                || scopes
                    .iter()
                    .any(|text| text.trim().is_empty() || text.len() > 128 || text.contains(','))
            {
                return Err(invalid("invalid scopes"));
            }
            put("scopes", Some(scopes.join(",")));
        }
        let since = args.since.as_deref().map(timestamp).transpose()?;
        let until = args.until.as_deref().map(timestamp).transpose()?;
        if since.zip(until).is_some_and(|(start, end)| start >= end) {
            return Err(invalid("since must be before until"));
        }
        put("since", since.map(|value| value.to_string()));
        put("until", until.map(|value| value.to_string()));
        put(
            "payloadType",
            args.payload_type.map(|value| value.to_string()),
        );
        if let Some(types) = args.payload_types {
            if types.is_empty() || types.len() > 20 || types.iter().any(|value| *value > 32767) {
                return Err(invalid("invalid payloadTypes"));
            }
            put(
                "payloadTypes",
                Some(
                    types
                        .iter()
                        .map(u16::to_string)
                        .collect::<Vec<_>>()
                        .join(","),
                ),
            );
        }
        if let Some(name) = args.payload_type_name {
            let types = payload_types(&name).ok_or_else(|| invalid("unknown payload type name"))?;
            put(
                "payloadTypes",
                Some(
                    types
                        .iter()
                        .map(u16::to_string)
                        .collect::<Vec<_>>()
                        .join(","),
                ),
            );
        }
        put("routeType", args.route_type.map(|value| value.to_string()));
        if let Some(types) = args.route_types {
            if types.is_empty() || types.len() > 20 || types.iter().any(|value| *value > 3) {
                return Err(invalid("invalid routeTypes"));
            }
            put(
                "routeTypes",
                Some(
                    types
                        .iter()
                        .map(u8::to_string)
                        .collect::<Vec<_>>()
                        .join(","),
                ),
            );
        }
        let hash_search = args.hash_prefix.is_some();
        if let Some(prefix) = args.hash_prefix {
            if !hex(&prefix, 4, 16) {
                return Err(invalid("invalid hashPrefix"));
            }
            put("hashPrefix", Some(prefix));
        }
        let channel_id = args.channel_id;
        if channel_id.is_some_and(|id| id == 0 || id > i32::MAX as u32) {
            return Err(invalid("invalid channelId"));
        }
        if name == "beacon_search_messages" {
            put("channelID", channel_id.map(|id| id.to_string()));
        }
        if let Some(hash) = args.channel_hash {
            if !hex(&hash, 2, 2) {
                return Err(invalid("invalid channelHash"));
            }
            put("channelHash", Some(hash));
        }
        if name == "beacon_list_channels" {
            if args.hash.as_deref().is_some_and(|hash| !hex(hash, 2, 2)) {
                return Err(invalid("invalid channel hash"));
            }
            put("hash", args.hash.clone());
            put("keyKnown", args.key_known.map(|value| value.to_string()));
        }
        put("cursor", args.cursor.map(|value| value.to_string()));
        put("pageCursor", args.page_cursor);
        let op = match name {
            "beacon_search_packets" => Operation::Packets,
            "beacon_get_packet" => Operation::Packet,
            "beacon_search_messages" => Operation::Messages,
            "beacon_list_channels" => Operation::Channels,
            "beacon_get_channel_messages" => Operation::ChannelMessages,
            _ => unreachable!("checked by fields"),
        };
        if name == "beacon_get_packet" {
            return self.client.request(op, args.hash.as_deref(), &query).await;
        }
        if name == "beacon_get_channel_messages" {
            let id = channel_id
                .ok_or_else(|| invalid("channelId is required"))?
                .to_string();
            put("limit", Some(limit.to_string()));
            return page(
                self.client.request(op, Some(&id), &query).await?,
                limit as usize,
                false,
            );
        }
        put("limit", Some(limit.to_string()));
        let response = self.client.request(op, None, &query).await?;
        let mut result = page(response, limit as usize, false)?;
        if name == "beacon_list_channels"
            && result["pagination"]["nextCursor"]["pageCursor"].is_string()
        {
            result["pagination"]["nextCursor"]
                .as_object_mut()
                .map(|cursor| cursor.remove("cursor"));
        }
        if name == "beacon_search_packets" && hash_search {
            result["pagination"]["nextCursor"]
                .as_object_mut()
                .map(|cursor| cursor.remove("pageCursor"));
        }
        if result["pagination"]["nextCursor"]
            .as_object()
            .is_some_and(serde_json::Map::is_empty)
        {
            result["pagination"]
                .as_object_mut()
                .unwrap()
                .remove("nextCursor");
        }
        Ok(result)
    }
}
