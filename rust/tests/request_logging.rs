use serde_json::json;
use std::{
    net::TcpListener,
    process::{Command, Stdio},
    time::Duration,
};

#[tokio::test]
async fn info_logging_summarizes_each_mcp_request_without_sdk_noise_or_payloads() {
    let reservation = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = reservation.local_addr().unwrap().port();
    drop(reservation);

    let child = Command::new(env!("CARGO_BIN_EXE_beacon-mcp-rust"))
        .env("HOST", "127.0.0.1")
        .env("PORT", port.to_string())
        .env("BEACON_BASE_URL", "http://127.0.0.1:9/")
        .env("BEACON_SUBSCRIPTIONS_ENABLED", "false")
        .env("LOG_LEVEL", "info")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();

    let http = reqwest::Client::builder()
        .timeout(Duration::from_secs(3))
        .build()
        .unwrap();
    let root = format!("http://127.0.0.1:{port}/");
    let mut ready = false;
    for _ in 0..50 {
        if let Ok(response) = http.get(format!("{root}healthz")).send().await
            && response.status().is_success()
        {
            ready = true;
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }

    // Ensure we always reap the process, even if startup fails.
    if !ready {
        let mut child = child;
        let _ = child.kill();
        let output = child.wait_with_output().unwrap();
        panic!(
            "gateway did not become ready: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    let modern = http
        .post(&root)
        .header("Accept", "application/json, text/event-stream")
        .header("Mcp-Method", "tools/list")
        .header("MCP-Protocol-Version", "2026-07-28")
        .json(&json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "tools/list",
            "params": {"_meta": {
                "io.modelcontextprotocol/protocolVersion": "2026-07-28",
                "io.modelcontextprotocol/clientInfo": {"name": "log-test", "version": "1"},
                "io.modelcontextprotocol/clientCapabilities": {}
            }}
        }))
        .send()
        .await
        .unwrap();
    assert!(modern.status().is_success());

    // Legacy clients do not send the MCP-Method header. They still get a
    // single line, marked "unspecified", without inspecting request bodies.
    let legacy = http
        .post(&root)
        .header("Accept", "application/json, text/event-stream")
        .json(&json!({"jsonrpc":"2.0","id":2,"method":"tools/list"}))
        .send()
        .await
        .unwrap();
    assert!(legacy.status().is_success());

    let rejected = http
        .post(&root)
        .header("Accept", "application/json, text/event-stream")
        .header("Mcp-Method", "tools/call")
        .header("Mcp-Name", "beacon_search_nodes")
        .header("MCP-Protocol-Version", "2026-07-28")
        .json(&json!({
            "jsonrpc": "2.0",
            "id": 3,
            "method": "tools/call",
            "params": {
                "name": "beacon_search_nodes",
                "arguments": {
                    "limit": "invalid",
                    "name": "SENSITIVE_REQUEST_BODY_73681"
                },
                "_meta": {
                    "io.modelcontextprotocol/protocolVersion": "2026-07-28",
                    "io.modelcontextprotocol/clientInfo": {"name": "log-test", "version": "1"},
                    "io.modelcontextprotocol/clientCapabilities": {}
                }
            }
        }))
        .send()
        .await
        .unwrap();
    assert!(rejected.status().is_success());
    assert_eq!(
        rejected.json::<serde_json::Value>().await.unwrap()["result"]["isError"],
        true
    );

    let health = http.get(format!("{root}readyz")).send().await.unwrap();
    assert_eq!(health.status(), 200);

    let mut child = child;
    child.kill().unwrap();
    let output = child.wait_with_output().unwrap();
    // tracing-subscriber's default writer is stdout; keep stderr too so a
    // change of writer does not silently invalidate the assertions.
    let logs = format!(
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let summaries: Vec<_> = logs
        .lines()
        .filter(|line| line.contains("MCP request completed"))
        .collect();
    assert_eq!(summaries.len(), 3, "{logs}");
    assert!(
        summaries
            .iter()
            .any(|line| line.contains("mcp_method=tools/list") && line.contains("http_status=200")),
        "{logs}"
    );
    assert!(
        summaries
            .iter()
            .any(|line| line.contains("mcp_method=unspecified")),
        "{logs}"
    );
    assert!(
        summaries.iter().any(|line| {
            line.contains("mcp_method=tools/call")
                && line.contains("tool=beacon_search_nodes")
                && line.contains("duration_ms=")
        }),
        "{logs}"
    );
    assert!(!logs.contains("SENSITIVE_REQUEST_BODY_73681"), "{logs}");
    assert!(!logs.contains("Service initialized as server"), "{logs}");
    assert!(!logs.contains("serve finished"), "{logs}");
    assert!(!logs.contains("peer_info"), "{logs}");
}
