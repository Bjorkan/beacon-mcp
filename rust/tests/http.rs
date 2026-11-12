use beacon_mcp_rust::{client::BeaconClient, server};
use tokio_util::sync::CancellationToken;

#[tokio::test]
async fn invalid_tool_inputs_are_plain_errors_but_upstream_400_is_structured() {
    let upstream = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let backend = format!("http://{}/", upstream.local_addr().unwrap());
    let upstream_task = tokio::spawn(async move {
        axum::serve(
            upstream,
            axum::Router::new().fallback(|| async {
                (
                    axum::http::StatusCode::BAD_REQUEST,
                    axum::Json(serde_json::json!({
                        "error": {"code":"bad_request", "message":"bad filter"}
                    })),
                )
            }),
        )
        .await
        .unwrap();
    });
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let root = format!("http://{}/", listener.local_addr().unwrap());
    let shutdown = CancellationToken::new();
    let client = BeaconClient::new(
        backend.parse().unwrap(),
        std::time::Duration::from_secs(2),
        std::time::Duration::from_secs(2),
    )
    .expect("HTTP client configuration");
    let app = server::router(shutdown.clone(), client);
    let task = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    let http = reqwest::Client::new();
    for (name, args) in [
        ("beacon_get_packet", serde_json::json!({"hash":"xyz"})),
        (
            "beacon_search_nodes",
            serde_json::json!({"iatas":[" arn "]}),
        ),
        (
            "beacon_search_nodes",
            serde_json::json!({"sort":"name","cursor":1}),
        ),
        ("beacon_compare_observers", serde_json::json!({})),
    ] {
        let reply: serde_json::Value = http
            .post(&root)
            .header("Accept", "application/json, text/event-stream")
            .header("MCP-Protocol-Version", "2026-07-28")
            .header("Mcp-Method", "tools/call")
            .header("Mcp-Name", name)
            .json(
                &serde_json::json!({"jsonrpc":"2.0", "id":1, "method":"tools/call", "params":{
                    "name":name,"arguments":args,"_meta":{
                        "io.modelcontextprotocol/protocolVersion":"2026-07-28",
                        "io.modelcontextprotocol/clientInfo":{"name":"test","version":"1"},
                        "io.modelcontextprotocol/clientCapabilities":{}
                    }
                }}),
            )
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
        assert_eq!(reply["result"]["isError"], true, "{reply}");
        assert!(
            reply["result"].get("structuredContent").is_none(),
            "{reply}"
        );
        assert!(
            reply["result"]["content"][0]["text"]
                .as_str()
                .unwrap()
                .starts_with("Input validation error: Invalid arguments for tool")
        );
    }
    for (name, arguments, message) in [
        (
            "beacon_search_packets",
            serde_json::json!({"since":"2026-02-30T15:30:00Z"}),
            "since must be a valid calendar timestamp",
        ),
        (
            "beacon_search_messages",
            serde_json::json!({"since":"2026-02-30T15:30:00Z"}),
            "since must be a valid calendar timestamp",
        ),
        (
            "beacon_get_network_series",
            serde_json::json!({"since":"2026-10-05T00:00:00Z","until":"2026-10-04T00:00:00Z"}),
            "since must be earlier than until",
        ),
    ] {
        let reply: serde_json::Value = http
            .post(&root)
            .header("Accept", "application/json, text/event-stream")
            .header("MCP-Protocol-Version", "2026-07-28")
            .header("Mcp-Method", "tools/call")
            .header("Mcp-Name", name)
            .json(
                &serde_json::json!({"jsonrpc":"2.0", "id":3, "method":"tools/call", "params":{
                    "name":name,"arguments":arguments,"_meta":{
                        "io.modelcontextprotocol/protocolVersion":"2026-07-28",
                        "io.modelcontextprotocol/clientInfo":{"name":"test","version":"1"},
                        "io.modelcontextprotocol/clientCapabilities":{}
                    }
                }}),
            )
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
        assert_eq!(
            reply["result"]["structuredContent"]["error"],
            serde_json::json!({"code":"INVALID_ARGUMENT","type":"ValidationError"})
        );
        assert_eq!(reply["result"]["content"][0]["text"], message);
    }
    let name = "beacon_get_server_info";
    let reply: serde_json::Value = http
        .post(&root)
        .header("Accept", "application/json, text/event-stream")
        .header("MCP-Protocol-Version", "2026-07-28")
        .header("Mcp-Method", "tools/call")
        .header("Mcp-Name", name)
        .json(
            &serde_json::json!({"jsonrpc":"2.0", "id":2, "method":"tools/call", "params":{
                "name":name,"arguments":{},"_meta":{
                    "io.modelcontextprotocol/protocolVersion":"2026-07-28",
                    "io.modelcontextprotocol/clientInfo":{"name":"test","version":"1"},
                    "io.modelcontextprotocol/clientCapabilities":{}
                }
            }}),
        )
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(
        reply["result"]["structuredContent"]["error"]["code"],
        "INVALID_ARGUMENT"
    );
    assert_eq!(reply["result"]["structuredContent"]["error"]["status"], 400);
    shutdown.cancel();
    task.abort();
    upstream_task.abort();
}

#[tokio::test]
async fn health_readiness_and_mcp_discovery() {
    let token = CancellationToken::new();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let server_token = token.clone();
    let task = tokio::spawn(async move {
        let client = BeaconClient::new(
            "http://127.0.0.1:9/".parse().unwrap(),
            std::time::Duration::from_secs(1),
            std::time::Duration::from_secs(1),
        )
        .expect("HTTP client configuration");
        axum::serve(listener, server::router(server_token, client))
            .await
            .unwrap();
    });
    let client = reqwest::Client::new();
    let root = format!("http://{addr}");
    for path in ["healthz", "readyz"] {
        let response: serde_json::Value = client
            .get(format!("{root}/{path}"))
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
        assert_eq!(response["status"], "ok");
    }
    let discovery = client
        .post(format!("{root}/"))
        .header("Accept", "application/json, text/event-stream")
        .header("MCP-Protocol-Version", "2026-07-28")
        .header("Mcp-Method", "server/discover")
        .json(&serde_json::json!({
            "jsonrpc": "2.0", "id": 1, "method": "server/discover",
            "params": { "_meta": {
                "io.modelcontextprotocol/protocolVersion": "2026-07-28",
                "io.modelcontextprotocol/clientInfo": {"name": "rust-test", "version": "1.0.0"},
                "io.modelcontextprotocol/clientCapabilities": {}
            }}
        }))
        .send()
        .await
        .unwrap();
    assert!(
        discovery.status().is_success(),
        "{}",
        discovery.text().await.unwrap()
    );
    token.cancel();
    task.abort();
}

#[test]
fn shutdown_grace_does_not_limit_normal_uptime() {
    use std::{
        process::{Command, Stdio},
        thread,
        time::Duration,
    };
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    drop(listener);
    let mut child = Command::new(env!("CARGO_BIN_EXE_beacon-mcp-rust"))
        .env("HOST", "127.0.0.1")
        .env("PORT", port.to_string())
        .env("BEACON_BASE_URL", "http://127.0.0.1:9/")
        .env("SHUTDOWN_GRACE_MS", "100")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    thread::sleep(Duration::from_millis(400));
    let exited = child.try_wait().unwrap();
    if exited.is_none() {
        child.kill().unwrap();
        child.wait().unwrap();
    }
    assert!(
        exited.is_none(),
        "shutdown grace expired before a shutdown signal: {exited:?}"
    );
}

#[test]
fn silent_logging_and_ipv6_bind_are_effective_at_runtime() {
    use std::{
        io::{Read, Write},
        net::{TcpListener, TcpStream},
        process::{Command, Stdio},
        thread,
        time::Duration,
    };

    // Some container hosts intentionally disable IPv6; the configuration
    // parser is tested separately even when an IPv6 runtime probe is unavailable.
    let Ok(reservation) = TcpListener::bind("[::1]:0") else {
        return;
    };
    let port = reservation.local_addr().unwrap().port();
    drop(reservation);
    let mut child = Command::new(env!("CARGO_BIN_EXE_beacon-mcp-rust"))
        .env("HOST", "::1")
        .env("PORT", port.to_string())
        .env("BEACON_BASE_URL", "http://127.0.0.1:9/")
        .env("LOG_LEVEL", "silent")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let mut responded = false;
    for _ in 0..50 {
        if let Ok(mut stream) = TcpStream::connect(("::1", port)) {
            stream
                .set_read_timeout(Some(Duration::from_secs(1)))
                .unwrap();
            stream
                .write_all(b"GET /healthz HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n")
                .unwrap();
            let mut response = String::new();
            stream.read_to_string(&mut response).unwrap();
            responded = response.contains("200 OK") && response.contains("\"status\":\"ok\"");
            break;
        }
        thread::sleep(Duration::from_millis(20));
    }
    child.kill().unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(responded, "IPv6 health request did not succeed");
    assert!(
        output.stdout.is_empty() && output.stderr.is_empty(),
        "LOG_LEVEL=silent emitted logs"
    );
}
