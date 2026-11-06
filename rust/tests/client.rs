use std::{
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    time::Duration,
};

use axum::{
    Router,
    body::Body,
    http::{Request, StatusCode},
    response::Response,
    routing::get,
};
use beacon_mcp_rust::client::{BeaconClient, ErrorCode, Operation};

async fn mock(
    f: impl Fn(Request<Body>) -> Response + Clone + Send + Sync + 'static,
) -> (BeaconClient, tokio::task::JoinHandle<()>) {
    let router = Router::new().fallback(get(move |req: Request<Body>| {
        let f = f.clone();
        async move { f(req) }
    }));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}/", listener.local_addr().unwrap());
    let task = tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
    (
        BeaconClient::new(
            url.parse().unwrap(),
            Duration::from_millis(100),
            Duration::from_millis(250),
        ),
        task,
    )
}

fn response(status: StatusCode, body: impl Into<Body>) -> Response {
    Response::builder()
        .status(status)
        .body(body.into())
        .unwrap()
}

#[tokio::test]
async fn fixed_public_get_routes_and_encoded_queries() {
    let (client, server) = mock(|req: Request<Body>| {
        assert_eq!(req.method(), "GET");
        assert_eq!(req.uri().path(), "/api/v1/nodes");
        assert_eq!(req.uri().query(), Some("iatas=ARN%2CMMX&scope=%23se13"));
        assert_eq!(req.headers()["user-agent"], "beacon-mcp/2.1.1");
        response(
            StatusCode::OK,
            r#"{"items":[{"id":"first"}],"hasMore":false}"#,
        )
    })
    .await;
    let result = client
        .request(
            Operation::Nodes,
            None,
            &[("iatas", "ARN,MMX".into()), ("scope", "#se13".into())],
        )
        .await
        .unwrap();
    assert_eq!(result["items"][0]["id"], "first");
    server.abort();
}

#[tokio::test]
async fn rejects_invalid_ids_and_oversized_urls_before_network() {
    let (client, server) = mock(|_: Request<Body>| panic!("unsafe request reached Beacon")).await;
    for (op, id) in [
        (Operation::Node, "../admin/config"),
        (Operation::Packet, "../admin/config"),
        (Operation::Trace, "not-hex"),
    ] {
        assert_eq!(
            client.request(op, Some(id), &[]).await.unwrap_err().code,
            ErrorCode::InvalidArgument
        );
    }
    let err = client
        .request(Operation::Nodes, None, &[("name", "x".repeat(8192))])
        .await
        .unwrap_err();
    assert_eq!(err.code, ErrorCode::InvalidArgument);
    server.abort();
}

#[tokio::test]
async fn classifies_errors_and_preserves_retry_after_without_immediate_retry() {
    let calls = Arc::new(AtomicUsize::new(0));
    let (client, server) = mock({
        let calls = calls.clone();
        move |_: Request<Body>| {
            calls.fetch_add(1, Ordering::SeqCst);
            Response::builder()
                .status(503)
                .header("Retry-After", "10")
                .body(Body::from(
                    r#"{"error":{"code":"deadline_exceeded","message":"secret SQL"}}"#,
                ))
                .unwrap()
        }
    })
    .await;
    let error = client
        .request(Operation::Packets, None, &[])
        .await
        .unwrap_err();
    assert_eq!(error.code, ErrorCode::DeadlineExceeded);
    assert_eq!(error.status, Some(StatusCode::SERVICE_UNAVAILABLE));
    assert_eq!(error.retry_after.as_deref(), Some("10"));
    assert!(!error.message.contains("secret SQL"));
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    server.abort();
}

#[tokio::test]
async fn retries_transient_503_and_classifies_400_404_429() {
    let calls = Arc::new(AtomicUsize::new(0));
    let (client, server) = mock({
        let calls = calls.clone();
        move |req: Request<Body>| match req.uri().path() {
            "/api/v1/info" if calls.fetch_add(1, Ordering::SeqCst) == 0 => {
                response(StatusCode::SERVICE_UNAVAILABLE, "{}")
            }
            "/api/v1/info" => response(StatusCode::OK, r#"{"serverVersion":"2.1.1"}"#),
            "/api/v1/nodes" => response(
                StatusCode::BAD_REQUEST,
                r#"{"error":{"code":"bad_request","message":"bad filter"}}"#,
            ),
            "/api/v1/packets" => response(
                StatusCode::TOO_MANY_REQUESTS,
                r#"{"error":{"code":"rate_limited"}}"#,
            ),
            _ => response(StatusCode::NOT_FOUND, r#"{"error":{"code":"not_found"}}"#),
        }
    })
    .await;
    assert_eq!(
        client
            .request(Operation::ServerInfo, None, &[])
            .await
            .unwrap()["serverVersion"],
        "2.1.1"
    );
    assert_eq!(calls.load(Ordering::SeqCst), 2);
    assert_eq!(
        client
            .request(Operation::Nodes, None, &[])
            .await
            .unwrap_err()
            .code,
        ErrorCode::InvalidArgument
    );
    assert_eq!(
        client
            .request(Operation::Packets, None, &[])
            .await
            .unwrap_err()
            .code,
        ErrorCode::ResourceExhausted
    );
    assert_eq!(
        client
            .request(Operation::Trace, Some("aabbccdd"), &[])
            .await
            .unwrap_err()
            .code,
        ErrorCode::NotFound
    );
    server.abort();
}

#[tokio::test]
async fn rejects_invalid_json_and_oversized_bodies() {
    let (client, server) = mock(|req: Request<Body>| match req.uri().path() {
        "/api/v1/iatas" => response(StatusCode::OK, "not-json"),
        "/api/v1/regions" => response(StatusCode::OK, "x".repeat(5_242_881)),
        _ => response(StatusCode::OK, "[]"),
    })
    .await;
    assert_eq!(
        client
            .request(Operation::Iatas, None, &[])
            .await
            .unwrap_err()
            .code,
        ErrorCode::Unavailable
    );
    assert_eq!(
        client
            .request(Operation::Regions, None, &[])
            .await
            .unwrap_err()
            .code,
        ErrorCode::ResponseTooLarge
    );
    server.abort();
}

#[tokio::test]
async fn bounds_timeouts_and_supports_caller_cancellation() {
    let calls = Arc::new(AtomicUsize::new(0));
    let router = Router::new().route(
        "/api/v1/info",
        get({
            let calls = calls.clone();
            move || {
                let calls = calls.clone();
                async move {
                    calls.fetch_add(1, Ordering::SeqCst);
                    tokio::time::sleep(Duration::from_millis(300)).await;
                    axum::Json(serde_json::json!({"serverVersion": "2.1.1"}))
                }
            }
        }),
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}/", listener.local_addr().unwrap());
    let server = tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
    let client = BeaconClient::new(
        url.parse().unwrap(),
        Duration::from_millis(40),
        Duration::from_secs(1),
    );
    assert_eq!(
        client
            .request(Operation::ServerInfo, None, &[])
            .await
            .unwrap_err()
            .code,
        ErrorCode::DeadlineExceeded
    );
    let client = BeaconClient::new(
        url.parse().unwrap(),
        Duration::from_secs(1),
        Duration::from_secs(1),
    );
    let pending =
        tokio::spawn(async move { client.request(Operation::ServerInfo, None, &[]).await });
    tokio::time::sleep(Duration::from_millis(20)).await;
    pending.abort();
    assert!(pending.await.unwrap_err().is_cancelled());
    assert_eq!(calls.load(Ordering::SeqCst), 2);
    server.abort();
}

#[tokio::test]
async fn stops_large_arrays_at_item_budget_and_preserves_nested_json() {
    let route = r#"{"name":"a,\\\"b","nested":[1,{"k":"v"}]}"#;
    let body = format!("[{}]", vec![route; 150_000].join(","));
    assert!(body.len() > 5_242_880);
    let (client, server) =
        mock(move |_: Request<Body>| response(StatusCode::OK, body.clone())).await;
    let page = client
        .array_page(
            Operation::RouteSearch,
            &[("from", "aa".into()), ("to", "bb".into())],
            3,
        )
        .await
        .unwrap();
    assert_eq!(page.items.len(), 3);
    assert_eq!(page.items[0]["nested"][1]["k"], "v");
    assert!(!page.complete);
    server.abort();
}

#[tokio::test]
async fn returns_small_complete_array_and_rejects_invalid_array_requests() {
    let (client, server) =
        mock(|_: Request<Body>| response(StatusCode::OK, r#"["ARN", "MMX"]"#)).await;
    let page = client.array_page(Operation::Iatas, &[], 3).await.unwrap();
    assert_eq!(page.items, vec!["ARN", "MMX"]);
    assert!(page.complete);
    assert_eq!(
        client
            .array_page(Operation::Node, &[], 3)
            .await
            .unwrap_err()
            .code,
        ErrorCode::InvalidArgument
    );
    server.abort();
}

#[tokio::test]
async fn accepts_bounded_non_array_page_envelopes_and_rejects_invalid_fallback() {
    let (client, server) = mock(|req: Request<Body>| match req.uri().path() {
        "/api/v1/iatas" => response(StatusCode::OK, r#"{"items":[1,2],"hasMore":false}"#),
        "/api/v1/regions" => response(StatusCode::OK, r#"{"items":[1,2,3]}"#),
        _ => response(StatusCode::OK, r#"{"items":"not an array"}"#),
    })
    .await;
    assert_eq!(
        client
            .array_page(Operation::Iatas, &[], 3)
            .await
            .unwrap()
            .items,
        vec![1, 2]
    );
    let bounded = client.array_page(Operation::Regions, &[], 2).await.unwrap();
    assert_eq!(bounded.items, vec![1, 2]);
    assert!(!bounded.complete);
    assert_eq!(
        client
            .array_page(Operation::RouteSearch, &[], 3)
            .await
            .unwrap_err()
            .code,
        ErrorCode::Unavailable
    );
    server.abort();
}
