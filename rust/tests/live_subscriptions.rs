// tungstenite's handshake callback requires its large HTTP response error type.
#![allow(clippy::result_large_err)]

use axum::{Json, Router, routing::get};
use beacon_mcp_rust::{
    client::BeaconClient,
    server,
    subscriptions::{Live, bridge},
};
use futures_util::{SinkExt, StreamExt};
use serde_json::{Value, json};
use std::{
    sync::{
        Arc, Mutex,
        atomic::{AtomicUsize, Ordering},
    },
    time::Duration,
};
use tokio_tungstenite::{accept_hdr_async, tungstenite::Message};
use tokio_util::sync::CancellationToken;

const NODE: &str = "123e4567-e89b-42d3-a456-426614174000";
const OBSERVER: &str = "123e4567-e89b-42d3-a456-426614174001";

#[tokio::test]
async fn websocket_uses_fresh_ticket_and_dispatches_only_valid_events() {
    let ticket_listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}/", ticket_listener.local_addr().unwrap());
    let ticket_server = tokio::spawn(async move {
        axum::serve(
            ticket_listener,
            Router::new().route(
                "/api/v1/ws-ticket",
                get(|| async { Json(json!({"ticket":"secret"})) }),
            ),
        )
        .await
        .unwrap();
    });
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let ws = format!("ws://{}/ws", listener.local_addr().unwrap());
    let header = Arc::new(Mutex::new(None));
    let mock = tokio::spawn({
        let header = header.clone();
        async move {
            let (connection, _) = listener.accept().await.unwrap();
            let mut socket=accept_hdr_async(connection,move |req:&tokio_tungstenite::tungstenite::handshake::server::Request, response:tokio_tungstenite::tungstenite::handshake::server::Response| {
                *header.lock().unwrap()=req.headers().get("X-Beacon-Ticket").and_then(|value|value.to_str().ok()).map(str::to_owned);
                Ok(response)
            }).await.unwrap();
            socket
                .send(Message::Text(
                    json!({"v":1,"type":"subscribed","id":"mcp-changes"})
                        .to_string()
                        .into(),
                ))
                .await
                .unwrap();
            socket.send(Message::Text(json!({"v":1,"type":"event","event":"nodeUpdate","data":{"nodeId":"123e4567-e89b-42d3-a456-426614174002"}}).to_string().into())).await.unwrap();
            socket
                .send(Message::Text(
                    json!({"v":1,"type":"hello"}).to_string().into(),
                ))
                .await
                .unwrap();
            let received = socket.next().await.unwrap().unwrap().into_text().unwrap();
            let subscribe: Value = serde_json::from_str(&received).unwrap();
            assert_eq!(subscribe["type"], "subscribe");
            assert_eq!(
                subscribe["scope"]["events"],
                json!(["nodeUpdate", "observerStatus"])
            );
            socket
                .send(Message::Text(
                    json!({"v":1,"type":"subscribed","id":"wrong-subscription"})
                        .to_string()
                        .into(),
                ))
                .await
                .unwrap();
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
                    json!({"v":1,"type":"event","event":"nodeUpdate","data":{"nodeId":NODE}})
                        .to_string()
                        .into(),
                ))
                .await
                .unwrap();
            socket.send(Message::Text(json!({"v":1,"type":"event","event":"observerStatus","data":{"observerId":OBSERVER}}).to_string().into())).await.unwrap();
            socket
                .send(Message::Text(
                    json!({"v":1,"type":"event","event":"nodeUpdate","data":{"nodeId":"../admin"}})
                        .to_string()
                        .into(),
                ))
                .await
                .unwrap();
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    });
    let shutdown = CancellationToken::new();
    let live = Live::new(2, Duration::ZERO, shutdown.clone());
    let mut events = live.subscribe();
    let client = BeaconClient::new(
        base.parse().unwrap(),
        Duration::from_secs(1),
        Duration::from_secs(1),
    )
    .expect("HTTP client configuration");
    let bridge_task = tokio::spawn(bridge(client, ws.parse().unwrap(), live));
    let first = tokio::time::timeout(Duration::from_secs(3), events.recv())
        .await
        .unwrap()
        .unwrap();
    let second = tokio::time::timeout(Duration::from_secs(3), events.recv())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        [first, second],
        [
            format!("beacon://nodes/{NODE}"),
            format!("beacon://observers/{OBSERVER}")
        ]
    );
    assert_eq!(header.lock().unwrap().as_deref(), Some("secret"));
    assert!(
        tokio::time::timeout(Duration::from_millis(25), events.recv())
            .await
            .is_err(),
        "pre-handshake events must never be delivered"
    );
    shutdown.cancel();
    bridge_task.await.unwrap();
    mock.await.unwrap();
    ticket_server.abort();
}

#[tokio::test]
async fn reconnect_fetches_new_ticket_and_legacy_404_connects_without_one() {
    for legacy in [false, true] {
        let count = Arc::new(AtomicUsize::new(0));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let base = format!("http://{}/", listener.local_addr().unwrap());
        let tickets = count.clone();
        let ticket_server = tokio::spawn(async move {
            axum::serve(
                listener,
                Router::new().route(
                    "/api/v1/ws-ticket",
                    get(move || {
                        let tickets = tickets.clone();
                        async move {
                            let number = tickets.fetch_add(1, Ordering::SeqCst) + 1;
                            if legacy {
                                (
                                    axum::http::StatusCode::NOT_FOUND,
                                    Json(json!({"code":"not_found"})),
                                )
                            } else {
                                (
                                    axum::http::StatusCode::OK,
                                    Json(json!({"ticket":format!("ticket-{number}")})),
                                )
                            }
                        }
                    }),
                ),
            )
            .await
            .unwrap();
        });
        let ws_listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let ws = format!("ws://{}/ws", ws_listener.local_addr().unwrap());
        let mock = tokio::spawn(async move {
            for connection_number in 1..=2 {
                let (connection, _) = ws_listener.accept().await.unwrap();
                let mut socket=accept_hdr_async(connection,move |request:&tokio_tungstenite::tungstenite::handshake::server::Request,response:tokio_tungstenite::tungstenite::handshake::server::Response| {
                    assert_eq!(request.headers().get("X-Beacon-Ticket").and_then(|v|v.to_str().ok()),
                        if legacy {None} else if connection_number==1 {Some("ticket-1")} else {Some("ticket-2")});
                    Ok(response)
                }).await.unwrap();
                socket
                    .send(Message::Text(
                        json!({"v":1,"type":"hello"}).to_string().into(),
                    ))
                    .await
                    .unwrap();
                assert!(
                    socket
                        .next()
                        .await
                        .unwrap()
                        .unwrap()
                        .into_text()
                        .unwrap()
                        .contains("mcp-changes")
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
                        json!({"v":1,"type":"event","event":"nodeUpdate","data":{"nodeId":NODE}})
                            .to_string()
                            .into(),
                    ))
                    .await
                    .unwrap();
                socket.close(None).await.unwrap();
            }
        });
        let shutdown = CancellationToken::new();
        let live = Live::new(2, Duration::ZERO, shutdown.clone());
        let mut events = live.subscribe();
        let client = BeaconClient::new(
            base.parse().unwrap(),
            Duration::from_secs(1),
            Duration::from_secs(1),
        )
        .expect("HTTP client configuration");
        let task = tokio::spawn(bridge(client, ws.parse().unwrap(), live));
        for _ in 0..2 {
            assert_eq!(
                tokio::time::timeout(Duration::from_secs(5), events.recv())
                    .await
                    .unwrap()
                    .unwrap(),
                format!("beacon://nodes/{NODE}")
            );
        }
        assert_eq!(count.load(Ordering::SeqCst), 2);
        shutdown.cancel();
        task.await.unwrap();
        mock.await.unwrap();
        ticket_server.abort();
    }
}

#[tokio::test]
async fn oversized_upstream_frame_is_closed_before_delivery() {
    let ticket_listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}/", ticket_listener.local_addr().unwrap());
    let ticket_server = tokio::spawn(async move {
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
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let ws_url = format!("ws://{}/ws", listener.local_addr().unwrap());
    let mock = tokio::spawn(async move {
        let (connection, _) = listener.accept().await.unwrap();
        let mut socket = tokio_tungstenite::accept_async(connection).await.unwrap();
        socket
            .send(Message::Text("x".repeat(131_073).into()))
            .await
            .unwrap();
        // The client drops the connection instead of buffering or publishing
        // the oversized application message.
        let closed = tokio::time::timeout(Duration::from_secs(2), socket.next())
            .await
            .expect("oversized frame did not close the connection");
        assert!(!matches!(closed, Some(Ok(Message::Text(_)))));
    });
    let shutdown = CancellationToken::new();
    let live = Live::new(2, Duration::ZERO, shutdown.clone());
    let mut events = live.subscribe();
    let client = BeaconClient::new(
        base.parse().unwrap(),
        Duration::from_secs(1),
        Duration::from_secs(1),
    )
    .expect("HTTP client configuration");
    let task = tokio::spawn(bridge(client, ws_url.parse().unwrap(), live));
    tokio::time::timeout(Duration::from_secs(3), mock)
        .await
        .expect("oversized frame test did not establish WebSocket")
        .unwrap();
    assert!(
        tokio::time::timeout(Duration::from_millis(50), events.recv())
            .await
            .is_err()
    );
    shutdown.cancel();
    task.await.unwrap();
    ticket_server.abort();
}

async fn listen(client: &reqwest::Client, root: &str, uri: &str) -> reqwest::Response {
    client
        .post(root)
        .header("Accept", "application/json, text/event-stream")
        .header("MCP-Protocol-Version", "2026-07-28")
        .header("Mcp-Method", "subscriptions/listen")
        .json(
            &json!({"jsonrpc":"2.0","id":"listen-1","method":"subscriptions/listen","params":{
                "notifications":{"resourceSubscriptions":[uri]},"_meta":{
                    "io.modelcontextprotocol/protocolVersion":"2026-07-28",
                    "io.modelcontextprotocol/clientInfo":{"name":"rust-test","version":"1.0.0"},
                    "io.modelcontextprotocol/clientCapabilities":{}
                }
            }}),
        )
        .send()
        .await
        .unwrap()
}

async fn until(
    stream: &mut (impl futures_util::Stream<Item = Result<bytes::Bytes, reqwest::Error>> + Unpin),
    pattern: &str,
) -> String {
    let mut text = String::new();
    tokio::time::timeout(Duration::from_secs(3), async {
        while !text.contains(pattern) {
            let chunk = stream.next().await.expect("SSE ended").unwrap();
            text.push_str(std::str::from_utf8(&chunk).unwrap());
        }
    })
    .await
    .unwrap();
    text
}

#[tokio::test]
async fn listen_sends_ack_and_only_matching_uri_then_cleans_up_on_shutdown() {
    let shutdown = CancellationToken::new();
    let live = Live::new(2, Duration::ZERO, shutdown.clone());
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let root = format!("http://{}/", listener.local_addr().unwrap());
    let client = BeaconClient::new(
        "http://127.0.0.1:9/".parse().unwrap(),
        Duration::from_secs(1),
        Duration::from_secs(1),
    )
    .expect("HTTP client configuration");
    let app = server::router_with_live(shutdown.clone(), client, Some(live.clone()));
    let task = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    let http = reqwest::Client::new();
    let first = listen(&http, &root, &format!("beacon://nodes/{NODE}")).await;
    assert_eq!(first.status(), 200);
    let second = listen(&http, &root, &format!("beacon://observers/{OBSERVER}")).await;
    assert_eq!(second.status(), 200);
    let mut first = first.bytes_stream();
    let mut second = second.bytes_stream();
    assert!(
        until(&mut first, "notifications/subscriptions/acknowledged")
            .await
            .contains(NODE)
    );
    assert!(
        until(&mut second, "notifications/subscriptions/acknowledged")
            .await
            .contains(OBSERVER)
    );
    live.received("observerStatus", &json!({"observerId":OBSERVER}));
    assert!(
        until(&mut second, "notifications/resources/updated")
            .await
            .contains(OBSERVER)
    );
    live.received("nodeUpdate", &json!({"nodeId":NODE}));
    assert!(
        until(&mut first, "notifications/resources/updated")
            .await
            .contains(NODE)
    );
    shutdown.cancel();
    assert!(
        tokio::time::timeout(Duration::from_secs(2), first.next())
            .await
            .is_ok()
    );
    task.abort();
}

#[tokio::test]
async fn stream_permits_are_bounded_and_released_on_disconnect() {
    let shutdown = CancellationToken::new();
    let live = Live::new(1, Duration::ZERO, shutdown.clone());
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let root = format!("http://{}/", listener.local_addr().unwrap());
    let client = BeaconClient::new(
        "http://127.0.0.1:9/".parse().unwrap(),
        Duration::from_secs(1),
        Duration::from_secs(1),
    )
    .expect("HTTP client configuration");
    let task = tokio::spawn({
        let app = server::router_with_live(shutdown.clone(), client, Some(live.clone()));
        async move { axum::serve(listener, app).await.unwrap() }
    });
    let http = reqwest::Client::new();
    let response = listen(&http, &root, &format!("beacon://nodes/{NODE}")).await;
    let mut first = response.bytes_stream();
    until(&mut first, "acknowledged").await;
    assert_eq!(live.permits.available_permits(), 0);
    let overflow = listen(&http, &root, &format!("beacon://nodes/{NODE}")).await;
    let mut overflow = overflow.bytes_stream();
    let result = until(&mut overflow, "Subscription stream limit reached").await;
    assert!(!result.contains("notifications/resources/updated"));
    drop(first);
    tokio::time::timeout(Duration::from_secs(2), async {
        while live.permits.available_permits() == 0 {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
    let response = listen(&http, &root, &format!("beacon://nodes/{NODE}")).await;
    let mut stream = response.bytes_stream();
    until(&mut stream, "acknowledged").await;
    shutdown.cancel();
    tokio::time::timeout(Duration::from_secs(2), async {
        while stream.next().await.is_some() {}
    })
    .await
    .unwrap();
    task.abort();
}

#[tokio::test]
async fn disabled_does_not_advertise_subscriptions_or_accept_listen() {
    let shutdown = CancellationToken::new();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let root = format!("http://{}/", listener.local_addr().unwrap());
    let client = BeaconClient::new(
        "http://127.0.0.1:9/".parse().unwrap(),
        Duration::from_secs(1),
        Duration::from_secs(1),
    )
    .expect("HTTP client configuration");
    let task = tokio::spawn({
        let app = server::router(shutdown.clone(), client);
        async move { axum::serve(listener, app).await.unwrap() }
    });
    let http = reqwest::Client::new();
    let discover = http
        .post(&root)
        .header("Accept", "application/json, text/event-stream")
        .header("MCP-Protocol-Version", "2026-07-28")
        .header("Mcp-Method", "server/discover")
        .json(
            &json!({"jsonrpc":"2.0","id":1,"method":"server/discover","params":{"_meta":{
                "io.modelcontextprotocol/protocolVersion":"2026-07-28",
                "io.modelcontextprotocol/clientInfo":{"name":"test","version":"1"},
                "io.modelcontextprotocol/clientCapabilities":{}
            }}}),
        )
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    assert!(!discover.contains("\"subscribe\":true"), "{discover}");
    let response = listen(&http, &root, &format!("beacon://nodes/{NODE}")).await;
    let body = response.text().await.unwrap();
    assert!(
        !body.contains("notifications/subscriptions/acknowledged"),
        "{body}"
    );
    shutdown.cancel();
    task.abort();
}

#[tokio::test]
async fn legacy_initialize_does_not_advertise_resource_subscribe() {
    let shutdown = CancellationToken::new();
    let live = Live::new(1, Duration::ZERO, shutdown.clone());
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let root = format!("http://{}/", listener.local_addr().unwrap());
    let client = BeaconClient::new(
        "http://127.0.0.1:9/".parse().unwrap(),
        Duration::from_secs(1),
        Duration::from_secs(1),
    )
    .expect("HTTP client configuration");
    let task = tokio::spawn({
        let app = server::router_with_live(shutdown.clone(), client, Some(live));
        async move { axum::serve(listener, app).await.unwrap() }
    });
    for revision in ["2025-11-25", "2024-10-07"] {
        let response=reqwest::Client::new().post(&root)
            .header("Accept","application/json, text/event-stream")
            .header("MCP-Protocol-Version",revision)
            .json(&json!({"jsonrpc":"2.0","id":3,"method":"initialize","params":{
                "protocolVersion":revision,"capabilities":{},"clientInfo":{"name":"legacy","version":"1"}
            }})).send().await.unwrap();
        assert_eq!(response.status(), 200);
        let body = response.text().await.unwrap();
        assert!(
            body.contains(&format!("\"protocolVersion\":\"{revision}\"")),
            "{body}"
        );
        assert!(body.contains("resources"), "{body}");
        assert!(!body.contains("\"subscribe\":true"), "{body}");
    }
    shutdown.cancel();
    task.abort();
}

#[tokio::test]
async fn mixed_case_packet_and_node_subscriptions_receive_canonical_updates() {
    let shutdown = CancellationToken::new();
    let live = Live::configured(3, Duration::ZERO, shutdown.clone(), true);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let root = format!("http://{}/", listener.local_addr().unwrap());
    let client = BeaconClient::new(
        "http://127.0.0.1:9/".parse().unwrap(),
        Duration::from_secs(1),
        Duration::from_secs(1),
    )
    .expect("HTTP client configuration");
    let task = tokio::spawn({
        let app = server::router_with_live(shutdown.clone(), client, Some(live.clone()));
        async move { axum::serve(listener, app).await.unwrap() }
    });
    let http = reqwest::Client::new();
    let first = listen(
        &http,
        &root,
        &format!("beacon://nodes/{}", NODE.to_uppercase()),
    )
    .await;
    let second = listen(&http, &root, "beacon://packets/AABBCCDDEEFF0011").await;
    assert_eq!(first.status(), 200);
    assert_eq!(second.status(), 200);
    let mut first = first.bytes_stream();
    let mut second = second.bytes_stream();
    let ack_node = until(&mut first, "notifications/subscriptions/acknowledged").await;
    let ack_packet = until(&mut second, "notifications/subscriptions/acknowledged").await;
    assert!(ack_node.contains(NODE), "{ack_node}");
    assert!(
        ack_packet.contains("beacon://packets/aabbccddeeff0011"),
        "{ack_packet}"
    );
    live.received("nodeUpdate", &json!({"nodeId":NODE}));
    live.received(
        "packetObservation",
        &json!({"packetHash":"AABBCCDDEEFF0011","content":"discarded"}),
    );
    let update_node = until(&mut first, "notifications/resources/updated").await;
    let update_packet = until(&mut second, "notifications/resources/updated").await;
    assert!(update_node.contains(NODE), "{update_node}");
    assert!(
        update_packet.contains("beacon://packets/aabbccddeeff0011"),
        "{update_packet}"
    );
    assert!(!update_packet.contains("discarded"));
    shutdown.cancel();
    task.abort();
}
