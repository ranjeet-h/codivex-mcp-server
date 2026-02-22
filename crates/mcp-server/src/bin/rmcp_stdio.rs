#![cfg_attr(not(feature = "rmcp-integration"), allow(dead_code))]

#[cfg(feature = "rmcp-integration")]
use std::path::PathBuf;

#[cfg(feature = "rmcp-integration")]
use common::{OpenLocationResult, SearchCodeResult};
#[cfg(feature = "rmcp-integration")]
use rmcp::{
    ErrorData as McpError, ServerHandler, ServiceExt,
    handler::server::{router::tool::ToolRouter, wrapper::Parameters},
    model::*,
    tool, tool_handler, tool_router,
    transport::stdio,
};
#[cfg(feature = "rmcp-integration")]
use schemars::JsonSchema;
#[cfg(feature = "rmcp-integration")]
use serde::{Deserialize, Serialize};

#[cfg(feature = "rmcp-integration")]
#[derive(Clone)]
struct CodivexRmcpServer {
    cwd: PathBuf,
    tool_router: ToolRouter<Self>,
}

#[cfg(feature = "rmcp-integration")]
impl CodivexRmcpServer {
    fn new(cwd: PathBuf) -> Self {
        Self {
            cwd,
            tool_router: Self::tool_router(),
        }
    }
}

#[cfg(feature = "rmcp-integration")]
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
struct SearchArgs {
    query: String,
    #[serde(default)]
    top_k: Option<usize>,
    #[serde(default)]
    repo_filter: Option<String>,
}

#[cfg(feature = "rmcp-integration")]
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
struct OpenArgs {
    path: String,
    line_start: usize,
    line_end: usize,
    #[serde(default)]
    repo_filter: Option<String>,
}

#[cfg(feature = "rmcp-integration")]
#[tool_router]
impl CodivexRmcpServer {
    #[tool(
        name = "searchCode",
        description = "Hybrid code search scoped to project"
    )]
    async fn search_code(
        &self,
        Parameters(args): Parameters<SearchArgs>,
    ) -> Result<String, McpError> {
        let scope = args
            .repo_filter
            .or_else(|| common::projects::read_selected_project(&self.cwd))
            .ok_or_else(|| McpError::invalid_params("project scope required".to_string(), None))?;
        let top_k = args.top_k.unwrap_or(5).max(1);
        let items = mcp_server::services::search::scoped_project_results(
            &self.cwd,
            &scope,
            &args.query,
            top_k,
        )
        .await
        .map_err(|e| McpError::internal_error(e.to_string(), None))?;
        let result = SearchCodeResult { items };
        serde_json::to_string(&result)
            .map_err(|e| McpError::internal_error(format!("serialize result failed: {e}"), None))
    }

    #[tool(name = "openLocation", description = "Resolve file + line range")]
    async fn open_location(
        &self,
        Parameters(args): Parameters<OpenArgs>,
    ) -> Result<String, McpError> {
        let base = args
            .repo_filter
            .or_else(|| common::projects::read_selected_project(&self.cwd))
            .unwrap_or_else(|| self.cwd.display().to_string());
        let requested = std::path::Path::new(&args.path);
        let resolved = if requested.is_absolute() {
            requested.to_path_buf()
        } else {
            std::path::Path::new(&base).join(requested)
        };
        let content = std::fs::read_to_string(&resolved)
            .map_err(|_| McpError::invalid_params("path not readable".to_string(), None))?;
        let lines = content.lines().count().max(1);
        if args.line_start < 1 || args.line_end < args.line_start || args.line_end > lines {
            return Err(McpError::invalid_params(
                format!(
                    "line range {}..{} out of bounds 1..={lines}",
                    args.line_start, args.line_end
                ),
                None,
            ));
        }
        let result = OpenLocationResult {
            path: resolved.display().to_string(),
            line_start: args.line_start,
            line_end: args.line_end,
        };
        serde_json::to_string(&result)
            .map_err(|e| McpError::internal_error(format!("serialize result failed: {e}"), None))
    }
}

#[cfg(feature = "rmcp-integration")]
#[tool_handler]
impl ServerHandler for CodivexRmcpServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo {
            capabilities: ServerCapabilities::builder().enable_tools().build(),
            instructions: Some(
                "Codivex RMCP stdio adapter exposing searchCode and openLocation tools".to_string(),
            ),
            ..Default::default()
        }
    }
}

#[cfg(feature = "rmcp-integration")]
#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_target(false)
        .json()
        .with_writer(std::io::stderr)
        .init();
    let cwd = std::env::current_dir()?;
    let service = CodivexRmcpServer::new(cwd).serve(stdio()).await?;
    service.waiting().await?;
    Ok(())
}

#[cfg(not(feature = "rmcp-integration"))]
fn main() {
    eprintln!("rmcp integration disabled; re-run with --features rmcp-integration");
}
