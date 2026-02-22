use axum::{extract::State, response::IntoResponse};

use crate::state::AppState;

pub async fn metrics_handler(State(state): State<AppState>) -> impl IntoResponse {
    state.metrics.render()
}
