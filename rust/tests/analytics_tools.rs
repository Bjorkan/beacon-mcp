use axum::{
    Router,
    body::Body,
    http::{Request, StatusCode},
    response::Response,
    routing::get,
};
use beacon_mcp_rust::{client::BeaconClient, server};
use serde_json::{Value, json};
use std::{
    sync::{Arc, Mutex},
    time::Duration,
};
use tokio_util::sync::CancellationToken;

const A: &str = "123e4567-e89b-42d3-a456-426614174000";
const B: &str = "123e4567-e89b-42d3-a456-426614174001";

async fn setup() -> (
    reqwest::Client,
    String,
    Arc<Mutex<Vec<String>>>,
    CancellationToken,
    Vec<tokio::task::JoinHandle<()>>,
) {
    let seen = Arc::new(Mutex::new(Vec::new()));
    let upstream = Router::new().fallback(get({
        let seen = seen.clone();
        move |request: Request<Body>| {
            let seen = seen.clone();
            async move {
                seen.lock().unwrap().push(request.uri().to_string());
                let (status, data) = match request.uri().path() {
                    "/api/v1/stats/top-advertisers" => {
                        (StatusCode::OK, json!([{"pubkey":"abc","flood":12}]))
                    }
                    "/api/v1/stats/overview" => (StatusCode::OK, json!({"totalPackets":10})),
                    "/api/v1/stats/series" => (StatusCode::OK, json!({"points":[{"packets":1}]})),
                    "/api/v1/stats/observer-comparison" => {
                        (StatusCode::OK, json!({"observerA":A,"observerB":B}))
                    }
                    _ => (StatusCode::NOT_FOUND, json!({"error":{"code":"not_found"}})),
                };
                Response::builder()
                    .status(status)
                    .body(Body::from(data.to_string()))
                    .unwrap()
            }
        }
    }));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let backend = format!("http://{}/", listener.local_addr().unwrap());
    let backend_task = tokio::spawn(async move { axum::serve(listener, upstream).await.unwrap() });
    let gateway_listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let root = format!("http://{}/", gateway_listener.local_addr().unwrap());
    let token = CancellationToken::new();
    let app = server::router(
        token.clone(),
        BeaconClient::new(
            backend.parse().unwrap(),
            Duration::from_secs(1),
            Duration::from_secs(1),
        ).expect("HTTP client configuration"),
    );
    let gateway_task =
        tokio::spawn(async move { axum::serve(gateway_listener, app).await.unwrap() });
    (
        reqwest::Client::new(),
        root,
        seen,
        token,
        vec![backend_task, gateway_task],
    )
}
async fn rpc(
    client: &reqwest::Client,
    root: &str,
    method: &str,
    name: Option<&str>,
    args: Value,
) -> Value {
    let mut request = client
        .post(root)
        .header("Accept", "application/json, text/event-stream")
        .header("MCP-Protocol-Version", "2026-07-28")
        .header("Mcp-Method", method);
    if let Some(name) = name {
        request = request.header("Mcp-Name", name);
    }
    let mut params = json!({"_meta":{
        "io.modelcontextprotocol/protocolVersion":"2026-07-28",
        "io.modelcontextprotocol/clientInfo":{"name":"rust-test","version":"1.0.0"},
        "io.modelcontextprotocol/clientCapabilities":{}
    }});
    if let Some(name) = name {
        params["name"] = json!(name);
        params["arguments"] = args;
    }
    let response = request
        .json(&json!({"jsonrpc":"2.0","id":1,"method":method,"params":params}))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    response.json().await.unwrap()
}

#[tokio::test]
async fn advertises_all_49_public_tools_and_calls_every_analytics_tool() {
    let (client, root, seen, token, tasks) = setup().await;
    let listed = rpc(&client, &root, "tools/list", None, json!({})).await;
    let expected_schemas: Value =
        serde_json::from_str(include_str!("../src/analytics-tool-definitions.json")).unwrap();
    for tool in listed["result"]["tools"].as_array().unwrap() {
        let name = tool["name"].as_str().unwrap();
        if let Some(expected) = expected_schemas.get(name) {
            assert_eq!(tool, expected, "{name} tool metadata");
        }
    }
    let mut names = listed["result"]["tools"]
        .as_array()
        .unwrap()
        .iter()
        .map(|item| item["name"].as_str().unwrap().to_owned())
        .collect::<Vec<_>>();
    let mut expected: Vec<String> =
        serde_json::from_str(include_str!("../../tests/fixtures/tool-names.json")).unwrap();
    names.sort();
    expected.sort();
    assert_eq!(names, expected);
    for (name, args, field) in [
        (
            "beacon_get_top_advertisers",
            json!({"sort":"flood","iatas":["arn"],"limit":1}),
            "items",
        ),
        (
            "beacon_get_network_overview",
            json!({"region":"sverige"}),
            "totalPackets",
        ),
        (
            "beacon_get_network_series",
            json!({"since":"2026-10-01T00:00:00Z","until":"2026-10-02T00:00:00Z"}),
            "points",
        ),
        (
            "beacon_compare_observers",
            json!({"observerA":A,"observerB":B,"since":"2026-10-01T00:00:00Z","until":"2026-10-02T00:00:00Z"}),
            "observerA",
        ),
    ] {
        let result = rpc(&client, &root, "tools/call", Some(name), args).await;
        assert_eq!(result["result"]["isError"], false, "{name}: {result}");
        if name == "beacon_get_top_advertisers" {
            assert_eq!(
                result["result"]["structuredContent"]["pagination"],
                json!({"hasMore":false})
            );
        }
        assert!(
            !result["result"]["structuredContent"][field].is_null(),
            "{name}: {result}"
        );
    }
    let paths = seen.lock().unwrap();
    assert!(
        paths
            .iter()
            .any(|uri| uri.contains("sort=flood") && uri.contains("iatas=ARN"))
    );
    assert!(
        paths
            .iter()
            .any(|uri| uri.contains("observerA=") && uri.contains("observerB="))
    );
    drop(paths);
    token.cancel();
    for task in tasks {
        task.abort();
    }
}

#[tokio::test]
async fn analytics_validation_fails_before_network() {
    let (client, root, seen, token, tasks) = setup().await;
    for (name, args) in [
        ("beacon_get_top_advertisers", json!({})),
        ("beacon_get_top_advertisers", json!({"sort":"volume"})),
        (
            "beacon_get_network_overview",
            json!({"region":"se","regionId":2}),
        ),
        (
            "beacon_get_network_series",
            json!({"until":"2026-10-01T00:00:00Z"}),
        ),
        (
            "beacon_compare_observers",
            json!({"observerA":A,"observerB":A,"since":"2026-10-01T00:00:00Z","until":"2026-10-02T00:00:00Z"}),
        ),
        (
            "beacon_compare_observers",
            json!({"observerA":"../admin","observerB":B,"since":"2026-10-01T00:00:00Z","until":"2026-10-02T00:00:00Z"}),
        ),
    ] {
        let response = rpc(&client, &root, "tools/call", Some(name), args).await;
        assert_eq!(response["result"]["isError"], true, "{name}: {response}");
        assert!(
            response["result"].get("structuredContent").is_none(),
            "{name}: {response}"
        );
    }
    // Reversed time windows are adapter-level calendar errors, so they keep
    // the structured ValidationError metadata the previous gateway returned.
    let response = rpc(
        &client,
        &root,
        "tools/call",
        Some("beacon_get_network_series"),
        json!({"since":"2026-10-02T00:00:00Z","until":"2026-10-01T00:00:00Z"}),
    )
    .await;
    assert_eq!(response["result"]["isError"], true, "{response}");
    assert_eq!(
        response["result"]["structuredContent"]["error"],
        json!({"code":"INVALID_ARGUMENT","type":"ValidationError"})
    );
    assert!(seen.lock().unwrap().is_empty());
    token.cancel();
    for task in tasks {
        task.abort();
    }
}
