//! Stateless bounded continuation tokens.
//! Opaque to callers; encode only validated operation + exact upstream
//! cursor(s) + filters + sort/order + pinned fixed windows. No host/path,
//! no authorization, no server-side state. Replaying the same token with the
//! same filters on another replica yields the same next page when upstream
//! state is unchanged.

use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::client::{ErrorCode, GatewayError};

pub const MAX_TOKEN_BYTES: usize = 2048;
pub const TOKEN_VERSION: u8 = 1;

#[derive(Serialize, Deserialize, Debug, PartialEq)]
struct TokenBody {
    v: u8,
    op: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    cursor: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    page_cursor: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    cursor_id: Option<String>,
    filters: Value,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    sort: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    order: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    since: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    until: Option<String>,
}

fn invalid(message: impl Into<String>) -> GatewayError {
    GatewayError::new(ErrorCode::InvalidArgument, message)
}

#[allow(clippy::too_many_arguments)]
pub fn encode(
    operation_key: &str,
    cursor: Option<&str>,
    page_cursor: Option<&str>,
    cursor_id: Option<&str>,
    filters: &Value,
    sort: Option<&str>,
    order: Option<&str>,
    since: Option<&str>,
    until: Option<&str>,
) -> Result<String, GatewayError> {
    if operation_key.is_empty() || operation_key.len() > 64 {
        return Err(invalid("invalid operation key"));
    }
    for label in [cursor, page_cursor, cursor_id, sort, order, since, until]
        .into_iter()
        .flatten()
    {
        if label.is_empty() || label.len() > 512 || label.chars().any(char::is_control) {
            return Err(invalid("invalid token field"));
        }
    }
    let filters_text = serde_json::to_string(filters).map_err(|_| invalid("invalid filters"))?;
    if filters_text.len() > 1024 {
        return Err(invalid("filters too large"));
    }
    let body = TokenBody {
        v: TOKEN_VERSION,
        op: operation_key.to_owned(),
        cursor: cursor.map(str::to_owned),
        page_cursor: page_cursor.map(str::to_owned),
        cursor_id: cursor_id.map(str::to_owned),
        filters: filters.clone(),
        sort: sort.map(str::to_owned),
        order: order.map(str::to_owned),
        since: since.map(str::to_owned),
        until: until.map(str::to_owned),
    };
    let json = serde_json::to_string(&body).map_err(|_| invalid("invalid token"))?;
    if json.len() > MAX_TOKEN_BYTES {
        return Err(invalid("continuation too large"));
    }
    Ok(URL_SAFE_NO_PAD.encode(json.as_bytes()))
}

pub struct Decoded {
    pub operation_key: String,
    pub cursor: Option<String>,
    pub page_cursor: Option<String>,
    pub cursor_id: Option<String>,
    pub filters: Value,
    pub sort: Option<String>,
    pub order: Option<String>,
    pub since: Option<String>,
    pub until: Option<String>,
}

pub fn decode(token: &str, expected_operation_key: &str) -> Result<Decoded, GatewayError> {
    if token.is_empty() || token.len() > 4096 {
        return Err(invalid("invalid continuation token"));
    }
    let bytes = URL_SAFE_NO_PAD
        .decode(token)
        .map_err(|_| invalid("invalid continuation token"))?;
    if bytes.len() > MAX_TOKEN_BYTES {
        return Err(invalid("continuation too large"));
    }
    let body: TokenBody =
        serde_json::from_slice(&bytes).map_err(|_| invalid("invalid continuation token"))?;
    if body.v != TOKEN_VERSION {
        return Err(invalid("unsupported continuation version"));
    }
    if body.op != expected_operation_key {
        return Err(invalid("continuation operation mismatch"));
    }
    Ok(Decoded {
        operation_key: body.op,
        cursor: body.cursor,
        page_cursor: body.page_cursor,
        cursor_id: body.cursor_id,
        filters: body.filters,
        sort: body.sort,
        order: body.order,
        since: body.since,
        until: body.until,
    })
}

#[cfg(test)]
mod tests {
    use super::{decode, encode};
    use serde_json::json;

    #[test]
    fn round_trip_preserves_exact_cursors_and_filters() {
        let filters = json!({"iata":"ARN","limit":20});
        let token = encode(
            "node.observations",
            Some("42"),
            Some("s1:name:uuid"),
            None,
            &filters,
            Some("lastSeen"),
            Some("desc"),
            Some("2026-10-01T00:00:00Z"),
            Some("2026-10-02T00:00:00Z"),
        )
        .unwrap();
        let decoded = decode(&token, "node.observations").unwrap();
        assert_eq!(decoded.cursor.as_deref(), Some("42"));
        assert_eq!(decoded.page_cursor.as_deref(), Some("s1:name:uuid"));
        assert_eq!(decoded.filters, filters);
    }
}
