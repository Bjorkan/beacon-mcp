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
    let upstream=Router::new().fallback(get({
        let seen=seen.clone();
        move |req:Request<Body>| {
            let seen=seen.clone();
            async move {
                assert_eq!(req.method(),"GET");
                seen.lock().unwrap().push(req.uri().to_string());
                let (status,body) = match req.uri().path() {
                    "/api/v1/packets" => (StatusCode::OK,json!({"items":[{"hash":"aabbccddeeff0011"}],"hasMore":true,"nextPageCursor":"v1:opaque","nextCursor":42})),
                    "/api/v1/packets/aabbccddeeff0011" => (StatusCode::OK,json!({"hash":"aabbccddeeff0011","observations":[{"heardAt":1}]})),
                    "/api/v1/messages" => (StatusCode::OK,json!({"items":[{"text":"hello"}],"hasMore":true,"nextCursor":24})),
                    "/api/v1/channels" => (StatusCode::OK,json!({"items":[{"hash":"ab","id":7}],"hasMore":true,"nextPageCursor":"opaque","nextCursor":55})),
                    "/api/v1/channels/7/messages" => (StatusCode::OK,json!({"items":[{"text":"channel"}],"hasMore":false})),
                    _ => (StatusCode::NOT_FOUND,json!({"error":{"code":"not_found","message":"missing"}})),
                };
                Response::builder().status(status).body(Body::from(body.to_string())).unwrap()
            }
        }
    }));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}/", listener.local_addr().unwrap());
    let upstream_task = tokio::spawn(async move { axum::serve(listener, upstream).await.unwrap() });
    let gateway_listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let root = format!("http://{}/", gateway_listener.local_addr().unwrap());
    let token = CancellationToken::new();
    let app = server::router(
        token.clone(),
        BeaconClient::new(
            url.parse().unwrap(),
            Duration::from_secs(1),
            Duration::from_secs(1),
        ),
    );
    let gateway_task =
        tokio::spawn(async move { axum::serve(gateway_listener, app).await.unwrap() });
    (
        reqwest::Client::new(),
        root,
        seen,
        token,
        vec![upstream_task, gateway_task],
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
async fn calls_all_five_packet_and_channel_tools_with_filters_and_cursors() {
    let (client, root, seen, token, tasks) = setup().await;
    let listed = rpc(&client, &root, "tools/list", None, json!({})).await;
    let expected_schemas: Value =
        serde_json::from_str(include_str!("../src/packet-output-schemas.json")).unwrap();
    for tool in listed["result"]["tools"].as_array().unwrap() {
        let name = tool["name"].as_str().unwrap();
        if let Some(expected) = expected_schemas.get(name) {
            assert_eq!(tool["outputSchema"], *expected, "{name} output schema");
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
            "beacon_search_packets",
            json!({"iatas":["arn"],"scope":"#se13","payloadTypeName":"reserved","routeTypes":[0,2],"hashPrefix":"abcde","cursor":42,"since":"2026-10-01T00:00:00Z","until":"2026-10-02T00:00:00Z"}),
            "items",
        ),
        (
            "beacon_get_packet",
            json!({"hash":"aabbccddeeff0011"}),
            "hash",
        ),
        (
            "beacon_search_messages",
            json!({"channelHash":"ab","since":"2026-10-01T00:00:00Z"}),
            "items",
        ),
        (
            "beacon_list_channels",
            json!({"hash":"ab","keyKnown":true,"pageCursor":"opaque"}),
            "items",
        ),
        (
            "beacon_get_channel_messages",
            json!({"channelId":7,"cursor":24}),
            "items",
        ),
    ] {
        assert!(names.contains(&name), "{name}");
        let result = rpc(&client, &root, "tools/call", Some(name), args).await;
        assert_eq!(result["result"]["isError"], false, "{name}: {result}");
        assert!(
            !result["result"]["structuredContent"][field].is_null(),
            "{name}: {result}"
        );
        let text = result["result"]["content"][0]["text"]
            .as_str()
            .expect("MCP text confirmation");
        let count = result["result"]["structuredContent"]["items"]
            .as_array()
            .map(Vec::len);
        assert_eq!(
            text,
            count
                .map(|n| format!("Beacon returned {n} item(s)."))
                .unwrap_or_else(|| "Beacon request succeeded.".into()),
            "packet or message data must not be duplicated as text"
        );
        if name == "beacon_list_channels" {
            assert_eq!(
                result["result"]["structuredContent"]["pagination"]["nextCursor"],
                json!({"pageCursor":"opaque"})
            );
        }
    }
    let paths = seen.lock().unwrap();
    assert!(
        paths
            .iter()
            .any(|path| path.contains("payloadTypes=12%2C13%2C14")
                && path.contains("hashPrefix=abcde")
                && path.contains("iatas=ARN"))
    );
    assert!(paths.iter().any(
        |path| !path.contains("channelID=7") && path.starts_with("/api/v1/channels/7/messages")
    ));
    drop(paths);
    token.cancel();
    for task in tasks {
        task.abort();
    }
}

#[tokio::test]
async fn rejects_invalid_inputs_without_contacting_upstream_and_reports_not_found() {
    let (client, root, seen, token, tasks) = setup().await;
    for (name, args) in [
        (
            "beacon_search_packets",
            json!({"payloadType":1,"payloadTypes":[2]}),
        ),
        (
            "beacon_search_packets",
            json!({"payloadTypeName":"unknown"}),
        ),
        (
            "beacon_search_packets",
            json!({"hashPrefix":"abcdef","pageCursor":"token"}),
        ),
        (
            "beacon_search_packets",
            json!({"since":"2026-10-02T00:00:00Z","until":"2026-10-01T00:00:00Z"}),
        ),
        (
            "beacon_search_messages",
            json!({"channelId":7,"channelHash":"ab"}),
        ),
        ("beacon_list_channels", json!({"hash":"abc"})),
        ("beacon_get_packet", json!({"hash":"../../admin"})),
        ("beacon_get_channel_messages", json!({"channelId":0})),
    ] {
        let response = rpc(&client, &root, "tools/call", Some(name), args).await;
        assert_eq!(response["result"]["isError"], true, "{name}: {response}");
        assert_eq!(
            response["result"]["structuredContent"]["error"]["code"],
            "INVALID_ARGUMENT"
        );
    }
    assert!(seen.lock().unwrap().is_empty());
    let missing = rpc(
        &client,
        &root,
        "tools/call",
        Some("beacon_get_packet"),
        json!({"hash":"ffffffffffffffff"}),
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
