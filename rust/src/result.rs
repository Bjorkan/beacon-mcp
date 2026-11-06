use crate::client::{ErrorCode, GatewayError};
use rmcp::model::{CallToolResult, ContentBlock};
use serde_json::{Value, json};

/// MCP structured results contain the data; the text block is only a short
/// confirmation, never a second copy of packet or message bodies.
pub fn success(value: Value) -> CallToolResult {
    let count = value.get("items").and_then(Value::as_array).map(Vec::len);
    let text = match count {
        Some(count) => format!("Beacon returned {count} item(s)."),
        None => "Beacon request succeeded.".into(),
    };
    let mut result = CallToolResult::structured(value);
    result.content = vec![ContentBlock::text(text)];
    result
}

pub fn error(error: GatewayError) -> CallToolResult {
    let kind = match (error.code, error.status.is_some()) {
        (ErrorCode::InvalidArgument, false) => "BeaconInputError",
        (ErrorCode::InvalidArgument, true) => "BeaconValidationError",
        (ErrorCode::NotFound, _) => "BeaconNotFoundError",
        (ErrorCode::DeadlineExceeded, _) => "BeaconTimeoutError",
        (ErrorCode::ResponseTooLarge, _) => "BeaconResponseTooLargeError",
        (ErrorCode::ResourceExhausted, _) => "BeaconRateLimitError",
        (ErrorCode::Unavailable, _) => "BeaconUpstreamError",
    };
    let mut details = json!({"code": error.code.as_str(), "type": kind});
    if let Some(status) = error.status {
        details["status"] = json!(status.as_u16());
    }
    if let Some(code) = error.upstream_code {
        details["upstreamCode"] = json!(code);
    }
    if let Some(retry_after) = error.retry_after {
        details["retryAfter"] = json!(retry_after);
    }
    let mut result = CallToolResult::structured_error(json!({"error": details}));
    result.content = vec![ContentBlock::text(error.message)];
    result
}
