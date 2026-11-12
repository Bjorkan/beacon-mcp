#![forbid(unsafe_code)]
#![cfg_attr(
    not(test),
    deny(clippy::unwrap_used, clippy::expect_used, clippy::panic)
)]

use beacon_mcp_rust::{
    client::BeaconClient,
    config::Config,
    server,
    subscriptions::{Live, bridge},
};
use std::{
    io::{Read, Write},
    net::{TcpStream, ToSocketAddrs},
    time::Duration,
};
use tokio_util::sync::CancellationToken;

fn healthcheck() -> bool {
    let port = match std::env::var("PORT")
        .unwrap_or_else(|_| "3000".into())
        .parse::<u16>()
    {
        Ok(port) => port,
        Err(_) => return false,
    };
    for host in ["127.0.0.1", "::1"] {
        let Ok(addresses) = (host, port).to_socket_addrs() else {
            continue;
        };
        for address in addresses {
            if let Ok(mut stream) = TcpStream::connect_timeout(&address, Duration::from_secs(1)) {
                if stream
                    .set_read_timeout(Some(Duration::from_secs(1)))
                    .is_err()
                    || stream
                        .set_write_timeout(Some(Duration::from_secs(1)))
                        .is_err()
                {
                    continue;
                }
                if stream
                    .write_all(
                        b"GET /healthz HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n",
                    )
                    .is_ok()
                {
                    let mut response = [0; 32];
                    if stream
                        .read(&mut response)
                        .is_ok_and(|size| response[..size].starts_with(b"HTTP/1.1 200"))
                    {
                        return true;
                    }
                }
            }
        }
    }
    false
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    if std::env::args().nth(1).as_deref() == Some("--healthcheck") {
        std::process::exit(if healthcheck() { 0 } else { 1 });
    }
    let config = Config::from_env()?;
    let level = if config.log_level == "silent" {
        "off"
    } else if config.log_level == "fatal" {
        // Rust tracing has no fatal level; report fatal messages at ERROR.
        "error"
    } else {
        &config.log_level
    };
    let mut filter = tracing_subscriber::EnvFilter::new(level);
    if config.log_level == "info" {
        // Keep SDK WARN/ERROR diagnostics, not a multi-line service lifecycle
        // trace for every stateless MCP request. Debug/trace remain opt-in.
        filter = filter.add_directive("rmcp=warn".parse()?);
    }
    // Docker captures logs as text; ANSI colour escapes make them harder to
    // search, compare and process in log collectors.
    tracing_subscriber::fmt()
        .with_ansi(false)
        .with_env_filter(filter)
        .init();
    let listener = tokio::net::TcpListener::bind(config.bind).await?;
    let shutdown = CancellationToken::new();
    tracing::info!(address = %listener.local_addr()?, "Beacon MCP Rust foundation listening");
    let client = BeaconClient::new(
        config.beacon_base_url.clone(),
        config.beacon_timeout,
        config.stats_timeout,
    )?;
    let live = config.subscriptions.enabled.then(|| {
        Live::new(
            config.subscriptions.max_streams,
            config.subscriptions.debounce,
            shutdown.clone(),
        )
    });
    let bridge_task = live.as_ref().map(|live| {
        tokio::spawn(bridge(
            client.clone(),
            config.subscriptions.ws_url.clone(),
            live.clone(),
        ))
    });
    let serve = async {
        axum::serve(
            listener,
            server::router_with_live(shutdown.clone(), client, live),
        )
        .with_graceful_shutdown(shutdown.clone().cancelled_owned())
        .await
    };
    tokio::pin!(serve);
    let signal = async {
        #[cfg(unix)]
        {
            let mut terminate =
                tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())?;
            tokio::select! {
                result = tokio::signal::ctrl_c() => result?,
                _ = terminate.recv() => {},
            }
        }
        #[cfg(not(unix))]
        tokio::signal::ctrl_c().await?;
        Ok::<(), std::io::Error>(())
    };
    tokio::select! {
        result = &mut serve => result?,
        result = signal => {
            result?;
            shutdown.cancel();
            tokio::time::timeout(config.shutdown_grace, &mut serve).await??;
        }
    }
    shutdown.cancel();
    if let Some(mut task) = bridge_task {
        match tokio::time::timeout(Duration::from_secs(2), &mut task).await {
            Ok(Ok(())) => {}
            Ok(Err(error)) => tracing::warn!(%error, "WebSocket bridge exited with an error"),
            Err(_) => {
                task.abort();
                tracing::warn!("WebSocket bridge aborted after shutdown deadline");
            }
        }
    }
    Ok(())
}
