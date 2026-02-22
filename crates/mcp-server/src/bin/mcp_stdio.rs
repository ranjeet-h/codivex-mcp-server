use axum::{
    body::{Body, to_bytes},
    http::Request,
};
use common::ports::resolve_runtime_ports;
use mcp_server::{app, state::AppState};
use tokio::io::{self, AsyncBufReadExt, AsyncWriteExt, BufReader};
use tower::ServiceExt;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_target(false)
        .json()
        .with_writer(std::io::stderr)
        .init();

    let cwd = std::env::current_dir()?;
    let runtime_ports = resolve_runtime_ports(&cwd, 38080, 38181, Some(38281))?;
    let state = AppState::from_env(runtime_ports, false)?;
    mcp_server::services::indexing::spawn_background_indexing(state.clone());

    let stdin = io::stdin();
    let mut lines = BufReader::new(stdin).lines();
    let mut stdout = io::stdout();

    while let Some(line) = lines.next_line().await? {
        if line.trim().is_empty() {
            continue;
        }

        let req = match Request::builder()
            .method("POST")
            .uri("/mcp")
            .header("content-type", "application/json")
            .body(Body::from(line))
        {
            Ok(v) => v,
            Err(_) => {
                stdout
                    .write_all(
                        b"{\"jsonrpc\":\"2.0\",\"id\":null,\"error\":{\"code\":-32700,\"message\":\"invalid request\"}}\n",
                    )
                    .await?;
                stdout.flush().await?;
                continue;
            }
        };

        let res = app::router(state.clone()).oneshot(req).await;
        match res {
            Ok(response) => {
                let body = to_bytes(response.into_body(), usize::MAX).await?;
                stdout.write_all(&body).await?;
                stdout.write_all(b"\n").await?;
                stdout.flush().await?;
            }
            Err(_) => {
                stdout
                    .write_all(
                        b"{\"jsonrpc\":\"2.0\",\"id\":null,\"error\":{\"code\":-32603,\"message\":\"internal error\"}}\n",
                    )
                    .await?;
                stdout.flush().await?;
            }
        }
    }

    Ok(())
}
