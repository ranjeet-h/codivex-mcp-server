use axum::middleware;
use axum::{
    Router,
    routing::{get, post},
};

use crate::handlers::{
    health::health,
    mcp::mcp_handler,
    metrics::metrics_handler,
    port_diagnostics::port_diagnostics_handler,
    schemas::schemas_handler,
    sse::sse_handler,
    telemetry::{telemetry_handler, telemetry_sse_handler},
    ws::ws_handler,
};
use crate::middleware::trace_with_correlation;
use crate::state::AppState;

pub fn router(state: AppState) -> Router {
    Router::new()
        .route("/health", get(health))
        .route("/port-diagnostics", get(port_diagnostics_handler))
        .route("/metrics", get(metrics_handler))
        .route("/telemetry", get(telemetry_handler))
        .route("/telemetry/sse", get(telemetry_sse_handler))
        .route("/schemas", get(schemas_handler))
        .route("/mcp", post(mcp_handler))
        .route("/mcp/sse", get(sse_handler))
        .route("/mcp/ws", get(ws_handler))
        .layer(middleware::from_fn(trace_with_correlation))
        .with_state(state)
}
