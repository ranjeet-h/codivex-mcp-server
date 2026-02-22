# CLI Guide (`codivex-mcp`)

## Install

From source (current repo):
```bash
cargo install --path crates/mcp-code-indexer --locked --force
```

Verify:
```bash
codivex-mcp --help
```

## Run

Add/select a repository:
```bash
codivex-mcp add-repo /absolute/path/to/project
```

Index selected repo:
```bash
codivex-mcp index-now
```

Index a specific repo directly:
```bash
codivex-mcp index-now /absolute/path/to/project
```

Show status:
```bash
codivex-mcp status
```

List repos:
```bash
codivex-mcp list-repos
```

Remove repo:
```bash
codivex-mcp remove-repo /absolute/path/to/project
```

## Update

Reinstall latest local source:
```bash
cargo install --path crates/mcp-code-indexer --locked --force
```

If using a custom install root:
```bash
cargo install --path crates/mcp-code-indexer --locked --force --root /custom/install/root
```

## Notes

- The package name is `codivex-mcp`.
- CLI state is stored in `.codivex/` in your current working directory.
