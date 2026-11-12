use axum::{Json, Router, routing::get};
use beacon_mcp_rust::{
    client::BeaconClient,
    subscriptions::{Live, bridge},
};
use futures_util::{SinkExt, StreamExt};
use serde_json::{Value, json};
use std::time::Duration;
use tokio_tungstenite::{accept_async, tungstenite::Message};
use tokio_util::sync::CancellationToken;

#[tokio::test]
async fn opt_in_websocket_requests_all_four_beacon_events_and_emits_resource_hints() {
    let ticket_listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let rest_url = format!("http://{}/", ticket_listener.local_addr().unwrap());
    let ticket_task = tokio::spawn(async move {
        axum::serve(
            ticket_listener,
            Router::new().route(
                "/api/v1/ws-ticket",
                get(|| async {
                    (
                        axum::http::StatusCode::NOT_FOUND,
                        Json(json!({"error":{"code":"not_found"}})),
                    )
                }),
            ),
        )
        .await
        .unwrap();
    });
    let ws_listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let ws_url = format!("ws://{}/ws", ws_listener.local_addr().unwrap());
    let mock = tokio::spawn(async move {
        let (connection, _) = ws_listener.accept().await.unwrap();
        let mut socket = accept_async(connection).await.unwrap();
        socket
            .send(Message::Text(
                json!({"v":1,"type":"hello"}).to_string().into(),
            ))
            .await
            .unwrap();
        let request = socket.next().await.unwrap().unwrap().into_text().unwrap();
        let body: Value = serde_json::from_str(&request).unwrap();
        assert_eq!(
            body["scope"]["events"],
            json!([
                "nodeUpdate",
                "observerStatus",
                "packetObservation",
                "channelMessage"
            ])
        );
        socket
            .send(Message::Text(
                json!({"v":1,"type":"subscribed","id":"mcp-changes"})
                    .to_string()
                    .into(),
            ))
            .await
            .unwrap();
        socket
            .send(Message::Text(
                json!({"v":1,"type":"event","event":"packetObservation",
                    "data":{"packetHash":"AABBCCDDEEFF0011","packet":{"payloadType":4}}})
                .to_string()
                .into(),
            ))
            .await
            .unwrap();
        socket
            .send(Message::Text(
                json!({"v":1,"type":"event","event":"channelMessage",
                    "data":{"channelId":25,"content":"not retained"}})
                .to_string()
                .into(),
            ))
            .await
            .unwrap();
        tokio::time::sleep(Duration::from_millis(80)).await;
    });
    let shutdown = CancellationToken::new();
    let live = Live::configured(2, Duration::ZERO, shutdown.clone(), true);
    let mut receiver = live.subscribe();
    let client = BeaconClient::new(
        rest_url.parse().unwrap(),
        Duration::from_secs(1),
        Duration::from_secs(1),
    )
    .expect("HTTP client");
    let task = tokio::spawn(bridge(client, ws_url.parse().unwrap(), live));
    let first = tokio::time::timeout(Duration::from_secs(3), receiver.recv())
        .await
        .unwrap()
        .unwrap();
    let second = tokio::time::timeout(Duration::from_secs(3), receiver.recv())
        .await
        .unwrap()
        .unwrap();
    let mut uris = [first, second];
    uris.sort();
    assert_eq!(
        uris,
        [
            "beacon://channels/25".to_owned(),
            "beacon://packets/aabbccddeeff0011".to_owned(),
        ]
    );
    shutdown.cancel();
    task.await.unwrap();
    mock.await.unwrap();
    ticket_task.abort();
}
