use axum::{
    body::{Body, to_bytes},
    http::{Request, StatusCode},
};
use common::projects::{IndexedChunk, IndexedProject};
use futures::{SinkExt, StreamExt};
use mcp_server::{app, state::AppState};
use serde_json::json;
use tokio_tungstenite::{connect_async, tungstenite::Message};
use tower::ServiceExt;

fn setup_indexed_project_state() -> AppState {
    let mut state = AppState::for_tests();
    let tmp = unique_tmp_dir("codivex-mcp-test");
    let _ = std::fs::create_dir_all(&tmp);
    state.cwd = tmp.clone();

    let project = tmp.join("repo-alpha");
    let project_str = project.display().to_string();
    let _ = std::fs::create_dir_all(project.join("src"));
    let _ = std::fs::write(
        project.join("src/lib.rs"),
        "fn a() {}\nfn b() {}\nfn c() {}\nfn d() {}\n",
    );
    let _ = common::projects::write_selected_project(&tmp, &project_str);
    let indexed = IndexedProject {
        project_path: project_str,
        files_scanned: 1,
        chunks_extracted: 1,
        indexed_at_unix: 1,
        chunks: vec![IndexedChunk {
            file: "src/date.rs".to_string(),
            symbol: Some("iso_to_date".to_string()),
            start_line: 40,
            end_line: 58,
            content: "fn iso_to_date(input: &str) -> String { input.to_string() }".to_string(),
        }],
    };
    let _ = common::projects::save_project_index(&tmp, &indexed);
    state
}

fn setup_indexed_project_state_with_token() -> AppState {
    let mut state = setup_indexed_project_state();
    state.api_token = Some("secret-token".to_string());
    state
}

fn setup_dual_project_state() -> AppState {
    let mut state = AppState::for_tests();
    let tmp = unique_tmp_dir("codivex-mcp-dual-test");
    let _ = std::fs::create_dir_all(&tmp);
    state.cwd = tmp.clone();

    let repo_alpha = tmp.join("repo-alpha");
    let repo_beta = tmp.join("repo-beta");
    let _ = std::fs::create_dir_all(repo_alpha.join("src"));
    let _ = std::fs::create_dir_all(repo_beta.join("src"));
    let _ = std::fs::write(
        repo_alpha.join("src/date.rs"),
        "fn iso_to_date(input: &str) -> String { input.to_string() }",
    );
    let _ = std::fs::write(
        repo_beta.join("src/repo.rs"),
        "fn save_user(name: &str) -> bool { !name.is_empty() }",
    );

    let alpha_path = repo_alpha.display().to_string();
    let beta_path = repo_beta.display().to_string();
    let _ = common::projects::write_selected_project(&tmp, &alpha_path);
    let _ = common::projects::save_project_index(
        &tmp,
        &IndexedProject {
            project_path: alpha_path.clone(),
            files_scanned: 1,
            chunks_extracted: 1,
            indexed_at_unix: 1,
            chunks: vec![IndexedChunk {
                file: "src/date.rs".to_string(),
                symbol: Some("iso_to_date".to_string()),
                start_line: 1,
                end_line: 1,
                content: "fn iso_to_date(input: &str) -> String { input.to_string() }".to_string(),
            }],
        },
    );
    let _ = common::projects::save_project_index(
        &tmp,
        &IndexedProject {
            project_path: beta_path,
            files_scanned: 1,
            chunks_extracted: 1,
            indexed_at_unix: 1,
            chunks: vec![IndexedChunk {
                file: "src/repo.rs".to_string(),
                symbol: Some("save_user".to_string()),
                start_line: 1,
                end_line: 1,
                content: "fn save_user(name: &str) -> bool { !name.is_empty() }".to_string(),
            }],
        },
    );
    state
}

fn unique_tmp_dir(prefix: &str) -> std::path::PathBuf {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    std::env::temp_dir().join(format!("{prefix}-{}-{nanos}", std::process::id()))
}

#[tokio::test]
async fn search_code_rpc_returns_result_payload() {
    let app = app::router(setup_indexed_project_state());
    let req = Request::builder()
        .method("POST")
        .uri("/mcp")
        .header("content-type", "application/json")
        .body(Body::from(
            json!({
                "jsonrpc": "2.0",
                "id": 1,
                "method": "searchCode",
                "params": { "query": "iso_to_date", "top_k": 1 }
            })
            .to_string(),
        ))
        .expect("request");

    let res = app.oneshot(req).await.expect("response");
    assert_eq!(res.status(), StatusCode::OK);
    let body = to_bytes(res.into_body(), usize::MAX).await.expect("bytes");
    let json: serde_json::Value = serde_json::from_slice(&body).expect("json");
    assert_eq!(json["jsonrpc"], "2.0");
    assert_eq!(json["id"], 1);
    let items = json["result"]["items"]
        .as_array()
        .expect("search items array");
    assert!(!items.is_empty());
    assert_eq!(items[0]["file"], "src/date.rs");
    assert_eq!(items[0]["function"], "iso_to_date");
}

#[tokio::test]
async fn search_code_honors_repo_filter_for_project_scoped_results() {
    let app = app::router(setup_dual_project_state());
    let req = Request::builder()
        .method("POST")
        .uri("/mcp")
        .header("content-type", "application/json")
        .body(Body::from(
            json!({
                "jsonrpc": "2.0",
                "id": 42,
                "method": "searchCode",
                "params": {
                    "query": "save_user",
                    "top_k": 1,
                    "repoFilter": "repo-beta"
                }
            })
            .to_string(),
        ))
        .expect("request");

    let res = app.oneshot(req).await.expect("response");
    assert_eq!(res.status(), StatusCode::OK);
    let body = to_bytes(res.into_body(), usize::MAX).await.expect("bytes");
    let json: serde_json::Value = serde_json::from_slice(&body).expect("json");
    let items = json["result"]["items"]
        .as_array()
        .expect("search items array");
    assert!(!items.is_empty());
    assert_eq!(items[0]["file"], "src/repo.rs");
    assert_eq!(items[0]["function"], "save_user");
}

#[tokio::test]
async fn open_location_rpc_returns_path_and_lines() {
    let app = app::router(setup_indexed_project_state());
    let req = Request::builder()
        .method("POST")
        .uri("/mcp")
        .header("content-type", "application/json")
        .body(Body::from(
            json!({
                "jsonrpc": "2.0",
                "id": "abc",
                "method": "openLocation",
                "params": { "path": "src/lib.rs", "line_start": 2, "line_end": 4 }
            })
            .to_string(),
        ))
        .expect("request");

    let res = app.oneshot(req).await.expect("response");
    assert_eq!(res.status(), StatusCode::OK);
    let body = to_bytes(res.into_body(), usize::MAX).await.expect("bytes");
    let json: serde_json::Value = serde_json::from_slice(&body).expect("json");
    assert!(
        json["result"]["path"]
            .as_str()
            .is_some_and(|p| p.ends_with("src/lib.rs"))
    );
    assert_eq!(json["result"]["line_start"], 2);
    assert_eq!(json["result"]["line_end"], 4);
}

#[tokio::test]
async fn sse_stream_contains_result_then_done_event() {
    let app = app::router(setup_indexed_project_state());
    let req = Request::builder()
        .method("GET")
        .uri("/mcp/sse?query=iso_to_date&top_k=2")
        .body(Body::empty())
        .expect("request");

    let res = app.oneshot(req).await.expect("response");
    assert_eq!(res.status(), StatusCode::OK);
    let body = to_bytes(res.into_body(), usize::MAX).await.expect("bytes");
    let text = String::from_utf8(body.to_vec()).expect("utf8");
    assert!(text.contains("event: result"));
    assert!(text.contains("event: done"));
}

#[tokio::test]
async fn rpc_requires_token_when_configured() {
    let app = app::router(setup_indexed_project_state_with_token());
    let req = Request::builder()
        .method("POST")
        .uri("/mcp")
        .header("content-type", "application/json")
        .body(Body::from(
            json!({
                "jsonrpc": "2.0",
                "id": 1,
                "method": "searchCode",
                "params": { "query": "iso_to_date", "top_k": 1 }
            })
            .to_string(),
        ))
        .expect("request");

    let res = app.oneshot(req).await.expect("response");
    assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn sse_emits_unauthorized_error_event_when_token_missing() {
    let app = app::router(setup_indexed_project_state_with_token());
    let req = Request::builder()
        .method("GET")
        .uri("/mcp/sse?query=iso_to_date&top_k=1")
        .body(Body::empty())
        .expect("request");

    let res = app.oneshot(req).await.expect("response");
    assert_eq!(res.status(), StatusCode::OK);
    let body = to_bytes(res.into_body(), usize::MAX).await.expect("bytes");
    let text = String::from_utf8(body.to_vec()).expect("utf8");
    assert!(text.contains("event: error"));
    assert!(text.contains("\"status\":401"));
}

#[tokio::test]
async fn initialize_returns_tool_capabilities() {
    let app = app::router(setup_indexed_project_state());
    let req = Request::builder()
        .method("POST")
        .uri("/mcp")
        .header("content-type", "application/json")
        .body(Body::from(
            json!({
                "jsonrpc": "2.0",
                "id": 7,
                "method": "initialize",
                "params": { "protocolVersion": "2025-06-18" }
            })
            .to_string(),
        ))
        .expect("request");

    let res = app.oneshot(req).await.expect("response");
    assert_eq!(res.status(), StatusCode::OK);
    let body = to_bytes(res.into_body(), usize::MAX).await.expect("bytes");
    let json: serde_json::Value = serde_json::from_slice(&body).expect("json");
    assert_eq!(json["result"]["protocolVersion"], "2025-06-18");
    assert!(json["result"]["capabilities"]["tools"].is_object());
}

#[tokio::test]
async fn initialize_accepts_missing_protocol_version() {
    let app = app::router(setup_indexed_project_state());
    let req = Request::builder()
        .method("POST")
        .uri("/mcp")
        .header("content-type", "application/json")
        .body(Body::from(
            json!({
                "jsonrpc": "2.0",
                "id": 71,
                "method": "initialize",
                "params": {}
            })
            .to_string(),
        ))
        .expect("request");

    let res = app.oneshot(req).await.expect("response");
    assert_eq!(res.status(), StatusCode::OK);
    let body = to_bytes(res.into_body(), usize::MAX).await.expect("bytes");
    let json: serde_json::Value = serde_json::from_slice(&body).expect("json");
    assert_eq!(json["result"]["protocolVersion"], "2025-06-18");
}

#[tokio::test]
async fn tools_list_returns_search_and_open_location() {
    let app = app::router(setup_indexed_project_state());
    let req = Request::builder()
        .method("POST")
        .uri("/mcp")
        .header("content-type", "application/json")
        .body(Body::from(
            json!({
                "jsonrpc": "2.0",
                "id": 8,
                "method": "tools/list",
                "params": {}
            })
            .to_string(),
        ))
        .expect("request");

    let res = app.oneshot(req).await.expect("response");
    assert_eq!(res.status(), StatusCode::OK);
    let body = to_bytes(res.into_body(), usize::MAX).await.expect("bytes");
    let json: serde_json::Value = serde_json::from_slice(&body).expect("json");
    let tools = json["result"]["tools"].as_array().expect("tools array");
    assert!(tools.iter().any(|tool| tool["name"] == "searchCode"));
    assert!(tools.iter().any(|tool| tool["name"] == "openLocation"));
}

#[tokio::test]
async fn tools_call_can_execute_search_code_with_camel_case_args() {
    let app = app::router(setup_indexed_project_state());
    let req = Request::builder()
        .method("POST")
        .uri("/mcp")
        .header("content-type", "application/json")
        .body(Body::from(
            json!({
                "jsonrpc": "2.0",
                "id": 9,
                "method": "tools/call",
                "params": {
                    "name": "searchCode",
                    "arguments": { "query": "iso_to_date", "topK": 1 }
                }
            })
            .to_string(),
        ))
        .expect("request");

    let res = app.oneshot(req).await.expect("response");
    assert_eq!(res.status(), StatusCode::OK);
    let body = to_bytes(res.into_body(), usize::MAX).await.expect("bytes");
    let json: serde_json::Value = serde_json::from_slice(&body).expect("json");
    assert_eq!(json["result"]["isError"], false);
    assert_eq!(
        json["result"]["structuredContent"]["items"][0]["function"],
        "iso_to_date"
    );
}

#[tokio::test]
async fn tools_call_can_execute_open_location_with_camel_case_lines() {
    let app = app::router(setup_indexed_project_state());
    let req = Request::builder()
        .method("POST")
        .uri("/mcp")
        .header("content-type", "application/json")
        .body(Body::from(
            json!({
                "jsonrpc": "2.0",
                "id": 10,
                "method": "tools/call",
                "params": {
                    "name": "openLocation",
                    "arguments": { "path": "src/lib.rs", "lineStart": 2, "lineEnd": 3 }
                }
            })
            .to_string(),
        ))
        .expect("request");

    let res = app.oneshot(req).await.expect("response");
    assert_eq!(res.status(), StatusCode::OK);
    let body = to_bytes(res.into_body(), usize::MAX).await.expect("bytes");
    let json: serde_json::Value = serde_json::from_slice(&body).expect("json");
    assert_eq!(json["result"]["isError"], false);
    assert_eq!(json["result"]["structuredContent"]["line_start"], 2);
    assert_eq!(json["result"]["structuredContent"]["line_end"], 3);
}

#[tokio::test]
async fn mcp_ping_and_empty_catalog_methods_are_supported() {
    let app = app::router(setup_indexed_project_state());
    for (id, method, key) in [
        (11, "ping", ""),
        (12, "resources/list", "resources"),
        (13, "prompts/list", "prompts"),
    ] {
        let req = Request::builder()
            .method("POST")
            .uri("/mcp")
            .header("content-type", "application/json")
            .body(Body::from(
                json!({
                    "jsonrpc": "2.0",
                    "id": id,
                    "method": method,
                    "params": {}
                })
                .to_string(),
            ))
            .expect("request");
        let res = app.clone().oneshot(req).await.expect("response");
        assert_eq!(res.status(), StatusCode::OK);
        let body = to_bytes(res.into_body(), usize::MAX).await.expect("bytes");
        let json: serde_json::Value = serde_json::from_slice(&body).expect("json");
        if !key.is_empty() {
            assert!(json["result"][key].is_array());
        } else {
            assert!(json["result"].is_object());
        }
    }
}

#[tokio::test]
async fn tools_call_unknown_tool_returns_validation_error() {
    let app = app::router(setup_indexed_project_state());
    let req = Request::builder()
        .method("POST")
        .uri("/mcp")
        .header("content-type", "application/json")
        .body(Body::from(
            json!({
                "jsonrpc":"2.0",
                "id": 99,
                "method":"tools/call",
                "params":{"name":"unknownTool","arguments":{}}
            })
            .to_string(),
        ))
        .expect("request");

    let res = app.oneshot(req).await.expect("response");
    assert_eq!(res.status(), StatusCode::OK);
    let body = to_bytes(res.into_body(), usize::MAX).await.expect("bytes");
    let json: serde_json::Value = serde_json::from_slice(&body).expect("json");
    assert_eq!(json["error"]["code"], -32602);
}

#[tokio::test]
async fn websocket_fallback_supports_json_rpc_calls() {
    let state = setup_indexed_project_state();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind");
    let addr = listener.local_addr().expect("local addr");
    let server = tokio::spawn(async move {
        axum::serve(listener, app::router(state))
            .await
            .expect("serve");
    });

    let ws_url = format!("ws://{addr}/mcp/ws");
    let (mut socket, _) = connect_async(ws_url).await.expect("connect");
    socket
        .send(Message::Text(
            json!({
                "jsonrpc":"2.0",
                "id": 77,
                "method": "searchCode",
                "params": { "query": "iso_to_date", "top_k": 1 }
            })
            .to_string()
            .into(),
        ))
        .await
        .expect("send");

    let msg = socket.next().await.expect("message").expect("ws frame");
    let Message::Text(payload) = msg else {
        panic!("expected text response");
    };
    let response: serde_json::Value = serde_json::from_str(&payload).expect("json");
    assert_eq!(response["id"], 77);
    assert_eq!(response["result"]["items"][0]["function"], "iso_to_date");

    server.abort();
}
