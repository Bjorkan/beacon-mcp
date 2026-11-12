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
                    "/api/v1/routes" => (
                        StatusCode::OK,
                        json!([{"id":7,"lastSeen":1000,"hopCount":2}]),
                    ),
                    "/api/v1/routes/search" => {
                        (StatusCode::OK, json!([{"id":8,"hops":["ab","cd"]}]))
                    }
                    "/api/v1/routes/cross" => (StatusCode::OK, json!([{"id":9,"iata":"ARN"}])),
                    "/api/v1/traces" => (
                        StatusCode::OK,
                        json!([{"traceTag":"aabbccdd","lastHeardAt":123}]),
                    ),
                    "/api/v1/traces/aabbccdd" => {
                        (StatusCode::OK, json!({"traceTag":"aabbccdd","type":"PING"}))
                    }
                    _ => (
                        StatusCode::NOT_FOUND,
                        json!({"error":{"code":"not_found","message":"missing"}}),
                    ),
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
async fn calls_all_five_route_and_trace_tools_with_compound_cursors() {
    let (client, root, seen, token, tasks) = setup().await;
    let listed = rpc(&client, &root, "tools/list", None, json!({})).await;
    let expected_schemas: Value =
        serde_json::from_str(include_str!("../src/routing-tool-definitions.json")).unwrap();
    for tool in listed["result"]["tools"].as_array().unwrap() {
        let name = tool["name"].as_str().unwrap();
        if let Some(expected) = expected_schemas.get(name) {
            assert_eq!(tool, expected, "{name} tool metadata");
        }
    }
    let names = listed["result"]["tools"]
        .as_array()
        .unwrap()
        .iter()
        .map(|item| item["name"].as_str().unwrap())
        .collect::<Vec<_>>();
    for (name, args, field) in [
        (
            "beacon_list_routes",
            json!({"iatas":["arn","mmx"],"limit":1,"cursor":500,"cursorId":5}),
            "items",
        ),
        (
            "beacon_search_routes",
            json!({"from":"AB","to":"cd","limit":1}),
            "items",
        ),
        (
            "beacon_find_cross_iata_routes",
            json!({"fromHash":"ab","toHash":"cd","fromIata":"arn","toIata":"mmx","limit":1}),
            "items",
        ),
        (
            "beacon_search_traces",
            json!({"type":"PING","since":"2026-10-01T00:00:00Z","until":"2026-10-02T00:00:00Z","cursor":500,"cursorTag":"aabbccdd","limit":1}),
            "items",
        ),
        ("beacon_get_trace", json!({"tag":"aabbccdd"}), "traceTag"),
    ] {
        assert!(names.contains(&name), "{name}");
        let result = rpc(&client, &root, "tools/call", Some(name), args).await;
        assert_eq!(result["result"]["isError"], false, "{name}: {result}");
        if matches!(
            name,
            "beacon_search_routes" | "beacon_find_cross_iata_routes"
        ) {
            assert_eq!(
                result["result"]["structuredContent"]["pagination"],
                json!({"hasMore":false})
            );
        }
        assert!(
            !result["result"]["structuredContent"][field].is_null(),
            "{name}: {result}"
        );
        if name == "beacon_list_routes" {
            assert_eq!(
                result["result"]["structuredContent"]["pagination"]["nextCursor"],
                json!({"cursor":1000,"cursorId":7})
            );
        }
        if name == "beacon_search_traces" {
            assert_eq!(
                result["result"]["structuredContent"]["pagination"]["nextCursor"],
                json!({"cursor":123,"cursorTag":"aabbccdd"})
            );
        }
    }
    let paths = seen.lock().unwrap();
    assert!(
        paths
            .iter()
            .any(|uri| uri.contains("iatas=ARN%2CMMX") && uri.contains("cursorId=5"))
    );
    assert!(
        paths
            .iter()
            .any(|uri| uri.contains("from=ab") && uri.contains("to=cd"))
    );
    drop(paths);
    token.cancel();
    for task in tasks {
        task.abort();
    }
}

#[tokio::test]
async fn invalid_filters_never_reach_beacon_and_unknown_tags_report_not_found() {
    let (client, root, seen, token, tasks) = setup().await;
    for (name, args) in [
        ("beacon_list_routes", json!({"cursorId":7})),
        ("beacon_list_routes", json!({"iata":"ARN","iatas":["MMX"]})),
        ("beacon_search_routes", json!({"from":"a","to":"b"})),
        (
            "beacon_find_cross_iata_routes",
            json!({"fromHash":"ab","toHash":"cd","fromIata":"arn"}),
        ),
        (
            "beacon_find_cross_iata_routes",
            json!({"fromHash":"ab","toHash":"cd","iatas":["arn","ARN"]}),
        ),
        ("beacon_search_traces", json!({"cursorTag":"aabbccdd"})),
        ("beacon_search_traces", json!({"type":"OTHER"})),
        ("beacon_get_trace", json!({"tag":"../admin"})),
    ] {
        let response = rpc(&client, &root, "tools/call", Some(name), args).await;
        assert_eq!(response["result"]["isError"], true, "{name}: {response}");
        assert!(
            response["result"].get("structuredContent").is_none(),
            "{name}: {response}"
        );
    }
    assert!(seen.lock().unwrap().is_empty());
    let missing = rpc(
        &client,
        &root,
        "tools/call",
        Some("beacon_get_trace"),
        json!({"tag":"ffffffff"}),
    )
    .await;
    assert_eq!(
        missing["result"]["structuredContent"]["error"]["code"],
        "NOT_FOUND"
    );
    token.cancel();
    for task in tasks {
        task.abort();
    }
}
