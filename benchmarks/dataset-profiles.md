# Benchmark Dataset Profiles

## Small (`small-50k`)
- Target size: ~50k LOC
- Composition: 5-10 repos/modules, mixed Rust/TS/Python
- Use case: local development smoke and regression checks

## Medium (`medium-500k`)
- Target size: ~500k LOC
- Composition: 20-40 modules, mixed app + library code
- Use case: primary acceptance benchmark profile

## Large (`large-1m`)
- Target size: 1M+ LOC
- Composition: monorepo-style tree with generated + handwritten code filtered via ignore rules
- Use case: stress test for indexing throughput and search latency tails

## Reproducibility Rules
- Snapshot commit SHAs for all source repos in `benchmarks/datasets.lock`.
- Include ignore patterns and language mix in each profile metadata file.
- Preserve same machine settings for comparative runs (CPU governor, power mode, no background build jobs).

