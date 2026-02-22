use axum::http::HeaderMap;

use crate::state::AppState;

pub fn is_authorized(headers: &HeaderMap, state: &AppState) -> bool {
    match &state.api_token {
        None => true,
        Some(expected) => headers
            .get("x-api-token")
            .and_then(|h| h.to_str().ok())
            .is_some_and(|token| token == expected),
    }
}
