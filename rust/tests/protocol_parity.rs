use beacon_mcp_rust::{client::BeaconClient, server};
use serde_json::{Value, json};
use std::time::Duration;
use tokio_util::sync::CancellationToken;

#[tokio::test]
async fn legacy_protocol_negotiates_without_session_and_does_not_filter_public_hosts() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let root = format!("http://{}/", listener.local_addr().unwrap());
    let shutdown = CancellationToken::new();
    let client = BeaconClient::new(
        "http://127.0.0.1:9/".parse().unwrap(),
        Duration::from_secs(1),
        Duration::from_secs(1),
    );
    let app = server::router(shutdown.clone(), client);
    let server = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    let http = reqwest::Client::new();
    for header in [("Host", "evil.example"), ("Origin", "https://evil.example")] {
        let response = http
            .get(format!("{root}healthz"))
            .header(header.0, header.1)
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), 200);
    }
    let legacy = http
        .post(&root)
        .header("Accept", "application/json, text/event-stream")
        .json(&json!({"jsonrpc":"2.0","id":1,"method":"tools/list"}))
        .send()
        .await
        .unwrap();
    assert_eq!(legacy.status(), 200);
    assert!(legacy.text().await.unwrap().contains("beacon_list_iatas"));
    for revision in [
        "2025-11-25",
        "2025-06-18",
        "2025-03-26",
        "2024-11-05",
        "2024-10-07",
        "1999-01-01",
    ] {
        let response = http.post(&root)
            .header("Accept", "application/json, text/event-stream")
            .header("MCP-Protocol-Version", revision)
            .json(&json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":revision,"capabilities":{},"clientInfo":{"name":"rust-parity","version":"1.0.0"}}}))
            .send().await.unwrap();
        assert_eq!(response.status(), 200, "{revision}");
        let payload: Value = response.json().await.unwrap();
        let expected = if revision == "1999-01-01" {
            "2025-11-25"
        } else {
            revision
        };
        assert_eq!(payload["result"]["protocolVersion"], expected, "{revision}");
        assert_eq!(payload["result"]["serverInfo"]["name"], "beacon-mcp");
        assert_eq!(payload["result"]["serverInfo"]["version"], "2.2.0");
    }
    for method in [reqwest::Method::GET, reqwest::Method::DELETE] {
        assert_eq!(
            http.request(method, &root).send().await.unwrap().status(),
            405
        );
    }
    shutdown.cancel();
    server.abort();
}
