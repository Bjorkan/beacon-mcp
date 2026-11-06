//! One bounded, payload-free summary per HTTP request to the MCP endpoint.
//! Do not inspect request/response bodies: they can contain public packet or
//! message contents, and subscription responses may stream indefinitely.
use axum::{
    extract::Request,
    http::HeaderMap,
    middleware::Next,
    response::Response,
};
use std::time::Instant;

// MCP-Method and MCP-Name are supplied by the client. Never log arbitrary
// header values (or request paths, query strings, arguments, or identities).
fn safe_header<'a>(headers: &'a HeaderMap, name: &'static str) -> Option<&'a str> {
    let value = headers.get(name)?.to_str().ok()?;
    if value.is_empty()
        || value.len() > 80
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-' | b'/' | b'.'))
    {
        return None;
    }
    Some(value)
}

/// Summarize a request once the transport returns its HTTP response headers.
/// For a streaming listen request, latency is time to response headers, not
/// stream lifetime. A 200 may still contain a JSON-RPC tool-level error.
pub async fn summarize(request: Request, next: Next) -> Response {
    if request.uri().path() != "/" {
        // Health/readiness probes must not flood the MCP access log.
        return next.run(request).await;
    }
    let http_method = request.method().clone();
    let mcp_method = safe_header(request.headers(), "mcp-method")
        .unwrap_or("unspecified")
        .to_owned();
    let tool = if mcp_method == "tools/call" {
        safe_header(request.headers(), "mcp-name").unwrap_or("unspecified")
    } else {
        "-"
    }
    .to_owned();
    let start = Instant::now();
    let response = next.run(request).await;
    tracing::info!(
        target: "beacon_mcp_rust::request_logging",
        http_method = %http_method,
        mcp_method = %mcp_method,
        tool = %tool,
        http_status = response.status().as_u16(),
        duration_ms = start.elapsed().as_millis(),
        "MCP request completed"
    );
    response
}

#[cfg(test)]
mod tests {
    use super::safe_header;
    use axum::http::{HeaderMap, HeaderValue};

    #[test]
    fn accepts_protocol_metadata_but_not_free_form_header_content() {
        let mut headers = HeaderMap::new();
        headers.insert("mcp-method", HeaderValue::from_static("tools/call"));
        headers.insert("mcp-name", HeaderValue::from_static("beacon_search_packets"));
        assert_eq!(safe_header(&headers, "mcp-method"), Some("tools/call"));
        assert_eq!(
            safe_header(&headers, "mcp-name"),
            Some("beacon_search_packets")
        );

        headers.insert("mcp-name", HeaderValue::from_static("token=secret"));
        assert_eq!(safe_header(&headers, "mcp-name"), None);
        headers.insert(
            "mcp-name",
            HeaderValue::from_str(&"a".repeat(81)).unwrap(),
        );
        assert_eq!(safe_header(&headers, "mcp-name"), None);
    }
}
