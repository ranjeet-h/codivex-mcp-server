use axum::{Json, extract::State};

use crate::state::{AppState, PortDiagnostics};

pub async fn port_diagnostics_handler(State(state): State<AppState>) -> Json<PortDiagnostics> {
    Json(PortDiagnostics {
        mcp_port: state.runtime_ports.mcp_port,
        ui_port: state.runtime_ports.ui_port,
        metrics_port: state.runtime_ports.metrics_port,
        conflicts_resolved: state.port_conflicts_resolved,
        pid: state.pid,
    })
}
