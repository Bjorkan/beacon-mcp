//! PR06 gateway tests: list/describe/execute, rejections with zero upstream calls.
//! Offline; mock upstream counts fetches to prove pre-network rejection.

use axum::{
    Router,
    body::Body,
    http::{Response, StatusCode},
    routing::get,
};
use beacon_api::client::BeaconClient;
use beacon_mcp::server;
use beacon_workflows::explore;
use serde_json::{Value, json};
use std::{
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    time::Duration,
};
use tokio_util::sync::CancellationToken;

const UUID: &str = "123e4567-e89b-42d3-a456-426614174000";

async fn mock_backend() -> (BeaconClient, Arc<AtomicUsize>, tokio::task::JoinHandle<()>) {
    let calls = Arc::new(AtomicUsize::new(0));
    let router = Router::new().fallback(get({
        let calls = calls.clone();
        move |request: axum::http::Request<Body>| {
            let calls = calls.clone();
            async move {
                calls.fetch_add(1, Ordering::SeqCst);
                let path = request.uri().path().to_owned();
                let data = if path.contains("/ws-ticket") {
                    json!({"ticket": "x"})
                } else {
                    json!({"raw": 123, "path": path})
                };
                Response::builder()
                    .status(StatusCode::OK)
                    .body(Body::from(data.to_string()))
                    .unwrap()
            }
        }
    }));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}/", listener.local_addr().unwrap());
    let task = tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
    (
        BeaconClient::new(
            base.parse().unwrap(),
            Duration::from_secs(2),
            Duration::from_secs(2),
        )
        .unwrap(),
        calls,
        task,
    )
}

fn args_for(key: &str) -> Value {
    match key {
        "server.info" | "broker.list" => json!({}),
        "iata.get" | "iata.border" => json!({"iata": "ARN"}),
        "region.get" => json!({"id": 3}),
        "scope.get" => json!({"scope": "public"}),
        "node.get" | "node.activity" | "node.neighbors" | "node.observations" | "node.poller"
        | "node.polling" | "node.telemetry" => {
            json!({"id": UUID, "range": "24h"})
        }
        "observer.get" | "observer.activity" | "observer.adverts" | "observer.telemetry" => {
            json!({"id": UUID})
        }
        "packet.get" => json!({"hash": "aabbccddeeff0011"}),
        "trace.get" => json!({"tag": "abcdef01"}),
        "channel.get" => json!({"id": 25}),
        "channel.messages" => json!({"id": 25}),
        "route.observations" => {
            json!({"iata": "ARN", "pathKey": "0123456789abcdef0123456789abcdef"})
        }
        _ => json!({"limit": 2}),
    }
}

#[tokio::test]
async fn all_53_reads_reachable_through_execute() {
    let (client, calls, task) = mock_backend().await;
    let mut reached = 0;
    for entry in beacon_api::registry::REGISTRY {
        let result = explore::execute(&client, entry.key, args_for(entry.key)).await;
        assert!(result.is_ok(), "{}: {:?}", entry.key, result.err());
        let value = result.unwrap();
        assert_eq!(value["key"], entry.key);
        assert_eq!(value["data"]["raw"], 123);
        reached += 1;
    }
    assert_eq!(reached, 53);
    assert!(calls.load(Ordering::SeqCst) >= 53);
    task.abort();
}

#[tokio::test]
async fn rejections_happen_before_network() {
    let (client, calls, task) = mock_backend().await;
    let before = calls.load(Ordering::SeqCst);
    assert!(
        explore::execute(&client, "no.such", json!({}))
            .await
            .is_err()
    );
    assert!(
        explore::execute(&client, "node.get", json!({"id": "../admin"}))
            .await
            .is_err()
    );
    // Prohibited: WsTicket never listable/describable/executable (not in registry).
    assert!(beacon_api::registry::find_by_key("ws.ticket").is_none());
    assert!(explore::describe("ws.ticket").is_err());
    assert!(explore::list(None, 51).is_err());
    assert_eq!(calls.load(Ordering::SeqCst), before);
    task.abort();
}

#[tokio::test]
async fn gateway_serves_through_mcp_with_legacy_catalog_retained() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let root = format!("http://{}/", listener.local_addr().unwrap());
    let token = CancellationToken::new();
    let (client, _, backend_task) = mock_backend().await;
    let app = server::router(token.clone(), client);
    let task = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    let http = reqwest::Client::new();
    let call = |name: &'static str, arguments: Value| {
        let http = http.clone();
        let root = root.clone();
        async move {
            let body: Value = http
                .post(&root)
                .header("Accept", "application/json, text/event-stream")
                .header("MCP-Protocol-Version", "2026-07-28")
                .header("Mcp-Method", "tools/call")
                .header("Mcp-Name", name)
                .json(&json!({"jsonrpc": "2.0", "id": 1, "method": "tools/call", "params": {
                    "name": name, "arguments": arguments, "_meta": {
                        "io.modelcontextprotocol/protocolVersion": "2026-07-28",
                        "io.modelcontextprotocol/clientInfo": {"name": "explore-test", "version": "1"},
                        "io.modelcontextprotocol/clientCapabilities": {}
                    }
                }}))
                .send()
                .await
                .unwrap()
                .json()
                .await
                .unwrap();
            body["result"].clone()
        }
    };
    // No-prerequisite direct execute first.
    let direct = call(
        "beacon_explore",
        json!({"mode": "execute", "operation": "server.info", "args": {}}),
    )
    .await;
    assert_eq!(direct["isError"], json!(false));
    // Legacy tool still present alongside gateway.
    let legacy = call("beacon_get_server_info", json!({})).await;
    assert_eq!(legacy["isError"], json!(false));
    token.cancel();
    task.abort();
    backend_task.abort();
}
