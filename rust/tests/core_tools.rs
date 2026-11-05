use std::{
    sync::{Arc, Mutex},
    time::Duration,
};

use axum::{
    Router,
    body::Body,
    http::{Request, StatusCode},
    response::Response,
    routing::get,
};
use beacon_mcp_rust::{client::BeaconClient, server};
use serde_json::{Value, json};
use tokio_util::sync::CancellationToken;

const ID: &str = "123e4567-e89b-42d3-a456-426614174000";

async fn setup() -> (
    reqwest::Client,
    String,
    Arc<Mutex<Vec<String>>>,
    CancellationToken,
    Vec<tokio::task::JoinHandle<()>>,
) {
    let paths = Arc::new(Mutex::new(Vec::new()));
    let backend = Router::new().fallback(get({
        let paths = paths.clone();
        move |req: Request<Body>| {
            let paths = paths.clone();
            async move {
                let uri = req.uri().to_string();
                paths.lock().unwrap().push(uri.clone());
                let data = match req.uri().path() {
                    "/api/v1/info" => json!({"serverVersion":"2.1.1"}),
                    "/api/v1/iatas" => json!([{"iata":"ARN"},{"iata":"MMX"}]),
                    "/api/v1/regions" => json!([{"slug":"sverige", "iatas":["ARN"]}]),
                    "/api/v1/scopes" => json!(["#a", "#b", "#c"]),
                    "/api/v1/nodes" => json!({"items":[{"id":ID,"lastSeen":1000}],"hasMore":true,"nextCursor":1000,"nextPageCursor":"v1:1000000:uuid","sort":"name","order":"asc"}),
                    "/api/v1/observers" => json!({"items":[{"id":ID}],"hasMore":false}),
                    path if path == format!("/api/v1/nodes/{ID}") => json!({"id":ID,"name":"Node"}),
                    path if path == format!("/api/v1/observers/{ID}") => json!({"id":ID,"status":"online"}),
                    path if path == format!("/api/v1/observers/{ID}/activity") => json!({"points":[], "interval":"1h"}),
                    _ => return Response::builder().status(StatusCode::NOT_FOUND).body(Body::from(r#"{"error":{"code":"not_found","message":"missing"}}"#)).unwrap(),
                };
                Response::builder().status(StatusCode::OK).body(Body::from(data.to_string())).unwrap()
            }
        }
    }));
    let backend_listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}/", backend_listener.local_addr().unwrap());
    let backend_task =
        tokio::spawn(async move { axum::serve(backend_listener, backend).await.unwrap() });
    let gateway_listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let gateway = format!("http://{}/", gateway_listener.local_addr().unwrap());
    let token = CancellationToken::new();
    let app = server::router(
        token.clone(),
        BeaconClient::new(
            base.parse().unwrap(),
            Duration::from_secs(1),
            Duration::from_secs(1),
        ),
    );
    let gateway_task =
        tokio::spawn(async move { axum::serve(gateway_listener, app).await.unwrap() });
    (
        reqwest::Client::new(),
        gateway,
        paths,
        token,
        vec![backend_task, gateway_task],
    )
}

async fn rpc(
    client: &reqwest::Client,
    root: &str,
    method: &str,
    name: Option<&str>,
    arguments: Value,
) -> Value {
    let mut request = client
        .post(root)
        .header("Accept", "application/json, text/event-stream")
        .header("MCP-Protocol-Version", "2026-07-28")
        .header("Mcp-Method", method);
    if let Some(name) = name {
        request = request.header("Mcp-Name", name);
    }
    let mut params = json!({"_meta": {
        "io.modelcontextprotocol/protocolVersion": "2026-07-28",
        "io.modelcontextprotocol/clientInfo": {"name":"rust-test","version":"1.0.0"},
        "io.modelcontextprotocol/clientCapabilities": {}
    }});
    if let Some(name) = name {
        params["name"] = json!(name);
        params["arguments"] = arguments;
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
async fn lists_only_implemented_tools_and_calls_all_nine() {
    let (client, root, paths, token, tasks) = setup().await;
    let listed = rpc(&client, &root, "tools/list", None, json!({})).await;
    let names = listed["result"]["tools"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v["name"].as_str().unwrap())
        .collect::<Vec<_>>();
    assert_eq!(names.len(), 9);
    for (name, args, field) in [
        ("beacon_get_server_info", json!({}), "serverVersion"),
        ("beacon_list_iatas", json!({"limit":1}), "items"),
        ("beacon_list_regions", json!({"limit":2}), "items"),
        ("beacon_list_scopes", json!({"limit":2}), "items"),
        (
            "beacon_search_nodes",
            json!({"sort":"name","iatas":["arn"],"limit":1}),
            "items",
        ),
        ("beacon_get_node", json!({"id":ID}), "name"),
        (
            "beacon_search_observers",
            json!({"status":"online", "type":"gateway", "pageCursor":"opaque:next", "cursor":1000}),
            "items",
        ),
        ("beacon_get_observer", json!({"id":ID}), "status"),
        (
            "beacon_get_observer_activity",
            json!({"id":ID,"range":"24h","interval":"1h","until":"2026-10-01T00:00:00Z"}),
            "points",
        ),
    ] {
        assert!(names.contains(&name));
        let result = rpc(&client, &root, "tools/call", Some(name), args).await;
        assert_eq!(result["result"]["isError"], false, "{name}: {result}");
        assert!(
            !result["result"]["structuredContent"][field].is_null(),
            "{name}: {result}"
        );
    }
    let node_page = rpc(
        &client,
        &root,
        "tools/call",
        Some("beacon_search_nodes"),
        json!({"limit":1,"pageCursor":"opaque:next", "cursor":1000}),
    )
    .await;
    assert_eq!(
        node_page["result"]["structuredContent"]["pagination"]["nextCursor"]["pageCursor"],
        "v1:1000000:uuid"
    );
    assert_eq!(
        node_page["result"]["structuredContent"]["pagination"]["nextCursor"]["cursor"],
        1000
    );
    let scope_page = rpc(
        &client,
        &root,
        "tools/call",
        Some("beacon_list_scopes"),
        json!({"limit":2,"cursor":2}),
    )
    .await;
    assert_eq!(
        scope_page["result"]["structuredContent"]["items"],
        json!(["#c"])
    );
    assert_eq!(
        scope_page["result"]["structuredContent"]["pagination"]["hasMore"],
        false
    );
    let requested = paths.lock().unwrap();
    assert!(requested.iter().any(|path| path.contains("iatas=ARN")));
    assert!(
        requested
            .iter()
            .any(|path| path.contains("type=gateway") && path.contains("pageCursor=opaque%3Anext"))
    );
    assert!(
        requested
            .iter()
            .any(|path| path.contains("until=1790812800000"))
    );
    drop(requested);
    token.cancel();
    for task in tasks {
        task.abort();
    }
}

#[tokio::test]
async fn rejects_invalid_filters_and_reports_upstream_errors() {
    let (client, root, paths, token, tasks) = setup().await;
    for (name, args) in [
        ("beacon_search_nodes", json!({"limit":51})),
        ("beacon_search_nodes", json!({"region":"se","regionId":1})),
        ("beacon_search_nodes", json!({"sort":"name","cursor":123})),
        ("beacon_search_nodes", json!({"regionId":2147483648u64})),
        ("beacon_search_nodes", json!({"iatas":[" arn "]})),
        ("beacon_search_nodes", json!({"typeName":"not_a_node_type"})),
        ("beacon_search_observers", json!({"type":1})),
        ("beacon_search_observers", json!({"type":" "})),
        ("beacon_search_observers", json!({"status":"invalid"})),
        (
            "beacon_get_observer_activity",
            json!({"id":ID,"range":"72h","interval":"5m"}),
        ),
        ("beacon_get_node", json!({"id":"../admin/config"})),
        (
            "beacon_get_node",
            json!({"id":"00000000-0000-0000-0000-000000000000"}),
        ),
    ] {
        let result = rpc(&client, &root, "tools/call", Some(name), args).await;
        assert_eq!(result["result"]["isError"], true, "{result}");
    }
    assert!(
        paths.lock().unwrap().is_empty(),
        "validation must precede upstream requests"
    );
    let unknown = rpc(
        &client,
        &root,
        "tools/call",
        Some("beacon_get_node"),
        json!({"id":"123e4567-e89b-42d3-a456-426614174001"}),
    )
    .await;
    assert_eq!(
        unknown["result"]["structuredContent"]["error"]["code"],
        "NOT_FOUND"
    );
    token.cancel();
    for task in tasks {
        task.abort();
    }
}
