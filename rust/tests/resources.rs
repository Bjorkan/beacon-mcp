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

const ID: &str = "123e4567-e89b-42d3-a456-426614174000";

async fn rpc(client: &reqwest::Client, root: &str, method: &str, params: Value) -> Value {
    let mut request_params = json!({
        "_meta":{
            "io.modelcontextprotocol/protocolVersion":"2026-07-28",
            "io.modelcontextprotocol/clientInfo":{"name":"rust-test","version":"1.0.0"},
            "io.modelcontextprotocol/clientCapabilities":{}
        }
    });
    if let Some(uri) = params.get("uri") {
        request_params["uri"] = uri.clone();
    }
    let mut request = client
        .post(root)
        .header("Accept", "application/json, text/event-stream")
        .header("MCP-Protocol-Version", "2026-07-28")
        .header("Mcp-Method", method);
    if let Some(uri) = params["uri"].as_str() {
        request = request.header("Mcp-Name", uri);
    }
    let response = request
        .json(&json!({"jsonrpc":"2.0","id":1,"method":method,"params":request_params}))
        .send()
        .await
        .unwrap();
    let status = response.status();
    let text = response.text().await.unwrap();
    assert_eq!(status, StatusCode::OK, "{text}");
    serde_json::from_str(&text).unwrap()
}

#[tokio::test]
async fn templates_read_both_resources_and_reject_injected_uris() {
    let seen = Arc::new(Mutex::new(Vec::new()));
    let backend = Router::new().fallback(get({
        let seen = seen.clone();
        move |request: Request<Body>| {
            let seen = seen.clone();
            async move {
                seen.lock().unwrap().push(request.uri().to_string());
                let body = match request.uri().path() {
                    path if path == format!("/api/v1/nodes/{ID}") => json!({"id":ID,"name":"Node"}),
                    path if path == format!("/api/v1/observers/{ID}") => {
                        json!({"id":ID,"status":"online"})
                    }
                    "/api/v1/packets/aabbccddeeff0011" => json!({"packetHash":"aabbccddeeff0011"}),
                    "/api/v1/channels/25" => json!({"id":25,"name":"public"}),
                    "/api/v1/traces/abcdef01" => json!({"traceTag":"abcdef01"}),
                    "/api/v1/iatas/ARN" => json!({"iata":"ARN"}),
                    "/api/v1/regions/3" => json!({"id":3,"slug":"sverige"}),
                    _ => {
                        return Response::builder()
                            .status(StatusCode::NOT_FOUND)
                            .body(Body::from(r#"{"error":{"code":"not_found"}}"#))
                            .unwrap();
                    }
                };
                Response::builder()
                    .status(StatusCode::OK)
                    .body(Body::from(body.to_string()))
                    .unwrap()
            }
        }
    }));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let upstream = format!("http://{}/", listener.local_addr().unwrap());
    let back_task = tokio::spawn(async move { axum::serve(listener, backend).await.unwrap() });
    let gateway_listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let root = format!("http://{}/", gateway_listener.local_addr().unwrap());
    let token = CancellationToken::new();
    let app = server::router(
        token.clone(),
        BeaconClient::new(
            upstream.parse().unwrap(),
            Duration::from_secs(1),
            Duration::from_secs(1),
        )
        .expect("HTTP client configuration"),
    );
    let app_task = tokio::spawn(async move { axum::serve(gateway_listener, app).await.unwrap() });
    let client = reqwest::Client::new();
    let templates = rpc(&client, &root, "resources/templates/list", json!({})).await;
    assert_eq!(
        templates["result"]["resourceTemplates"]
            .as_array()
            .unwrap()
            .len(),
        7,
        "{templates}"
    );
    for kind in ["nodes", "observers"] {
        let result = rpc(
            &client,
            &root,
            "resources/read",
            json!({"uri":format!("beacon://{kind}/{ID}")}),
        )
        .await;
        assert_eq!(
            result["result"]["contents"][0]["uri"],
            format!("beacon://{kind}/{ID}"),
            "{result}"
        );
        assert_eq!(
            result["result"]["contents"][0]["mimeType"],
            "application/json"
        );
        let data: Value =
            serde_json::from_str(result["result"]["contents"][0]["text"].as_str().unwrap())
                .unwrap();
        assert_eq!(data["id"], ID);
    }
    for (uri, upstream_field, expected) in [
        ("beacon://packets/AABBCCDDEEFF0011", "packetHash", json!("aabbccddeeff0011")),
        ("beacon://channels/25", "id", json!(25)),
        ("beacon://traces/ABCDEF01", "traceTag", json!("abcdef01")),
        ("beacon://iatas/arn", "iata", json!("ARN")),
        ("beacon://regions/3", "id", json!(3)),
    ] {
        let result = rpc(&client, &root, "resources/read", json!({"uri":uri})).await;
        assert!(result["error"].is_null(), "{result}");
        let body: Value = serde_json::from_str(
            result["result"]["contents"][0]["text"].as_str().unwrap(),
        ).unwrap();
        assert_eq!(body[upstream_field], expected, "{uri}");
    }
    for uri in [
        "beacon://nodes/../admin/config",
        "beacon://observers/%2e%2e",
        "beacon://nodes/00000000-0000-0000-0000-000000000000",
    ] {
        let result = rpc(&client, &root, "resources/read", json!({"uri":uri})).await;
        assert!(!result["error"].is_null(), "{result}");
    }
    assert_eq!(seen.lock().unwrap().len(), 7);
    let unknown = rpc(
        &client,
        &root,
        "resources/read",
        json!({"uri":"beacon://nodes/123e4567-e89b-42d3-a456-426614174001"}),
    )
    .await;
    assert!(!unknown["error"].is_null());
    token.cancel();
    back_task.abort();
    app_task.abort();
}
