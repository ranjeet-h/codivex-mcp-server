use std::net::{IpAddr, Ipv4Addr, SocketAddr};

use common::ports::resolve_runtime_ports;
use mcp_server::services::indexing::spawn_background_indexing;
use mcp_server::{app, state::AppState};
use tracing::info;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt().with_target(false).json().init();

    let cwd = std::env::current_dir()?;
    let preferred_mcp = std::env::var("MCP_PORT")
        .ok()
        .and_then(|v| v.parse::<u16>().ok())
        .unwrap_or(38080);
    let preferred_ui = std::env::var("UI_PORT")
        .ok()
        .and_then(|v| v.parse::<u16>().ok())
        .unwrap_or(38181);
    let runtime_ports = resolve_runtime_ports(&cwd, preferred_mcp, preferred_ui, Some(38281))?;
    let port_conflicts_resolved =
        runtime_ports.mcp_port != preferred_mcp || runtime_ports.ui_port != preferred_ui;

    let addr = bind_addr_from_env(runtime_ports.mcp_port)?;
    let state = AppState::from_env(runtime_ports.clone(), port_conflicts_resolved)?;
    spawn_background_indexing(state.clone());
    info!("mcp-server listening on http://{addr}");
    info!("MCP JSON-RPC endpoint: http://{addr}/mcp");
    info!("MCP SSE endpoint: http://{addr}/mcp/sse");
    info!("Metrics endpoint: http://{addr}/metrics");
    info!("Allocated UI port: {}", runtime_ports.ui_port);
    if let Some(metrics_port) = runtime_ports.metrics_port {
        info!("Reserved metrics port: {metrics_port}");
    }

    let listener = tokio::net::TcpListener::bind(addr).await?;
    let shutdown_state = state.clone();
    axum::serve(listener, app::router(state.clone()))
        .with_graceful_shutdown(async move {
            shutdown_signal().await;
            shutdown_state.begin_shutdown();
            if let Err(err) = shutdown_state.persist_runtime_state().await {
                tracing::warn!(error = %err, "failed persisting runtime state during shutdown");
            }
        })
        .await?;
    Ok(())
}

async fn shutdown_signal() {
    #[cfg(unix)]
    {
        use tokio::signal::unix::{SignalKind, signal};
        let mut terminate =
            signal(SignalKind::terminate()).expect("signal handler for SIGTERM should install");
        tokio::select! {
            _ = tokio::signal::ctrl_c() => {}
            _ = terminate.recv() => {}
        }
    }
    #[cfg(not(unix))]
    {
        let _ = tokio::signal::ctrl_c().await;
    }
}

fn bind_addr_from_env(port: u16) -> anyhow::Result<SocketAddr> {
    let host = std::env::var("MCP_BIND_ADDR").unwrap_or_else(|_| "127.0.0.1".to_string());
    let allow_non_local = std::env::var("MCP_ALLOW_NON_LOCAL")
        .map(|v| v.eq_ignore_ascii_case("true"))
        .unwrap_or(false);

    build_bind_addr(&host, port, allow_non_local)
}

fn build_bind_addr(host: &str, port: u16, allow_non_local: bool) -> anyhow::Result<SocketAddr> {
    let ip = host.parse::<IpAddr>()?;
    let is_local_default = ip == IpAddr::V4(Ipv4Addr::LOCALHOST);

    if !is_local_default && !allow_non_local {
        anyhow::bail!("non-local bind requested for {ip}, set MCP_ALLOW_NON_LOCAL=true to opt in");
    }

    Ok(SocketAddr::new(ip, port))
}

#[cfg(test)]
mod tests {
    use super::build_bind_addr;

    #[test]
    fn defaults_to_localhost() {
        let addr = build_bind_addr("127.0.0.1", 38080, false).expect("default bind addr");
        assert_eq!(addr.ip().to_string(), "127.0.0.1");
        assert_eq!(addr.port(), 38080);
    }

    #[test]
    fn rejects_non_local_without_opt_in() {
        let err = build_bind_addr("0.0.0.0", 38080, false).expect_err("expected rejection");
        assert!(err.to_string().contains("MCP_ALLOW_NON_LOCAL=true"));
    }
}
