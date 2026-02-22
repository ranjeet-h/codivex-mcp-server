#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
OUT_DIR="${ROOT_DIR}/benchmarks/matrix"
mkdir -p "${OUT_DIR}"
rm -f "${OUT_DIR}"/*.json

DEFAULT_MATRIX="small:${ROOT_DIR}:120"
MATRIX="${BENCHMARK_MATRIX:-${DEFAULT_MATRIX}}"
QUERY="${BENCHMARK_QUERY:-iso to date}"

IFS=';' read -ra entries <<< "${MATRIX}"
for entry in "${entries[@]}"; do
  IFS=':' read -r profile path max_files <<< "${entry}"
  if [[ -z "${profile}" || -z "${path}" || -z "${max_files}" ]]; then
    echo "skipping invalid matrix entry: ${entry}"
    continue
  fi
  if [[ ! -d "${path}" ]]; then
    echo "skipping missing dataset path: ${path}"
    continue
  fi

  echo "[bench-matrix] running profile=${profile} path=${path} max_files=${max_files}"
  BENCHMARK_DATASET_PROFILE="${profile}" \
  BENCHMARK_DATASET_PATH="${path}" \
  BENCHMARK_REDACT_PATH="${BENCHMARK_REDACT_PATH:-true}" \
  BENCHMARK_MAX_FILES="${max_files}" \
  BENCHMARK_QUERY="${QUERY}" \
  cargo run -p mcp-server --bin benchmark_suite >/tmp/codivex-bench-${profile}.json

  report_file="${OUT_DIR}/${profile}.json"
  cp benchmarks/latest-report.json "${report_file}"
done

python3 - <<'PY'
import glob, json, os
root = os.path.abspath("benchmarks/matrix")
reports = []
for path in sorted(glob.glob(os.path.join(root, "*.json"))):
    if path.endswith("summary.json"):
        continue
    with open(path) as f:
        reports.append(json.load(f))
with open(os.path.join(root, "summary.json"), "w") as f:
    json.dump(reports, f, indent=2)
print(json.dumps(reports, indent=2))
PY
