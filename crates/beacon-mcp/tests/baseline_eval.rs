//! PR 01 executable bilingual eval seed (offline, mock upstream only).
//! Each test maps to docs/ai-first/01-eval-baseline.md EVAL-0x and asserts
//! deterministic pass/fail without network, bodies, or sessions.

use axum::{
    Router,
    body::Body,
    http::{Response, StatusCode},
    routing::get,
};
use beacon_api::client::{BeaconClient, Operation};
use serde_json::{Value, json};
use std::time::Duration;

async fn mock(
    respond: impl Fn(&str) -> (StatusCode, String) + Clone + Send + Sync + 'static,
) -> (BeaconClient, tokio::task::JoinHandle<()>) {
    let router = Router::new().fallback(get({
        let respond = respond.clone();
        move |request: axum::http::Request<Body>| async move {
            let uri = request.uri().to_string();
            let (status, body) = respond(&uri);
            Response::builder()
                .status(status)
                .body(Body::from(body))
                .unwrap()
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
        .expect("client"),
        task,
    )
}

const NODE: &str = "123e4567-e89b-42d3-a456-426614174000";

#[tokio::test]
async fn eval01_polling_omission_stays_explicit() {
    let (client, task) = mock(|uri| {
        if uri.contains("/polling") {
            (
                StatusCode::OK,
                json!({"assigned": null, "standbys": []}).to_string(),
            )
        } else {
            (
                StatusCode::OK,
                json!({"battery": 4.1, "trusted": true}).to_string(),
            )
        }
    })
    .await;
    let polling = client
        .request(Operation::NodePolling, Some(NODE), &[])
        .await
        .unwrap();
    let telemetry = client
        .request(
            Operation::NodeTelemetry,
            Some(NODE),
            &[("range", "24h".to_string())],
        )
        .await
        .unwrap();
    assert!(polling.get("assigned").is_some());
    assert_eq!(polling["assigned"], Value::Null);
    assert_eq!(telemetry["battery"], json!(4.1));
    assert_ne!(telemetry["battery"], json!(0));
    task.abort();
}

#[tokio::test]
async fn eval02_ambiguous_name_returns_bounded_candidates() {
    let (client, task) = mock(|_| {
        (
            StatusCode::OK,
            json!({"items": [
                {"name": "Kalle", "id": "aa"},
                {"name": "Kalle-2", "id": "bb"},
            ]})
            .to_string(),
        )
    })
    .await;
    let result = client
        .request(Operation::Nodes, None, &[("name", "Kalle".to_string())])
        .await
        .unwrap();
    let items = result["items"].as_array().unwrap();
    assert_eq!(items.len(), 2);
    assert!(
        items
            .iter()
            .all(|item| item["name"].as_str().unwrap().starts_with("Kalle"))
    );
    task.abort();
}

#[tokio::test]
async fn eval03_node_and_observer_ids_stay_distinct() {
    let (client, task) = mock(|uri| {
        if uri.contains("/observers/") {
            (StatusCode::OK, json!({"kind": "observer"}).to_string())
        } else {
            (StatusCode::OK, json!({"kind": "node"}).to_string())
        }
    })
    .await;
    let node = client
        .request(Operation::Node, Some(NODE), &[])
        .await
        .unwrap();
    let observer = client
        .request(Operation::Observer, Some(NODE), &[])
        .await
        .unwrap();
    assert_eq!(node["kind"], json!("node"));
    assert_eq!(observer["kind"], json!("observer"));
    assert_ne!(node["kind"], observer["kind"]);
    task.abort();
}

#[tokio::test]
async fn eval04_missing_temperature_is_missing_not_zero() {
    let (client, task) = mock(|_| {
        (
            StatusCode::OK,
            json!({"battery": 4.0, "trusted": true}).to_string(),
        )
    })
    .await;
    let telemetry = client
        .request(
            Operation::NodeTelemetry,
            Some(NODE),
            &[("range", "24h".to_string())],
        )
        .await
        .unwrap();
    assert!(telemetry.get("temperature").is_none());
    assert_ne!(telemetry["battery"], json!(0));
    task.abort();
}

#[tokio::test]
async fn eval05_truncated_array_reports_honestly() {
    let (client, task) = mock(|_| (StatusCode::OK, json!([1, 2, 3, 4, 5]).to_string())).await;
    let page = client.array_page(Operation::Iatas, &[], 2).await.unwrap();
    assert_eq!(page.items.len(), 2);
    assert!(!page.complete);
    task.abort();
}

#[tokio::test]
async fn eval06_same_cursor_replays_statelessly() {
    let (client, task) = mock(|_| {
        (
            StatusCode::OK,
            json!({"items": [{"id": "x"}], "hasMore": false}).to_string(),
        )
    })
    .await;
    let first = client
        .request(Operation::Nodes, None, &[("cursor", "42".to_string())])
        .await
        .unwrap();
    let second = client
        .request(Operation::Nodes, None, &[("cursor", "42".to_string())])
        .await
        .unwrap();
    assert_eq!(first, second);
    task.abort();
}
