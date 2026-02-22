pub mod config;
pub mod ports;
pub mod projects;

use schemars::JsonSchema;
use schemars::Schema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub struct RpcRequest {
    pub jsonrpc: String,
    pub id: RpcId,
    pub method: String,
    #[serde(default)]
    pub params: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(untagged)]
pub enum RpcId {
    String(String),
    Number(i64),
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq)]
pub struct RpcResponse<T> {
    pub jsonrpc: &'static str,
    pub id: RpcId,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<T>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<RpcError>,
}

impl<T> RpcResponse<T> {
    pub fn ok(id: RpcId, result: T) -> Self {
        Self {
            jsonrpc: "2.0",
            id,
            result: Some(result),
            error: None,
        }
    }

    pub fn err(id: RpcId, code: i64, message: impl Into<String>) -> Self {
        Self {
            jsonrpc: "2.0",
            id,
            result: None,
            error: Some(RpcError {
                code,
                message: message.into(),
            }),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub struct RpcError {
    pub code: i64,
    pub message: String,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub enum RpcErrorCode {
    ParseError,
    InvalidParams,
    MethodNotFound,
    IndexUnavailable,
    Timeout,
    Internal,
}

impl RpcErrorCode {
    pub const fn as_i64(self) -> i64 {
        match self {
            Self::ParseError => -32700,
            Self::InvalidParams => -32602,
            Self::MethodNotFound => -32601,
            Self::IndexUnavailable => -32010,
            Self::Timeout => -32011,
            Self::Internal => -32603,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub struct SearchCodeParams {
    pub query: String,
    #[serde(default = "default_top_k", alias = "topK")]
    pub top_k: usize,
    #[serde(default, alias = "repoFilter")]
    pub repo_filter: Option<String>,
}

fn default_top_k() -> usize {
    5
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub struct SearchResultItem {
    pub file: String,
    pub function: String,
    pub start_line: usize,
    pub end_line: usize,
    pub code_block: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub struct SearchCodeResult {
    pub items: Vec<SearchResultItem>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub struct OpenLocationParams {
    pub path: String,
    #[serde(alias = "lineStart")]
    pub line_start: usize,
    #[serde(alias = "lineEnd")]
    pub line_end: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub struct OpenLocationResult {
    pub path: String,
    pub line_start: usize,
    pub line_end: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub struct CodeChunk {
    pub id: String,
    pub fingerprint: String,
    pub file_path: String,
    pub language: String,
    pub symbol: Option<String>,
    pub start_line: usize,
    pub end_line: usize,
    pub start_char: usize,
    pub end_char: usize,
    pub content: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq)]
pub struct SearchScoredChunk {
    pub chunk: CodeChunk,
    pub lexical_score: Option<f32>,
    pub vector_score: Option<f32>,
    pub fused_score: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq)]
pub struct SchemaBundle {
    pub search_code_params: Schema,
    pub search_code_result: Schema,
    pub open_location_params: Schema,
    pub open_location_result: Schema,
}

pub fn schema_bundle() -> SchemaBundle {
    SchemaBundle {
        search_code_params: schemars::schema_for!(SearchCodeParams),
        search_code_result: schemars::schema_for!(SearchCodeResult),
        open_location_params: schemars::schema_for!(OpenLocationParams),
        open_location_result: schemars::schema_for!(OpenLocationResult),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rpc_response_ok_sets_fields() {
        let response = RpcResponse::ok(RpcId::Number(1), SearchCodeResult { items: Vec::new() });
        assert_eq!(response.jsonrpc, "2.0");
        assert!(response.result.is_some());
        assert!(response.error.is_none());
    }

    #[test]
    fn schema_bundle_generates() {
        let schemas = schema_bundle();
        let search = serde_json::to_string(&schemas.search_code_params).expect("serialize schema");
        assert!(!search.is_empty());
    }
}
