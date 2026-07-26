#!/usr/bin/env bash
set -euo pipefail

# Run the published compare-018 contract. Missing browser, evidence, or
# adapter prerequisites fail closed; this script never fabricates a platform
# or best-in-class report.

root_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$root_dir"

chrome_path="${CHROME_PATH:-}"
if [[ -z "$chrome_path" ]]; then
  for candidate in google-chrome chromium chromium-browser chrome; do
    if command -v "$candidate" >/dev/null 2>&1; then
      chrome_path="$(command -v "$candidate")"
      break
    fi
  done
fi
if [[ -z "$chrome_path" || ! -f "$chrome_path" ]]; then
  echo "compare-018 requires CHROME_PATH or an installed Chrome/Chromium binary" >&2
  exit 2
fi
command -v node >/dev/null 2>&1 || { echo "compare-018 requires node" >&2; exit 2; }
command -v npm >/dev/null 2>&1 || { echo "compare-018 requires npm" >&2; exit 2; }

validation_dir="${GLASS_VALIDATION_OUTPUT_DIR:-benchmarks/results/validation/$(git rev-parse --short HEAD)}"
if [[ -z "${GLASS_RATIFIED_GATES_REPORT:-}" || -z "${GLASS_RELEASE_VALIDATION_REPORT:-}" || -z "${GLASS_PLATFORM_MATRIX_REPORT:-}" ]]; then
  scripts/generate-validation-evidence.sh "$validation_dir"
  export GLASS_RATIFIED_GATES_REPORT="$validation_dir/ratified-gates.json"
  export GLASS_RELEASE_VALIDATION_REPORT="$validation_dir/release-validation.json"
  export GLASS_PLATFORM_MATRIX_REPORT="$validation_dir/platform-matrix.json"
fi

export CHROME_PATH="$chrome_path"
exec node benchmarks/run-acceptance.mjs
