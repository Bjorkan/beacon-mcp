use axum::{
    Router,
    body::Body,
    http::{Request, StatusCode},
    response::Response,
    routing::get,
};
use beacon_mcp_rust::{
    client::{BeaconClient, ErrorCode},
    extra_tools::ExtraTools,
};
use serde_json::{Value, json};
use std::{
    sync::{Arc, Mutex},
    time::Duration,
};

const ID: &str = "123e4567-e89b-42d3-a456-426614174000";
const PATH_KEY: &str = "0123456789abcdef0123456789abcdef";

async fn mock() -> (
    ExtraTools,
    Arc<Mutex<Vec<String>>>,
    tokio::task::JoinHandle<()>,
) {
    let seen = Arc::new(Mutex::new(Vec::new()));
    let app = Router::new().fallback(get({
        let seen = seen.clone();
        move |request: Request<Body>| {
            let seen = seen.clone();
            async move {
                let uri = request.uri().to_string();
                seen.lock().unwrap().push(uri.clone());
                let body = if uri.contains("/nodes/") && uri.contains("/activity") {
                    json!({"range":"48h","points":[{"t":1762819200000,"observations":2}],
                        "summary":{"observations":2},"windowStart":1762819200000})
                } else if uri.contains("/nodes/") && uri.contains("/observations") {
                    json!({"items":[{"id":9},{"id":8}],"hasMore":true,"nextCursor":8})
                } else if uri.contains("/observers/directory") {
                    json!({"items":[{"id":ID,"observationCount":100}],"hasMore":true,
                        "nextCursor":123,"windowStart":1234567890000,"windowEnd":1234567990000,
                        "coverage":{"complete":true},"maxObservationCount":100,
                        "effectiveSort":"traffic","observerTypes":["mqtt"]})
                } else if uri.contains("/routes/") && uri.contains("/observations") {
                    json!({"items":[],"hasMore":false,"matchAvailable":true,"pathBytes":"010203",
                        "windowStart":1000,"windowEnd":2000})
                } else if uri.contains("/stats/node-types") {
                    json!([{"type":"repeater","count":5},{"type":"sensor","count":3}])
                } else if uri.contains("/packets/backfill") {
                    json!([{"packetHash":"0011223344556677"}])
                } else {
                    json!({"result":"ok"})
                };
                Response::builder().status(StatusCode::OK)
                    .body(Body::from(body.to_string())).unwrap()
            }
        }
    }));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}/", listener.local_addr().unwrap());
    let task = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    let client = BeaconClient::new(url.parse().unwrap(),
        Duration::from_secs(2), Duration::from_secs(2));
    (ExtraTools::new(client), seen, task)
}

fn args(value: Value) -> Option<serde_json::Map<String, Value>> {
    Some(value.as_object().unwrap().clone())
}

#[test]
fn frozen_discovery_has_all_additional_tools_and_read_only_hints() {
    let client = BeaconClient::new("http://127.0.0.1:9/".parse().unwrap(),
        Duration::from_secs(1), Duration::from_secs(1));
    let tools = ExtraTools::new(client);
    let listed = tools.list();
    assert_eq!(listed.len(), 26);
    let mut names = std::collections::HashSet::new();
    for tool in listed {
        let raw = serde_json::to_value(&tool).unwrap();
        assert_eq!(raw["annotations"]["readOnlyHint"], true);
        assert_eq!(raw["annotations"]["destructiveHint"], false);
        assert!(raw["inputSchema"]["properties"].is_object());
        assert!(raw["outputSchema"].is_object());
        assert!(names.insert(tool.name.to_string()), "duplicate tool name");
    }
    assert!(names.contains("beacon_get_node_activity"));
    assert!(names.contains("beacon_get_node_observations"));
    assert!(names.contains("beacon_get_signal_stats"));
    assert!(names.contains("beacon_get_route_observations"));
}

#[tokio::test]
async fn node_activity_and_observation_cursors_route_exactly() {
    let (tools, seen, task) = mock().await;
    let activity = tools.execute("beacon_get_node_activity",
        args(json!({"id":ID,"range":"48h"}))).await.unwrap();
    assert_eq!(activity["points"][0]["observations"], 2);
    let page = tools.execute("beacon_get_node_observations",
        args(json!({"id":ID,"cursor":2_147_483_649_u64,"limit":1}))).await.unwrap();
    assert_eq!(page["items"].as_array().unwrap().len(), 1);
    assert_eq!(page["pagination"]["nextCursor"]["cursor"], 8);
    let paths = seen.lock().unwrap();
    assert_eq!(paths[0], format!("/api/v1/nodes/{ID}/activity?range=48h"));
    assert!(paths[1].starts_with(&format!("/api/v1/nodes/{ID}/observations?")));
    assert!(paths[1].contains("cursor=2147483649"));
    assert!(paths[1].contains("limit=1"));
    task.abort();
}

#[tokio::test]
async fn route_evidence_uses_fixed_validated_path_segments() {
    let (tools, seen, task) = mock().await;
    let evidence = tools.execute("beacon_get_route_observations",
        args(json!({"iata":"arn","pathKey":PATH_KEY,"range":"24h","limit":5})))
        .await.unwrap();
    assert_eq!(evidence["matchAvailable"], true);
    assert!(seen.lock().unwrap()[0].starts_with(&format!(
        "/api/v1/routes/ARN/{PATH_KEY}/observations?"
    )));
    for bad in [
        json!({"iata":"ARN","pathKey":"../../admin"}),
        json!({"iata":"../","pathKey":PATH_KEY}),
        json!({"iata":"ARN","pathKey":PATH_KEY,"range":"24h",
            "since":"2026-11-01T00:00:00Z","until":"2026-11-02T00:00:00Z"})
    ] {
        assert_eq!(
            tools.execute("beacon_get_route_observations", args(bad))
                .await.unwrap_err().code,
            ErrorCode::InvalidArgument
        );
    }
    assert_eq!(seen.lock().unwrap().len(), 1);
    task.abort();
}

#[tokio::test]
async fn backfill_and_unpaged_aggregations_are_bounded_and_preserve_ids() {
    let (tools, seen, task) = mock().await;
    let page = tools.execute("beacon_backfill_packets",
        args(json!({"afterObservationId":2_147_483_649_u64,"limit":1,
            "iatas":["arn","mmx"]}))).await.unwrap();
    assert_eq!(page["items"].as_array().unwrap().len(), 1);
    let types = tools.execute("beacon_get_node_types",
        args(json!({"region":"sverige","limit":1}))).await.unwrap();
    assert_eq!(types["items"].as_array().unwrap().len(), 1);
    assert_eq!(types["pagination"], json!({"hasMore":false,"truncated":true}));
    let paths = seen.lock().unwrap();
    assert!(paths[0].contains("afterObservationId=2147483649"));
    assert!(paths[0].contains("iatas=ARN%2CMMX"));
    assert!(paths[1].contains("region=sverige"));
    assert!(!paths[1].contains("limit="));
    task.abort();
}

#[tokio::test]
async fn directory_preserves_window_and_coverage_on_continuation() {
    let (tools, seen, task) = mock().await;
    let data = tools.execute("beacon_list_observer_directory",
        args(json!({"limit":1,"sort":"traffic",
            "since":"2026-11-01T00:00:00Z",
            "until":"2026-11-02T00:00:00Z",
            "cursor":12}))).await.unwrap();
    assert_eq!(data["pagination"]["nextCursor"]["cursor"], 123);
    assert_eq!(data["windowStart"], 1_234_567_890_000_u64);
    assert_eq!(data["maxObservationCount"], 100);
    assert_eq!(data["coverage"]["complete"], true);
    assert_eq!(data["observerTypes"][0], "mqtt");
    let paths = seen.lock().unwrap();
    assert!(paths[0].contains("since=1793491200000"));
    assert!(paths[0].contains("until=1793577600000"));
    task.abort();
}
