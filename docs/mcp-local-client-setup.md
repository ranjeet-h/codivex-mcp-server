# Local MCP Client Setup (No API Keys)

This project is local-first. Use localhost or stdio transport only. No API key is required by default.

## Prerequisites
- Start server (HTTP):
  - `make run-mcp`
- Or start stdio transport:
  - `make run-stdio`
- Ensure at least one project is indexed:
  - `cargo run -p codivex-mcp -- add-repo /absolute/path/to/project`
  - `cargo run -p codivex-mcp -- index-now`

## Cursor (recommended)
Global config file:
- `~/.cursor/mcp.json`

Project config file:
- `.cursor/mcp.json`

HTTP local server:
```json
{
  "mcpServers": {
    "codivex-local": {
      "type": "http",
      "url": "http://127.0.0.1:38080/mcp"
    }
  }
}
```

Stdio local server:
```json
{
  "mcpServers": {
    "codivex-local": {
      "type": "stdio",
      "command": "cargo",
      "args": ["run", "-p", "mcp-server", "--bin", "mcp_stdio"]
    }
  }
}
```

## Claude Code
Stdio local server:
```bash
claude mcp add --scope user codivex-local -- cargo run -p mcp-server --bin mcp_stdio
```

HTTP local server:
```bash
claude mcp add --scope user --transport http codivex-local http://127.0.0.1:38080/mcp
```

## Zed
Add a local context server in Zed settings (MCP/context server config):

Stdio:
```json
{
  "context_servers": {
    "codivex-local": {
      "command": "cargo",
      "args": ["run", "-p", "mcp-server", "--bin", "mcp_stdio"]
    }
  }
}
```

HTTP:
```json
{
  "context_servers": {
    "codivex-local": {
      "url": "http://127.0.0.1:38080/mcp"
    }
  }
}
```

## How the LLM Knows Which Tool to Use
LLM clients discover tools through MCP `initialize` and `tools/list`. This server publishes:
- `searchCode`: project-scoped retrieval of relevant chunks.
- `openLocation`: file + line range resolution for precise follow-up.

To improve reliability:
- Keep tool descriptions specific and action-oriented (implemented in this server).
- Keep JSON schemas strict (required fields, types, bounds).
- In prompts, include project scope when needed:
  - `repoFilter` argument
  - or header `x-codivex-project`

Example tool call flow:
1. `searchCode` with `{ "query": "iso to date", "top_k": 5, "repoFilter": "/abs/project" }`
2. `openLocation` for selected hit path + lines.

## Optional Rule in Client
If your client supports rules/instructions, add:
- "For code navigation and retrieval, always call `searchCode` first, then `openLocation` for exact lines before making changes."

## Verify Connection Quickly
```bash
curl -sS http://127.0.0.1:38080/mcp \
  -H 'content-type: application/json' \
  -d '{"jsonrpc":"2.0","id":1,"method":"tools/list","params":{}}'
```

## References
- Cursor MCP docs: https://docs.cursor.com/advanced/model-context-protocol
- Cursor CLI MCP docs: https://docs.cursor.com/cli/mcp
- Claude Code MCP docs: https://docs.anthropic.com/en/docs/claude-code/mcp
- Zed MCP docs: https://zed.dev/docs/ai/mcp
- MCP tools spec (current): https://modelcontextprotocol.io/specification/2025-06-18/server/tools
