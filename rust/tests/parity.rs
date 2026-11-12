use axum::{
    Router,
    body::Body,
    http::{Request, StatusCode},
    response::Response,
    routing::get,
};
use beacon_mcp_rust::{
    analytics_tools::AnalyticsTools,
    client::{BeaconClient, ErrorCode, Operation},
    packet_tools::PacketTools,
    routing_tools::RoutingTools,
    tools::CoreTools,
};
use serde_json::{Value, json};
use std::{
    sync::{Arc, Mutex},
    time::Duration,
};

async fn backend(
    respond: impl Fn(&str) -> String + Clone + Send + Sync + 'static,
) -> (
    BeaconClient,
    Arc<Mutex<Vec<String>>>,
    tokio::task::JoinHandle<()>,
) {
    let seen = Arc::new(Mutex::new(Vec::new()));
    let router = Router::new().fallback(get({
        let seen = seen.clone();
        move |request: Request<Body>| {
            let respond = respond.clone();
            let seen = seen.clone();
            async move {
                let uri = request.uri().to_string();
                seen.lock().unwrap().push(uri.clone());
                Response::builder()
                    .status(StatusCode::OK)
                    .body(Body::from(respond(&uri)))
                    .unwrap()
            }
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
        .expect("HTTP client configuration"),
        seen,
        task,
    )
}

fn args(value: Value) -> Option<serde_json::Map<String, Value>> {
    Some(value.as_object().unwrap().clone())
}

#[tokio::test]
async fn exact_node_cursor_sort_neighbors_and_numeric_fallback() {
    let (client, seen, task) = backend(|_| json!({"items":[{"id":"first","neighborIds":["second"]}],"hasMore":true,"nextCursor":1000,"nextPageCursor":"s1:name:uuid","sort":"name","order":"asc"}).to_string()).await;
    let tools = CoreTools::new(client);
    let result = tools
        .execute(
            "beacon_search_nodes",
            args(json!({"sort":"name","neighbors":true,"limit":1,"iatas":[" arn "]})),
        )
        .await
        .unwrap();
    assert_eq!(result["items"][0]["neighborIds"], json!(["second"]));
    assert_eq!(
        result["pagination"]["nextCursor"]["pageCursor"],
        "s1:name:uuid"
    );
    assert_eq!(result["sort"], "name");
    assert_eq!(result["order"], "asc");
    tools
        .execute(
            "beacon_search_nodes",
            args(json!({"pageCursor":"s1:name:uuid","limit":1})),
        )
        .await
        .unwrap();
    let seen = seen.lock().unwrap();
    assert!(seen[0].contains("iatas=ARN") && seen[0].contains("neighbors=true"));
    assert!(seen[1].contains("pageCursor=s1%3Aname%3Auuid"));
    task.abort();
}

#[tokio::test]
async fn packet_hash_and_legacy_payload_filters_preserve_exact_queries() {
    let (client, seen, task) = backend(|_| {
        json!({"items":[],"hasMore":true,"nextCursor":1000,"nextPageCursor":"v1:uuid"}).to_string()
    })
    .await;
    let tools = PacketTools::new(client);
    for name in ["group_text", "grp_txt", "reserved"] {
        tools
            .execute(
                "beacon_search_packets",
                args(json!({"payloadTypeName":name})),
            )
            .await
            .unwrap();
    }
    let page = tools
        .execute(
            "beacon_search_packets",
            args(json!({"hashPrefix":"aBcDe","cursor":1000,"iatas":["arn"]})),
        )
        .await
        .unwrap();
    assert_eq!(page["pagination"]["nextCursor"], json!({"cursor":1000}));
    let seen = seen.lock().unwrap();
    assert!(seen[0].contains("payloadTypes=5") && seen[1].contains("payloadTypes=5"));
    assert!(seen[2].contains("payloadTypes=12%2C13%2C14"));
    assert!(seen[3].contains("hashPrefix=aBcDe") && seen[3].contains("iatas=ARN"));
    task.abort();
}

#[tokio::test]
async fn channel_page_exposes_only_precise_cursor() {
    let (client, seen, task) = backend(|_| {
        json!({"items":[{"id":1}],"hasMore":true,"nextCursor":1000,"nextPageCursor":"v1:1000000:1"})
            .to_string()
    })
    .await;
    let result = PacketTools::new(client)
        .execute(
            "beacon_list_channels",
            args(json!({"pageCursor":"v1:1000:7","limit":1})),
        )
        .await
        .unwrap();
    assert_eq!(
        result["pagination"]["nextCursor"],
        json!({"pageCursor":"v1:1000000:1"})
    );
    assert!(seen.lock().unwrap()[0].contains("pageCursor=v1%3A1000%3A7"));
    task.abort();
}

#[tokio::test]
async fn directed_and_multi_iata_routes_never_widen_to_all_routes() {
    let (client, seen, task) = backend(|_| "[]".into()).await;
    let routes = RoutingTools::new(client);
    routes
        .execute(
            "beacon_search_routes",
            args(json!({"from":"aabbccdd","to":"ff","iatas":["arn","mmx"],"limit":1})),
        )
        .await
        .unwrap();
    routes
        .execute(
            "beacon_find_cross_iata_routes",
            args(json!({"fromHash":"aa","toHash":"bb","fromIata":"mmx","toIata":"had","limit":1})),
        )
        .await
        .unwrap();
    let seen = seen.lock().unwrap();
    assert!(
        seen[0].contains("iatas=ARN%2CMMX")
            && seen[0].contains("from=aabbccdd")
            && seen[0].contains("to=ff")
    );
    assert!(
        seen[1].contains("fromHash=aa")
            && seen[1].contains("toHash=bb")
            && seen[1].contains("fromIata=MMX")
            && seen[1].contains("toIata=HAD")
    );
    task.abort();
}

#[tokio::test]
async fn strict_calendar_times_and_analytics_windows_reject_bad_inputs_before_network() {
    let (client, seen, task) = backend(|_| panic!("invalid filter reached Beacon")).await;
    let packets = PacketTools::new(client.clone());
    let analytics = AnalyticsTools::new(client.clone());
    for time in [
        "2026-02-30T15:30:00Z",
        "2026-13-01T00:00:00Z",
        "2026-02-29T00:00:00Z",
        "2026-10-04 15:30",
        "2026-10-04T15:30:00+00:00",
    ] {
        assert_eq!(
            packets
                .execute("beacon_search_packets", args(json!({"since":time})))
                .await
                .unwrap_err()
                .code,
            ErrorCode::InvalidArgument
        );
    }
    assert_eq!(
        analytics
            .execute(
                "beacon_get_network_series",
                args(json!({"since":"2026-10-05T00:00:00Z","until":"2026-10-04T00:00:00Z"}))
            )
            .await
            .unwrap_err()
            .code,
        ErrorCode::InvalidArgument
    );
    assert_eq!(
        client
            .request(Operation::Node, Some("../../admin"), &[])
            .await
            .unwrap_err()
            .code,
        ErrorCode::InvalidArgument
    );
    assert!(seen.lock().unwrap().is_empty());
    task.abort();
}

#[tokio::test]
async fn non_pageable_arrays_differentiate_truncation_and_complete_results() {
    let (client, _, task) =
        backend(|_| r#"[{"iata":"ARN"},{"iata":"MMX"},{"iata":"GOT"}]"#.into()).await;
    let tools = CoreTools::new(client);
    let truncated = tools
        .execute("beacon_list_iatas", args(json!({"limit":2})))
        .await
        .unwrap();
    assert_eq!(
        truncated["pagination"],
        json!({"hasMore":false,"truncated":true})
    );
    let complete = tools
        .execute("beacon_list_iatas", args(json!({"limit":3})))
        .await
        .unwrap();
    assert_eq!(complete["pagination"], json!({"hasMore":false}));
    task.abort();
}

#[tokio::test]
async fn incremental_json_array_keeps_escapes_strings_scalars_and_nested_values() {
    let chunks: Vec<&'static str> = vec![
        "[\"a,b\", \"c\\\"d\", ",
        "null, 12, true, {\"x\":\"y,z\"}",
        "]",
    ];
    let router = Router::new().route(
        "/api/v1/iatas",
        get(move || {
            let chunks = chunks.clone();
            async move {
                let stream = futures_util::stream::iter(
                    chunks
                        .into_iter()
                        .map(|chunk| Ok::<_, std::io::Error>(chunk.as_bytes().to_vec())),
                );
                Response::builder()
                    .status(StatusCode::OK)
                    .body(Body::from_stream(stream))
                    .unwrap()
            }
        }),
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}/", listener.local_addr().unwrap());
    let task = tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
    let client = BeaconClient::new(
        base.parse().unwrap(),
        Duration::from_secs(2),
        Duration::from_secs(2),
    )
    .expect("HTTP client configuration");
    let page = client.array_page(Operation::Iatas, &[], 10).await.unwrap();
    assert!(page.complete);
    assert_eq!(
        page.items,
        json!(["a,b", "c\"d", null, 12, true, {"x":"y,z"}])
            .as_array()
            .unwrap()
            .clone()
    );
    task.abort();
}

#[tokio::test]
async fn single_iata_filters_are_forwarded_without_implicit_widening() {
    let (client, seen, task) = backend(|_| json!({"items":[],"hasMore":false}).to_string()).await;
    let core = CoreTools::new(client.clone());
    let packet = PacketTools::new(client);
    for name in ["beacon_search_nodes", "beacon_search_observers"] {
        core.execute(name, args(json!({"iata":"arn"})))
            .await
            .unwrap();
        assert_eq!(
            core.execute(name, args(json!({"iata":"arn","iatas":["mmx"]})))
                .await
                .unwrap_err()
                .code,
            ErrorCode::InvalidArgument
        );
    }
    for name in ["beacon_search_packets", "beacon_list_channels"] {
        packet
            .execute(name, args(json!({"iata":"mmx"})))
            .await
            .unwrap();
        assert_eq!(
            packet
                .execute(name, args(json!({"iata":"arn","iatas":["mmx"]})))
                .await
                .unwrap_err()
                .code,
            ErrorCode::InvalidArgument
        );
    }
    let paths = seen.lock().unwrap();
    assert_eq!(paths.len(), 4);
    assert!(paths[0].contains("iata=ARN"));
    assert!(paths[1].contains("iata=ARN"));
    assert!(paths[2].contains("iata=MMX"));
    assert!(paths[3].contains("iata=MMX"));
    task.abort();
}

#[tokio::test]
async fn channel_numeric_cursor_is_usable_when_precise_token_is_absent() {
    let (client, seen, task) =
        backend(|_| json!({"items":[{"id":1}],"hasMore":true,"nextCursor":55}).to_string()).await;
    let packet = PacketTools::new(client);
    let result = packet
        .execute("beacon_list_channels", args(json!({"cursor":25,"limit":1})))
        .await
        .unwrap();
    assert_eq!(result["pagination"]["nextCursor"], json!({"cursor":55}));
    assert!(seen.lock().unwrap()[0].contains("cursor=25"));
    assert_eq!(
        packet
            .execute(
                "beacon_list_channels",
                args(json!({"cursor":25,"pageCursor":"opaque"}))
            )
            .await
            .unwrap_err()
            .code,
        ErrorCode::InvalidArgument
    );
    assert_eq!(seen.lock().unwrap().len(), 1);
    task.abort();
}
