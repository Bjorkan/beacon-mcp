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
                let _ = stream.set_read_timeout(Some(Duration::from_secs(1)));
                let _ = stream.set_write_timeout(Some(Duration::from_secs(1)));
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
    tracing_subscriber::fmt()
        .with_env_filter(if config.log_level == "silent" {
            "off".to_owned()
        } else if config.log_level == "fatal" {
            // Rust tracing has no fatal level; report fatal messages at ERROR.
            "error".to_owned()
        } else {
            config.log_level.clone()
        })
        .init();
    let listener = tokio::net::TcpListener::bind(config.bind).await?;
    let shutdown = CancellationToken::new();
    tracing::info!(address = %listener.local_addr()?, "Beacon MCP Rust foundation listening");
    let client = BeaconClient::new(
        config.beacon_base_url.clone(),
        config.beacon_timeout,
        config.stats_timeout,
    );
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
    if let Some(task) = bridge_task {
        let _ = tokio::time::timeout(std::time::Duration::from_secs(2), task).await;
    }
    Ok(())
}
