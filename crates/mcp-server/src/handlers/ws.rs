use axum::{
    body::{Body, to_bytes},
    extract::State,
    extract::ws::{Message, WebSocket, WebSocketUpgrade},
    http::Request,
    response::IntoResponse,
};
use tower::ServiceExt;

use crate::{app, state::AppState};

pub async fn ws_handler(State(state): State<AppState>, ws: WebSocketUpgrade) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_socket(state, socket))
}

async fn handle_socket(state: AppState, mut socket: WebSocket) {
    while let Some(Ok(msg)) = socket.recv().await {
        let Message::Text(text) = msg else {
            if matches!(msg, Message::Close(_)) {
                break;
            }
            continue;
        };

        let req = match Request::builder()
            .method("POST")
            .uri("/mcp")
            .header("content-type", "application/json")
            .body(Body::from(text.to_string()))
        {
            Ok(r) => r,
            Err(_) => {
                let _ = socket
                    .send(Message::Text(
                        "{\"jsonrpc\":\"2.0\",\"id\":null,\"error\":{\"code\":-32700,\"message\":\"invalid request\"}}"
                            .to_string()
                            .into(),
                    ))
                    .await;
                continue;
            }
        };

        let response = match app::router(state.clone()).oneshot(req).await {
            Ok(res) => res,
            Err(_) => {
                let _ = socket
                    .send(Message::Text(
                        "{\"jsonrpc\":\"2.0\",\"id\":null,\"error\":{\"code\":-32603,\"message\":\"internal error\"}}"
                            .to_string()
                            .into(),
                    ))
                    .await;
                continue;
            }
        };

        let body = match to_bytes(response.into_body(), usize::MAX).await {
            Ok(bytes) => bytes,
            Err(_) => {
                let _ = socket
                    .send(Message::Text(
                        "{\"jsonrpc\":\"2.0\",\"id\":null,\"error\":{\"code\":-32603,\"message\":\"internal error\"}}"
                            .to_string()
                            .into(),
                    ))
                    .await;
                continue;
            }
        };
        let text = String::from_utf8(body.to_vec()).unwrap_or_else(|_| {
            "{\"jsonrpc\":\"2.0\",\"id\":null,\"error\":{\"code\":-32603,\"message\":\"internal error\"}}"
                .to_string()
        });
        let _ = socket.send(Message::Text(text.into())).await;
    }
}
