# Performance and Quality Testing Guide

## Quick Commands
- Benchmark suite:
  - `make bench`
- Benchmark matrix:
  - `make bench-matrix`
- Load test:
  - `make load-test`
- SLO validation:
  - `make validate-slo`
- Quality harness (MRR/Recall):
  - `make quality-harness`

## Using Real Local Projects Safely
For local/private datasets, pass paths via env vars but keep reports path-redacted by default.

- Single benchmark:
  - `BENCHMARK_DATASET_PROFILE=local BENCHMARK_DATASET_PATH=/absolute/project/path make bench`
- Matrix:
  - `BENCHMARK_MATRIX='small:/path/a:200;medium:/path/b:600' make bench-matrix`

Redaction defaults:
- `BENCHMARK_REDACT_PATH=true` (default)
- `QUALITY_REDACT_PATH=true` (default)

## Quality Dataset
Default dataset file:
- `benchmarks/quality-dataset-v1.json`

Override project path without editing dataset:
- `QUALITY_PROJECT_PATH=/absolute/project/path make quality-harness`
- Note: the query set in `benchmarks/quality-dataset-v1.json` is project-specific. If you override `QUALITY_PROJECT_PATH`, update queries/expected files to avoid false 0.0 scores.

## SLO Threshold Overrides
- `SLO_MAX_HYBRID_MS` (default `45`)
- `SLO_MAX_EMBEDDING_MS` (default `20`)
- `SLO_MIN_THROUGHPUT_QPS` (default `100`)
- `SLO_MAX_API_P50_MS` (default `50`)
- `SLO_MAX_API_P95_MS` (default `200`)
