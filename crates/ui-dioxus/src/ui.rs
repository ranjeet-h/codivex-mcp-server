use dioxus::prelude::*;

#[component]
pub fn AdminPage(mcp_endpoint: String, ui_endpoint: String) -> Element {
    rsx! {
        div {
            id: "app",
            style: "font-family: ui-sans-serif, system-ui; padding: 20px; max-width: 980px; margin: 0 auto; display: grid; gap: 14px;",
            h1 { "Codivex Admin" }

            section {
                style: panel_style(),
                h2 { "Endpoints" }
                p { "MCP endpoint: {mcp_endpoint}" }
                p { "UI endpoint: {ui_endpoint}" }
            }

            section {
                style: panel_style(),
                h2 { "Project Indexing" }
                input { id: "folder-picker", r#type: "file", style: "display:none;", multiple: true }
                div { style: "display:flex; gap:8px; flex-wrap:wrap;",
                    input {
                        id: "project-path-input",
                        r#type: "text",
                        placeholder: "Absolute path or project name (optional roots: CODIVEX_PROJECT_ROOTS)",
                        style: input_style()
                    }
                    button { id: "btn-select-repo", style: button_style(), "Select Folder" }
                    button { id: "btn-apply-path", style: button_style(), "Use Path" }
                }
                div { style: "display:flex; gap:8px; flex-wrap:wrap; margin-top:8px;",
                    button { id: "btn-start-index", style: button_style(), "Index Selected" }
                    button { id: "btn-reindex", style: button_style(), "Re-index" }
                    button { id: "btn-clear-index", style: danger_button_style(), "Clear Index" }
                }
                p { id: "selected-project-status", "Selected project: none" }
                p { id: "index-action-status", "Index status: idle" }
            }

            section {
                style: panel_style(),
                h2 { "Search Playground" }
                div { style: "display:flex; gap:8px; flex-wrap:wrap;",
                    input { id: "search-query", r#type: "text", value: "iso to date", style: input_style() }
                    input { id: "search-topk", r#type: "number", value: "5", min: "1", max: "20", style: "width:80px; padding:8px;" }
                    button { id: "btn-search", style: button_style(), "Run searchCode" }
                }
                p { id: "search-status", "Status: idle" }
                p { "SSE stream preview:" }
                pre {
                    id: "sse-stream-output",
                    style: "background:#0d1117;color:#e6edf3;padding:12px;border-radius:8px;white-space:pre-wrap;max-height:180px;overflow:auto;",
                    ""
                }
                table { style: "width:100%; border-collapse:collapse;",
                    thead {
                        tr {
                            th { style: th_style(), "File" }
                            th { style: th_style(), "Function" }
                            th { style: th_style(), "Line Range" }
                        }
                    }
                    tbody { id: "result-tbody" }
                }
            }

            section {
                style: panel_style(),
                h2 { "Live Health" }
                div { style: "display:flex; gap:24px; flex-wrap:wrap;",
                    p { "Queue depth: ", span { id: "health-queue-depth", "0" } }
                    p { "Chunks indexed: ", span { id: "health-chunks-indexed", "0" } }
                    p { "Index size: ", span { id: "health-index-size", "0 B" } }
                    p { "Latency p50/p95: ", span { id: "health-latency", "0ms / 0ms" } }
                }
                p { "Indexed projects:" }
                table { style: "width:100%; border-collapse:collapse;",
                    thead {
                        tr {
                            th { style: th_style(), "Project" }
                            th { style: th_style(), "Files" }
                            th { style: th_style(), "Chunks" }
                            th { style: th_style(), "Indexed At (unix)" }
                        }
                    }
                    tbody { id: "project-catalog-body" }
                }
                p { "Watcher runtime:" }
                pre {
                    id: "runtime-watchers",
                    style: "background:#0d1117;color:#e6edf3;padding:12px;border-radius:8px;white-space:pre-wrap;max-height:200px;overflow:auto;",
                    "[]"
                }
            }
        }
    }
}

const fn panel_style() -> &'static str {
    "border:1px solid #ddd;border-radius:10px;padding:14px;background:#fafafa;"
}
const fn input_style() -> &'static str {
    "min-width:420px;padding:8px;border:1px solid #bbb;border-radius:6px;"
}
const fn button_style() -> &'static str {
    "padding:8px 12px;border:1px solid #444;background:#111;color:#fff;border-radius:6px;"
}
const fn danger_button_style() -> &'static str {
    "padding:8px 12px;border:1px solid #7f1d1d;background:#b91c1c;color:#fff;border-radius:6px;"
}
const fn th_style() -> &'static str {
    "text-align:left;border-bottom:1px solid #ddd;padding:8px;"
}
