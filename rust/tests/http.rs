use beacon_mcp_rust::server;
use tokio_util::sync::CancellationToken;

#[tokio::test]
async fn health_readiness_and_mcp_discovery() {
    let token = CancellationToken::new();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let server_token = token.clone();
    let task = tokio::spawn(async move {
        axum::serve(listener, server::router(server_token))
            .await
            .unwrap();
    });
    let client = reqwest::Client::new();
    let root = format!("http://{addr}");
    for path in ["healthz", "readyz"] {
        let response: serde_json::Value = client
            .get(format!("{root}/{path}"))
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
        assert_eq!(response["status"], "ok");
    }
    let discovery = client
        .post(format!("{root}/"))
        .header("Accept", "application/json, text/event-stream")
        .header("MCP-Protocol-Version", "2026-07-28")
        .header("Mcp-Method", "server/discover")
        .json(&serde_json::json!({
            "jsonrpc": "2.0", "id": 1, "method": "server/discover",
            "params": { "_meta": {
                "io.modelcontextprotocol/protocolVersion": "2026-07-28",
                "io.modelcontextprotocol/clientInfo": {"name": "rust-test", "version": "1.0.0"},
                "io.modelcontextprotocol/clientCapabilities": {}
            }}
        }))
        .send()
        .await
        .unwrap();
    assert!(
        discovery.status().is_success(),
        "{}",
        discovery.text().await.unwrap()
    );
    token.cancel();
    task.abort();
}
