SHELL := /bin/bash
.DEFAULT_GOAL := help

.PHONY: help setup fetch run run-mcp run-stdio run-rmcp-stdio check-rmcp run-ui run-all bench bench-matrix quality-harness load-test validate-slo smoke-install-cli verify-docker fmt fmt-check typecheck check clippy test audit deny outdated deps-check clean all ci

help:
	@echo "Available targets:"
	@echo "  make setup       - Verify toolchain and install rustfmt/clippy"
	@echo "  make fetch       - Fetch workspace dependencies"
	@echo "  make run         - Run MCP server (crates/mcp-server)"
	@echo "  make run-mcp     - Same as run"
	@echo "  make run-stdio   - Run MCP server in stdio transport mode"
	@echo "  make run-rmcp-stdio - Run feature-gated rmcp stdio adapter"
	@echo "  make check-rmcp  - Compile-check rmcp stdio adapter feature"
	@echo "  make run-ui      - Run Dioxus UI binary (crates/ui-dioxus)"
	@echo "  make run-all     - Run MCP server + Dioxus UI together"
	@echo "  make bench       - Run benchmark command suite"
	@echo "  make bench-matrix - Run benchmark suite across configured dataset matrix"
	@echo "  make quality-harness - Evaluate retrieval quality (MRR/Recall) dataset"
	@echo "  make load-test   - Run API/SSE load test runner"
	@echo "  make validate-slo - Validate benchmark/load reports against SLO thresholds"
	@echo "  make smoke-install-cli - Validate cargo install path for codivex-mcp"
	@echo "  make verify-docker - Verify docker runtime wiring (mounts, qdrant, model path)"
	@echo "  make fmt         - Format all Rust code"
	@echo "  make fmt-check   - Check formatting only"
	@echo "  make typecheck   - Type-check workspace (cargo check)"
	@echo "  make check       - Alias for typecheck"
	@echo "  make clippy      - Run clippy with warnings denied"
	@echo "  make test        - Run workspace tests"
	@echo "  make audit       - Run cargo-audit (security advisories)"
	@echo "  make deny        - Run cargo-deny checks"
	@echo "  make outdated    - Run cargo-outdated (informational)"
	@echo "  make deps-check  - Run audit + deny + outdated"
	@echo "  make all         - fmt-check + typecheck + clippy + test + deps-check"
	@echo "  make ci          - Same as all"
	@echo "  make clean       - Remove build artifacts"

setup:
	rustup show active-toolchain
	rustup component add rustfmt clippy
	@command -v cargo-audit >/dev/null 2>&1 || cargo install cargo-audit
	@command -v cargo-deny >/dev/null 2>&1 || cargo install cargo-deny
	@command -v cargo-outdated >/dev/null 2>&1 || cargo install cargo-outdated
	cargo --version
	rustc --version

fetch:
	cargo fetch --locked

run: run-mcp

run-mcp:
	cargo run -p mcp-server

run-stdio:
	cargo run -p mcp-server --bin mcp_stdio

run-rmcp-stdio:
	cargo run -p mcp-server --bin rmcp_stdio --features rmcp-integration

check-rmcp:
	cargo check -p mcp-server --bin rmcp_stdio --features rmcp-integration

run-ui:
	cargo run -p ui-dioxus

run-all:
	@set -euo pipefail; \
	trap 'kill 0' INT TERM EXIT; \
	echo "Starting MCP server and Dioxus UI..."; \
	cargo run -p mcp-server --bin mcp-server & \
	cargo run -p ui-dioxus & \
	wait

bench:
	cargo run -p mcp-server --bin benchmark_suite

bench-matrix:
	./scripts/run-benchmark-matrix.sh

quality-harness:
	cargo run -p mcp-server --bin evaluate_quality

load-test:
	cargo run -p mcp-server --bin load_test_runner

validate-slo:
	cargo run -p mcp-server --bin validate_slo

smoke-install-cli:
	./scripts/smoke-install-cli.sh

verify-docker:
	./scripts/verify-docker-runtime.sh

fmt:
	cargo fmt --all

fmt-check:
	cargo fmt --all --check

typecheck:
	cargo check --workspace --all-targets

check: typecheck

clippy:
	cargo clippy --workspace --all-targets -- -D warnings

test:
	cargo test --workspace --all-targets

audit:
	cargo audit

deny:
	cargo deny check

outdated:
	cargo outdated --workspace --root-deps-only || true

deps-check: audit deny outdated

all: fmt-check typecheck clippy test deps-check

ci: all

clean:
	cargo clean
