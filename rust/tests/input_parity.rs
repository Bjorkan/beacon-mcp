use beacon_mcp_rust::{client::BeaconClient, server};
use serde_json::{Value, json};
use std::time::Duration;
use tokio_util::sync::CancellationToken;

/// Frozen negative results from the previous TypeScript MCP HTTP handler.
/// No production data or upstream network request is used by these cases.
#[tokio::test]
async fn negative_input_results_match_previous_gateway() {
    let cases: Vec<Value> =
        serde_json::from_str(include_str!("fixtures/invalid-tool-cases.json")).unwrap();
    assert_eq!(cases.len(), 30);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let root = format!("http://{}/", listener.local_addr().unwrap());
    let token = CancellationToken::new();
    let client = BeaconClient::new(
        "http://127.0.0.1:9/".parse().unwrap(),
        Duration::from_millis(100),
        Duration::from_millis(100),
    );
    let app = server::router(token.clone(), client);
    let task = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    let http = reqwest::Client::new();
    for (index, case) in cases.iter().enumerate() {
        let name = case["name"].as_str().unwrap();
        let body: Value = http.post(&root)
            .header("Accept", "application/json, text/event-stream")
            .header("MCP-Protocol-Version", "2026-07-28")
            .header("Mcp-Method", "tools/call")
            .header("Mcp-Name", name)
            .json(&json!({"jsonrpc":"2.0", "id":index + 1, "method":"tools/call", "params":{
                "name":name,"arguments":case["arguments"],"_meta":{
                    "io.modelcontextprotocol/protocolVersion":"2026-07-28",
                    "io.modelcontextprotocol/clientInfo":{"name":"negative-parity","version":"1"},
                    "io.modelcontextprotocol/clientCapabilities":{}
                }
            }})).send().await.unwrap().json().await.unwrap();
        let result = &body["result"];
        assert_eq!(result["isError"], true, "{name}: {index}");
        assert_eq!(
            result["structuredContent"]["error"], case["error"],
            "{name}: {index}"
        );
        assert_eq!(
            result["content"][0]["text"], case["text"],
            "{name}: {index}"
        );
    }
    token.cancel();
    task.abort();
}
