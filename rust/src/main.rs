use beacon_mcp_rust::{config::Config, server};
use tokio_util::sync::CancellationToken;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()),
        )
        .init();
    let config = Config::from_env()?;
    let listener = tokio::net::TcpListener::bind(config.bind).await?;
    let shutdown = CancellationToken::new();
    tracing::info!(address = %listener.local_addr()?, "Beacon MCP Rust foundation listening");
    let serve = async {
        axum::serve(listener, server::router(shutdown.clone()))
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
    Ok(())
}
