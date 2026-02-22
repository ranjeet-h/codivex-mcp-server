# Benchmark Targets

Derived from `idea.md` guidance and used as acceptance references.

## Latency Targets
- Qdrant ANN (1M-scale): target band ~3-10ms
- Tantivy lexical query: target band ~5ms class
- End-to-end query: target band ~20-50ms typical path

## Throughput Targets
- Throughput aspiration: up to ~1200 QPS (environment dependent)

## Resource Planning Targets
- Recommended runtime: 2-4 vCPUs baseline
- Vector memory planning:
  - 1536-d float vector: ~6KB/vector
  - 1M vectors: ~6GB pre-quantization
  - ~1.5GB class with quantization enabled

