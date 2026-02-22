use axum::Json;
use common::RpcResponse;
use serde::Serialize;

pub fn json_from_response<T: Serialize>(response: RpcResponse<T>) -> Json<serde_json::Value> {
    Json(serde_json::to_value(response).unwrap_or_else(
        |_| serde_json::json!({ "jsonrpc": "2.0", "id": null, "error": { "code": -32603, "message": "internal serialization error" } }),
    ))
}
