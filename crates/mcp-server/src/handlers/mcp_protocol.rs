use common::{OpenLocationParams, SearchCodeParams, schema_bundle};
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InitializeParams {
    #[serde(default)]
    pub protocol_version: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InitializeResult {
    pub protocol_version: String,
    pub server_info: ServerInfo,
    pub capabilities: ServerCapabilities,
}

#[derive(Debug, Clone, Serialize)]
pub struct ServerInfo {
    pub name: String,
    pub version: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ServerCapabilities {
    pub tools: ToolsCapability,
}

#[derive(Debug, Clone, Serialize)]
pub struct ToolsCapability {
    pub list_changed: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolsListResult {
    pub tools: Vec<ToolDescriptor>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ResourcesListResult {
    pub resources: Vec<Value>,
}

#[derive(Debug, Clone, Serialize)]
pub struct PromptsListResult {
    pub prompts: Vec<Value>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolDescriptor {
    pub name: String,
    pub title: String,
    pub description: String,
    pub input_schema: Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output_schema: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub annotations: Option<ToolAnnotations>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolAnnotations {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub read_only_hint: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub destructive_hint: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub idempotent_hint: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub open_world_hint: Option<bool>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolCallParams {
    pub name: String,
    #[serde(default)]
    pub arguments: Value,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolCallResult {
    pub content: Vec<ToolContent>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub structured_content: Option<Value>,
    #[serde(default)]
    pub is_error: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct ToolContent {
    #[serde(rename = "type")]
    pub kind: String,
    pub text: String,
}

pub fn initialize_result(params: Option<InitializeParams>) -> InitializeResult {
    let requested = params
        .and_then(|p| p.protocol_version)
        .unwrap_or_else(|| "2025-06-18".to_string());
    let protocol_version = if requested.trim().is_empty() {
        "2025-06-18".to_string()
    } else {
        requested
    };
    InitializeResult {
        protocol_version,
        server_info: ServerInfo {
            name: "codivex-mcp".to_string(),
            version: env!("CARGO_PKG_VERSION").to_string(),
        },
        capabilities: ServerCapabilities {
            tools: ToolsCapability {
                list_changed: false,
            },
        },
    }
}

pub fn tools_list_result() -> anyhow::Result<ToolsListResult> {
    let schemas = schema_bundle();
    let search_schema = serde_json::to_value(schemas.search_code_params)?;
    let open_schema = serde_json::to_value(schemas.open_location_params)?;
    let search_output_schema = serde_json::json!({
        "type": "object",
        "properties": {
            "items": {
                "type": "array",
                "items": {
                    "type": "object",
                    "properties": {
                        "file": { "type": "string" },
                        "function": { "type": "string" },
                        "start_line": { "type": "integer", "minimum": 1 },
                        "end_line": { "type": "integer", "minimum": 1 },
                        "code_block": { "type": "string" }
                    },
                    "required": ["file", "function", "start_line", "end_line", "code_block"]
                }
            }
        },
        "required": ["items"]
    });
    let open_output_schema = serde_json::json!({
        "type": "object",
        "properties": {
            "path": { "type": "string" },
            "line_start": { "type": "integer", "minimum": 1 },
            "line_end": { "type": "integer", "minimum": 1 }
        },
        "required": ["path", "line_start", "line_end"]
    });
    Ok(ToolsListResult {
        tools: vec![
            ToolDescriptor {
                name: "searchCode".to_string(),
                title: "Search Code".to_string(),
                description: "Search indexed code in exactly one project and return ranked chunks (file + line range + snippet). Prefer exact symbols first; pass repoFilter for project scope when multiple repos are indexed.".to_string(),
                input_schema: search_schema,
                output_schema: Some(search_output_schema),
                annotations: Some(ToolAnnotations {
                    read_only_hint: Some(true),
                    destructive_hint: Some(false),
                    idempotent_hint: Some(true),
                    open_world_hint: Some(false),
                }),
            },
            ToolDescriptor {
                name: "openLocation".to_string(),
                title: "Open Location".to_string(),
                description: "Validate and open a source file location by path and line range. Use after searchCode to fetch exact lines for reasoning or edits."
                    .to_string(),
                input_schema: open_schema,
                output_schema: Some(open_output_schema),
                annotations: Some(ToolAnnotations {
                    read_only_hint: Some(true),
                    destructive_hint: Some(false),
                    idempotent_hint: Some(true),
                    open_world_hint: Some(false),
                }),
            },
        ],
    })
}

pub fn resources_list_result() -> ResourcesListResult {
    ResourcesListResult {
        resources: Vec::new(),
    }
}

pub fn prompts_list_result() -> PromptsListResult {
    PromptsListResult {
        prompts: Vec::new(),
    }
}

pub fn parse_search_arguments(value: Value) -> Result<SearchCodeParams, String> {
    serde_json::from_value::<SearchCodeParams>(value).map_err(|e| format!("invalid args: {e}"))
}

pub fn parse_open_arguments(value: Value) -> Result<OpenLocationParams, String> {
    serde_json::from_value::<OpenLocationParams>(value).map_err(|e| format!("invalid args: {e}"))
}
