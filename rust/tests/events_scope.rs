use beacon_mcp_rust::{client::BeaconClient, server};
use serde_json::{Value, json};
use std::time::Duration;
use tokio_util::sync::CancellationToken;

async fn rpc(
    client: &reqwest::Client,
    root: &str,
    version: &str,
    method: &str,
    params: Value,
) -> Value {
    let response = client
        .post(root)
        .header("Accept", "application/json, text/event-stream")
        .header("MCP-Protocol-Version", version)
        .header("Mcp-Method", method)
        .json(&json!({"jsonrpc":"2.0","id":1,"method":method,"params":params}))
        .send()
        .await
        .unwrap();
    let status = response.status();
    let value: Value = response.json().await.unwrap();
    assert_eq!(
        status.as_u16(),
        if method == "events/subscribe" {
            404
        } else {
            200
        },
        "{method}: {value}"
    );
    value
}

#[tokio::test]
async fn public_gateway_does_not_advertise_or_accept_unimplemented_events() {
    let shutdown = CancellationToken::new();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let root = format!("http://{}/", listener.local_addr().unwrap());
    let client = BeaconClient::new(
        "http://127.0.0.1:9/".parse().unwrap(),
        Duration::from_secs(1),
        Duration::from_secs(1),
    ).expect("HTTP client configuration");
    let app = server::router(shutdown.clone(), client);
    let task = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    let http = reqwest::Client::new();
    let modern = rpc(
        &http,
        &root,
        "2026-07-28",
        "server/discover",
        json!({"_meta":{
            "io.modelcontextprotocol/protocolVersion":"2026-07-28",
            "io.modelcontextprotocol/clientInfo":{"name":"events-scope-test","version":"1"},
            "io.modelcontextprotocol/clientCapabilities":{}
        }}),
    )
    .await;
    assert!(
        modern["result"]["capabilities"]["events"].is_null(),
        "{modern}"
    );
    let legacy = rpc(
        &http,
        &root,
        "2025-11-25",
        "initialize",
        json!({"protocolVersion":"2025-11-25","capabilities":{},
            "clientInfo":{"name":"events-scope-test","version":"1"}}),
    )
    .await;
    assert!(
        legacy["result"]["capabilities"]["events"].is_null(),
        "{legacy}"
    );
    let subscribe = rpc(
        &http,
        &root,
        "2026-07-28",
        "events/subscribe",
        json!({"_meta":{
            "io.modelcontextprotocol/protocolVersion":"2026-07-28",
            "io.modelcontextprotocol/clientInfo":{"name":"events-scope-test","version":"1"},
            "io.modelcontextprotocol/clientCapabilities":{}
        }}),
    )
    .await;
    assert_eq!(subscribe["error"]["code"], -32601, "{subscribe}");
    shutdown.cancel();
    task.abort();
}
