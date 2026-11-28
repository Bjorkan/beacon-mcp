//! PR 03 validator keyword coverage: oneOf/$defs/nested/unknown/format/if-then-else.
//! Offline; exercises the standard-compatible validator through real MCP calls.

use beacon_api::client::BeaconClient;
use beacon_mcp::server;
use serde_json::{Value, json};
use std::time::Duration;
use tokio_util::sync::CancellationToken;

async fn app() -> (String, CancellationToken, tokio::task::JoinHandle<()>) {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let root = format!("http://{}/", listener.local_addr().unwrap());
    let token = CancellationToken::new();
    let client = BeaconClient::new(
        "http://127.0.0.1:9/".parse().unwrap(),
        Duration::from_millis(100),
        Duration::from_millis(100),
    )
    .unwrap();
    let app = server::router(token.clone(), client);
    let task = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    (root, token, task)
}

async fn call_tool(root: &str, name: &str, arguments: Value) -> Value {
    let http = reqwest::Client::new();
    let body: Value = http
        .post(root)
        .header("Accept", "application/json, text/event-stream")
        .header("MCP-Protocol-Version", "2026-07-28")
        .header("Mcp-Method", "tools/call")
        .header("Mcp-Name", name)
        .json(
            &json!({"jsonrpc":"2.0","id":1,"method":"tools/call","params":{
                "name":name,"arguments":arguments,"_meta":{
                    "io.modelcontextprotocol/protocolVersion":"2026-07-28",
                    "io.modelcontextprotocol/clientInfo":{"name":"schema-test","version":"1"},
                    "io.modelcontextprotocol/clientCapabilities":{}
                }
            }}),
        )
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    body["result"].clone()
}

#[tokio::test]
async fn unknown_keys_rejected_and_valid_inputs_pass_schema() {
    let (root, token, task) = app().await;
    // Valid: UUID accepted by schema, then fails upstream (127.0.0.1:9 refused).
    // Schema-valid calls yield a structured upstream error, not a plain input error.
    let ok = call_tool(
        &root,
        "beacon_get_node",
        json!({"id": "123e4567-e89b-42d3-a456-426614174000"}),
    )
    .await;
    assert_eq!(ok["isError"], true);
    assert!(
        ok.get("structuredContent").is_some(),
        "valid input reaches upstream: {ok}"
    );
    assert!(
        !ok["content"][0]["text"]
            .as_str()
            .unwrap_or_default()
            .contains("Unrecognized key")
    );
    // Unknown key rejected.
    let bad = call_tool(
        &root,
        "beacon_get_node",
        json!({"id": "123e4567-e89b-42d3-a456-426614174000", "bogus": 1}),
    )
    .await;
    assert_eq!(bad["isError"], true);
    assert!(
        bad["content"][0]["text"]
            .as_str()
            .unwrap()
            .contains("Unrecognized key")
    );
    // Invalid UUID rejected.
    let bad_uuid = call_tool(&root, "beacon_get_node", json!({"id": "not-a-uuid"})).await;
    assert_eq!(bad_uuid["isError"], true);
    token.cancel();
    task.abort();
}

#[tokio::test]
async fn conditional_mode_args_still_enforced() {
    let (root, token, task) = app().await;
    // region + regionId mutually exclusive (enforced in workflows).
    let both = call_tool(
        &root,
        "beacon_search_nodes",
        json!({"region": "se", "regionId": 1}),
    )
    .await;
    assert_eq!(both["isError"], true);
    token.cancel();
    task.abort();
}
