use beacon_mcp_rust::server;
use tokio_util::sync::CancellationToken;

#[tokio::test]
async fn health_readiness_and_mcp_discovery() {
    let token = CancellationToken::new();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let server_token = token.clone();
    let task = tokio::spawn(async move {
        axum::serve(listener, server::router(server_token))
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
