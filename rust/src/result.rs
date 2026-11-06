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

/// Zod/tool-schema failures in the previous MCP server were plain tool errors,
/// unlike Beacon HTTP 400 responses, which retain structured error metadata.
pub fn invalid_input(name: &str, reason: &str) -> CallToolResult {
    let mut result = CallToolResult::default();
    result.is_error = Some(true);
    result.content = vec![ContentBlock::text(format!(
        "Input validation error: Invalid arguments for tool {name}: {reason}"
    ))];
    result
}

/// Invalid calendar dates and time-window order reach the TypeScript adapter
/// after schema validation; unlike schema failures they have classified metadata.
pub fn adapter_input_error(message: &str) -> CallToolResult {
    let mut result = CallToolResult::structured_error(json!({
        "error": {"code": "INVALID_ARGUMENT", "type": "ValidationError"}
    }));
    result.content = vec![ContentBlock::text(message)];
    result
}

#[cfg(test)]
#[test]
fn local_input_errors_are_not_upstream_errors() {
    let result =
        serde_json::to_value(invalid_input("beacon_get_packet", "hash: Required")).unwrap();
    assert_eq!(result["isError"], true);
    assert!(result.get("structuredContent").is_none());
    assert!(
        result["content"][0]["text"]
            .as_str()
            .unwrap()
            .contains("Input validation error:")
    );
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
