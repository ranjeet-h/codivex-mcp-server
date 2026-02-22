use axum::{
    Json,
    extract::State,
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
};
use common::{
    OpenLocationParams, OpenLocationResult, RpcErrorCode, RpcRequest, RpcResponse,
    SearchCodeParams, SearchCodeResult, schema_bundle,
};
use sha2::{Digest, Sha256};
use std::time::Instant;
use tracing::{info, warn};

use crate::{
    handlers::auth::is_authorized,
    handlers::mcp_protocol::{
        ToolCallParams, ToolCallResult, ToolContent, initialize_result, parse_open_arguments,
        parse_search_arguments, prompts_list_result, resources_list_result, tools_list_result,
    },
    json_rpc::json_from_response,
    services::search::{cache_key, cache_lookup, cache_store, scoped_project_results},
    state::AppState,
};

pub async fn mcp_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(req): Json<RpcRequest>,
) -> impl IntoResponse {
    if !is_authorized(&headers, &state) {
        return (StatusCode::UNAUTHORIZED, "unauthorized").into_response();
    }

    metrics::counter!("mcp_requests_total").increment(1);
    let project_scope = scoped_project_from_headers(&headers)
        .or_else(|| common::projects::read_selected_project(&state.cwd).filter(|p| !p.is_empty()))
        .map(|scope| resolve_project_scope(&state.cwd, &scope));

    match req.method.as_str() {
        "ping" => {
            json_from_response(RpcResponse::ok(req.id, serde_json::json!({}))).into_response()
        }
        "initialize" => handle_initialize(req).into_response(),
        "tools/list" => handle_tools_list(req).into_response(),
        "resources/list" => handle_resources_list(req).into_response(),
        "prompts/list" => handle_prompts_list(req).into_response(),
        "tools/call" => handle_tools_call(&state, req, project_scope.as_deref())
            .await
            .into_response(),
        "searchCode" => handle_search_code(&state, req, project_scope.as_deref())
            .await
            .into_response(),
        "openLocation" => {
            handle_open_location(&state, req, project_scope.as_deref()).into_response()
        }
        _ => {
            warn!("unknown method");
            json_from_response(RpcResponse::<serde_json::Value>::err(
                req.id,
                RpcErrorCode::MethodNotFound.as_i64(),
                "method not found",
            ))
            .into_response()
        }
    }
}

fn handle_initialize(req: RpcRequest) -> Json<serde_json::Value> {
    let params = if req.params.is_null() {
        None
    } else {
        match serde_json::from_value(req.params) {
            Ok(v) => Some(v),
            Err(err) => {
                return json_from_response(RpcResponse::<serde_json::Value>::err(
                    req.id,
                    RpcErrorCode::InvalidParams.as_i64(),
                    format!("invalid initialize params: {err}"),
                ));
            }
        }
    };
    json_from_response(RpcResponse::ok(req.id, initialize_result(params)))
}

fn handle_tools_list(req: RpcRequest) -> Json<serde_json::Value> {
    match tools_list_result() {
        Ok(result) => json_from_response(RpcResponse::ok(req.id, result)),
        Err(err) => json_from_response(RpcResponse::<serde_json::Value>::err(
            req.id,
            RpcErrorCode::Internal.as_i64(),
            format!("failed generating tools list: {err}"),
        )),
    }
}

fn handle_resources_list(req: RpcRequest) -> Json<serde_json::Value> {
    json_from_response(RpcResponse::ok(req.id, resources_list_result()))
}

fn handle_prompts_list(req: RpcRequest) -> Json<serde_json::Value> {
    json_from_response(RpcResponse::ok(req.id, prompts_list_result()))
}

async fn handle_tools_call(
    state: &AppState,
    req: RpcRequest,
    project_scope: Option<&str>,
) -> Json<serde_json::Value> {
    let params = match serde_json::from_value::<ToolCallParams>(req.params) {
        Ok(p) => p,
        Err(err) => {
            return json_from_response(RpcResponse::<serde_json::Value>::err(
                req.id,
                RpcErrorCode::InvalidParams.as_i64(),
                format!("invalid tools/call params: {err}"),
            ));
        }
    };

    match params.name.as_str() {
        "searchCode" | "search_code" => match parse_search_arguments(params.arguments) {
            Ok(search_params) => {
                let started = Instant::now();
                match execute_search(state, search_params, project_scope).await {
                    Ok(result) => {
                        state
                            .record_search_latency_ms(started.elapsed().as_millis())
                            .await;
                        let structured = serde_json::to_value(&result).ok();
                        let text = serde_json::to_string(&result)
                            .unwrap_or_else(|_| "{\"items\":[]}".to_string());
                        json_from_response(RpcResponse::ok(
                            req.id,
                            ToolCallResult {
                                content: vec![ToolContent {
                                    kind: "text".to_string(),
                                    text,
                                }],
                                structured_content: structured,
                                is_error: false,
                            },
                        ))
                    }
                    Err(err) => json_from_response(RpcResponse::ok(
                        req.id,
                        ToolCallResult {
                            content: vec![ToolContent {
                                kind: "text".to_string(),
                                text: err.message,
                            }],
                            structured_content: None,
                            is_error: true,
                        },
                    )),
                }
            }
            Err(err) => json_from_response(RpcResponse::<serde_json::Value>::err(
                req.id,
                RpcErrorCode::InvalidParams.as_i64(),
                err,
            )),
        },
        "openLocation" | "open_location" => match parse_open_arguments(params.arguments) {
            Ok(open_params) => match execute_open_location(state, open_params, project_scope) {
                Ok(result) => {
                    let structured = serde_json::to_value(&result).ok();
                    let text = serde_json::to_string(&result).unwrap_or_else(|_| {
                        "{\"path\":\"\",\"line_start\":0,\"line_end\":0}".to_string()
                    });
                    json_from_response(RpcResponse::ok(
                        req.id,
                        ToolCallResult {
                            content: vec![ToolContent {
                                kind: "text".to_string(),
                                text,
                            }],
                            structured_content: structured,
                            is_error: false,
                        },
                    ))
                }
                Err(err) => json_from_response(RpcResponse::ok(
                    req.id,
                    ToolCallResult {
                        content: vec![ToolContent {
                            kind: "text".to_string(),
                            text: err.message,
                        }],
                        structured_content: None,
                        is_error: true,
                    },
                )),
            },
            Err(err) => json_from_response(RpcResponse::<serde_json::Value>::err(
                req.id,
                RpcErrorCode::InvalidParams.as_i64(),
                err,
            )),
        },
        _ => json_from_response(RpcResponse::<serde_json::Value>::err(
            req.id,
            RpcErrorCode::InvalidParams.as_i64(),
            format!("unsupported tool: {}", params.name),
        )),
    }
}

async fn handle_search_code(
    state: &AppState,
    req: RpcRequest,
    project_scope: Option<&str>,
) -> Json<serde_json::Value> {
    let started = Instant::now();
    if let Err(err) = validate_search_params(&req.params) {
        return json_from_response(RpcResponse::<SearchCodeResult>::err(
            req.id,
            RpcErrorCode::InvalidParams.as_i64(),
            err,
        ));
    }
    match serde_json::from_value::<SearchCodeParams>(req.params) {
        Ok(params) => {
            let query_hash = hash_query(&params.query);
            info!(query_hash = query_hash, top_k = params.top_k, "searchCode");
            match execute_search(state, params, project_scope).await {
                Ok(result) => {
                    state
                        .record_search_latency_ms(started.elapsed().as_millis())
                        .await;
                    json_from_response(RpcResponse::ok(req.id, result))
                }
                Err(err) => json_from_response(RpcResponse::<SearchCodeResult>::err(
                    req.id,
                    err.code,
                    err.message,
                )),
            }
        }
        Err(err) => json_from_response(RpcResponse::<SearchCodeResult>::err(
            req.id,
            RpcErrorCode::InvalidParams.as_i64(),
            format!("invalid params: {err}"),
        )),
    }
}

fn handle_open_location(
    state: &AppState,
    req: RpcRequest,
    project_scope: Option<&str>,
) -> Json<serde_json::Value> {
    if let Err(err) = validate_open_location_params(&req.params) {
        return json_from_response(RpcResponse::<OpenLocationResult>::err(
            req.id,
            RpcErrorCode::InvalidParams.as_i64(),
            err,
        ));
    }
    match serde_json::from_value::<OpenLocationParams>(req.params) {
        Ok(params) => match execute_open_location(state, params, project_scope) {
            Ok(result) => json_from_response(RpcResponse::ok(req.id, result)),
            Err(err) => json_from_response(RpcResponse::<OpenLocationResult>::err(
                req.id,
                err.code,
                err.message,
            )),
        },
        Err(err) => json_from_response(RpcResponse::<OpenLocationResult>::err(
            req.id,
            RpcErrorCode::InvalidParams.as_i64(),
            format!("invalid params: {err}"),
        )),
    }
}

#[derive(Debug, Clone)]
struct MethodError {
    code: i64,
    message: String,
}

async fn execute_search(
    state: &AppState,
    params: SearchCodeParams,
    project_scope: Option<&str>,
) -> Result<SearchCodeResult, MethodError> {
    if params.query.trim().is_empty() {
        return Err(MethodError {
            code: RpcErrorCode::InvalidParams.as_i64(),
            message: "query cannot be empty".to_string(),
        });
    }
    if params.query == "__index_unavailable__" {
        return Err(MethodError {
            code: RpcErrorCode::IndexUnavailable.as_i64(),
            message: "index unavailable".to_string(),
        });
    }
    if params.query == "__timeout__" {
        return Err(MethodError {
            code: RpcErrorCode::Timeout.as_i64(),
            message: "query timed out".to_string(),
        });
    }

    let scoped_from_request = params
        .repo_filter
        .as_deref()
        .filter(|v| !v.trim().is_empty())
        .map(|scope| resolve_project_scope(&state.cwd, scope));
    let effective_scope = scoped_from_request.or_else(|| project_scope.map(str::to_string));
    let Some(scope) = effective_scope else {
        return Err(MethodError {
            code: RpcErrorCode::InvalidParams.as_i64(),
            message: "project scope required: set repoFilter or x-codivex-project header or select project in admin UI".to_string(),
        });
    };

    let key = cache_key(&scope, &params.query, params.top_k);
    if let Some(cached) = cache_lookup(&state.query_cache, &key).await {
        metrics::counter!("mcp_query_cache_hits_total").increment(1);
        return Ok(cached);
    }

    metrics::counter!("mcp_query_cache_misses_total").increment(1);
    let items = scoped_project_results(&state.cwd, &scope, &params.query, params.top_k)
        .await
        .unwrap_or_default();
    let result = SearchCodeResult { items };
    if result.items.is_empty() {
        return Err(MethodError {
            code: RpcErrorCode::IndexUnavailable.as_i64(),
            message: "project has no indexed data or no matches".to_string(),
        });
    }
    cache_store(
        &state.query_cache,
        key,
        SearchCodeResult {
            items: result.items.clone(),
        },
    )
    .await;
    Ok(result)
}

fn execute_open_location(
    state: &AppState,
    params: OpenLocationParams,
    project_scope: Option<&str>,
) -> Result<OpenLocationResult, MethodError> {
    let resolved_path = resolve_source_path(&state.cwd, project_scope, &params.path);
    let content = std::fs::read_to_string(&resolved_path).map_err(|_| MethodError {
        code: RpcErrorCode::InvalidParams.as_i64(),
        message: "path does not exist or is not readable".to_string(),
    })?;

    let line_count = content.lines().count().max(1);
    let valid_range = params.line_start >= 1
        && params.line_end >= params.line_start
        && params.line_end <= line_count;
    if !valid_range {
        return Err(MethodError {
            code: RpcErrorCode::InvalidParams.as_i64(),
            message: format!(
                "requested line range {}..{} outside file bounds (1..={line_count})",
                params.line_start, params.line_end
            ),
        });
    }

    Ok(OpenLocationResult {
        path: resolved_path.display().to_string(),
        line_start: params.line_start,
        line_end: params.line_end,
    })
}

fn hash_query(query: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(query.as_bytes());
    format!("{:x}", hasher.finalize())
}

fn validate_search_params(params: &serde_json::Value) -> Result<(), String> {
    let bundle = schema_bundle();
    let schema = serde_json::to_value(bundle.search_code_params)
        .map_err(|e| format!("schema serialization error: {e}"))?;
    jsonschema::validate(&schema, params).map_err(|e| format!("schema validation failed: {e}"))
}

fn validate_open_location_params(params: &serde_json::Value) -> Result<(), String> {
    let bundle = schema_bundle();
    let schema = serde_json::to_value(bundle.open_location_params)
        .map_err(|e| format!("schema serialization error: {e}"))?;
    jsonschema::validate(&schema, params).map_err(|e| format!("schema validation failed: {e}"))
}

fn scoped_project_from_headers(headers: &HeaderMap) -> Option<String> {
    headers
        .get("x-codivex-project")
        .or_else(|| headers.get("x-project-path"))
        .and_then(|v| v.to_str().ok())
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .map(str::to_string)
}

fn resolve_project_scope(cwd: &std::path::Path, scope: &str) -> String {
    let requested = std::path::Path::new(scope);
    if requested.is_absolute() {
        return requested.display().to_string();
    }
    let from_cwd = cwd.join(scope);
    if from_cwd.exists() {
        return from_cwd.display().to_string();
    }
    for root in configured_project_roots(cwd) {
        let candidate = root.join(scope);
        if candidate.exists() {
            return candidate.display().to_string();
        }
    }
    from_cwd.display().to_string()
}

fn configured_project_roots(cwd: &std::path::Path) -> Vec<std::path::PathBuf> {
    let mut roots = vec![cwd.to_path_buf()];
    if let Ok(raw) = std::env::var("CODIVEX_PROJECT_ROOTS") {
        let sep = if cfg!(windows) { ';' } else { ':' };
        roots.extend(
            raw.split(sep)
                .map(str::trim)
                .filter(|p| !p.is_empty())
                .map(std::path::PathBuf::from),
        );
    }
    roots
}

fn resolve_source_path(
    cwd: &std::path::Path,
    project_scope: Option<&str>,
    path: &str,
) -> std::path::PathBuf {
    let requested = std::path::Path::new(path);
    if requested.is_absolute() {
        return requested.to_path_buf();
    }
    if let Some(scope) = project_scope {
        return std::path::Path::new(scope).join(path);
    }
    cwd.join(path)
}
