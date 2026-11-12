use axum::{
    Router,
    body::Body,
    http::{Request, StatusCode},
    response::Response,
    routing::get,
};
use beacon_mcp_rust::{client::BeaconClient, composite_tools::CompositeTools, server};
use serde_json::{Value, json};
use std::{
    sync::{Arc, Mutex},
    time::Duration,
};
use tokio_util::sync::CancellationToken;

const NODE: &str = "123e4567-e89b-42d3-a456-426614174000";

async fn mock() -> (
    CompositeTools,
    Arc<Mutex<Vec<String>>>,
    tokio::task::JoinHandle<()>,
) {
    let paths = Arc::new(Mutex::new(Vec::new()));
    let app = Router::new().fallback(get({
        let paths = paths.clone();
        move |request: Request<Body>| {
            let paths = paths.clone();
            async move {
                paths.lock().unwrap().push(request.uri().to_string());
                let response = match request.uri().path() {
                    path if path == format!("/api/v1/nodes/{NODE}") => {
                        json!({"id": NODE, "name":"station"})
                    }
                    path if path == format!("/api/v1/nodes/{NODE}/activity") => {
                        json!({"points":[{"t":123,"observations":4}]})
                    }
                    path if path == format!("/api/v1/nodes/{NODE}/neighbors") => {
                        json!([{"id":1},{"id":2},{"id":3}])
                    }
                    "/api/v1/stats/overview" => json!({"totalPackets":12}),
                    _ => {
                        return Response::builder()
                            .status(StatusCode::NOT_FOUND)
                            .body(Body::from("{}"))
                            .unwrap();
                    }
                };
                Response::builder()
                    .status(StatusCode::OK)
                    .body(Body::from(response.to_string()))
                    .unwrap()
            }
        }
    }));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}/", listener.local_addr().unwrap());
    let task = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    let client = BeaconClient::new(
        url.parse().unwrap(),
        Duration::from_secs(2),
        Duration::from_secs(2),
    )
    .expect("HTTP client");
    (CompositeTools::new(client), paths, task)
}

fn args(value: Value) -> Option<serde_json::Map<String, Value>> {
    Some(value.as_object().unwrap().clone())
}

#[tokio::test]
async fn diagnostic_and_region_comparison_are_bounded_and_read_only() {
    let (tools, seen, task) = mock().await;
    let result = tools
        .execute(
            "beacon_diagnose_node",
            args(json!({"id":NODE,"range":"48h","neighborLimit":2})),
        )
        .await
        .unwrap();
    assert_eq!(result["node"]["name"], "station");
    assert_eq!(result["neighbors"].as_array().unwrap().len(), 2);
    assert_eq!(result["neighborsTruncated"], true);
    assert_eq!(result["requestedRange"], "48h");
    let result = tools
        .execute(
            "beacon_compare_regions",
            args(json!({"iataA":"arn","iataB":"mmx"})),
        )
        .await
        .unwrap();
    assert_eq!(result["regionA"]["iata"], "ARN");
    assert_eq!(result["regionB"]["iata"], "MMX");
    let paths = seen.lock().unwrap();
    assert_eq!(paths.len(), 5);
    assert!(paths.iter().all(|url| url.starts_with("/api/v1/")));
    assert!(paths.iter().any(|url| url.contains("range=48h")));
    assert!(paths.iter().any(|url| url.contains("iatas=ARN")));
    assert!(paths.iter().any(|url| url.contains("iatas=MMX")));
    drop(paths);
    task.abort();
}

#[tokio::test]
async fn composite_inputs_are_checked_before_network() {
    let (tools, seen, task) = mock().await;
    for (name, input) in [
        ("beacon_diagnose_node", json!({"id":"../admin"})),
        (
            "beacon_diagnose_node",
            json!({"id":NODE,"neighborLimit":21}),
        ),
        ("beacon_diagnose_node", json!({"id":NODE,"range":"800h"})),
        (
            "beacon_compare_regions",
            json!({"iataA":"arn","iataB":"arn"}),
        ),
        (
            "beacon_compare_regions",
            json!({"iataA":"ARN/..","iataB":"MMX"}),
        ),
        (
            "beacon_compare_regions",
            json!({"iataA":"ARN","iataB":"MMX","unexpected":"x"}),
        ),
    ] {
        assert!(tools.execute(name, args(input)).await.is_err(), "{name}");
    }
    assert!(seen.lock().unwrap().is_empty());
    task.abort();
}

#[tokio::test]
async fn prompts_are_discoverable_and_render_without_upstream_calls() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let root = format!("http://{}/", listener.local_addr().unwrap());
    let shutdown = CancellationToken::new();
    let client = BeaconClient::new(
        "http://127.0.0.1:9/".parse().unwrap(),
        Duration::from_millis(100),
        Duration::from_millis(100),
    )
    .expect("HTTP client");
    let task = tokio::spawn({
        let app = server::router(shutdown.clone(), client);
        async move { axum::serve(listener, app).await.unwrap() }
    });
    let http = reqwest::Client::new();
    let call = |method: &str, mut params: Value| {
        params["_meta"] = json!({
            "io.modelcontextprotocol/protocolVersion":"2026-07-28",
            "io.modelcontextprotocol/clientInfo":{"name":"composite-test","version":"1"},
            "io.modelcontextprotocol/clientCapabilities":{}
        });
        let mut request = http
            .post(&root)
            .header("Accept", "application/json, text/event-stream")
            .header("MCP-Protocol-Version", "2026-07-28")
            .header("Mcp-Method", method);
        if let Some(name) = params.get("name").and_then(Value::as_str) {
            request = request.header("Mcp-Name", name);
        }
        request.json(&json!({"jsonrpc":"2.0","id":1,"method":method,"params":params}))
    };
    let list: Value = call("prompts/list", json!({}))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(
        list["result"]["prompts"].as_array().unwrap().len(),
        3,
        "{list}"
    );
    for (name, arguments) in [
        ("beacon_node_diagnostics", json!({"id":NODE})),
        (
            "beacon_region_comparison",
            json!({"iataA":"arn","iataB":"mmx"}),
        ),
        (
            "beacon_route_review",
            json!({"iata":"arn","pathKey":"0123456789abcdef0123456789abcdef"}),
        ),
    ] {
        let reply: Value = call("prompts/get", json!({"name":name,"arguments":arguments}))
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
        assert_eq!(reply["result"]["messages"][0]["role"], "user", "{reply}");
        assert_eq!(reply["result"]["messages"][0]["content"]["type"], "text");
    }
    let bad: Value = call(
        "prompts/get",
        json!({"name":"beacon_route_review","arguments":{"iata":"..","pathKey":"x"}}),
    )
    .send()
    .await
    .unwrap()
    .json()
    .await
    .unwrap();
    assert!(bad["error"].is_object(), "{bad}");
    shutdown.cancel();
    task.abort();
}
