#!/usr/bin/env bash
set -euo pipefail

# Common source gates shared by local preflight and the CI test job. Platform
# fixtures, client smoke tests, dependency audits, and fuzz execution remain in
# their owning CI jobs; the release workflow consumes the resulting exact-SHA
# certification instead of repeating these source gates.

report_path="${GLASS_RELEASE_DOC_REPORT:-${TMPDIR:-/tmp}/glass-release-documentation.json}"
mkdir -p "$(dirname "$report_path")"

run_doc_check() {
  local -a args=(--report "$report_path" --require-previous-version)
  if [[ -n "${GLASS_PREVIOUS_VERSION:-}" ]]; then
    args+=(--previous-version "$GLASS_PREVIOUS_VERSION")
  fi
  python3 scripts/check-release-documentation.py "${args[@]}"
}

cargo fmt --all -- --check
python3 scripts/check-version-sync.py
python3 scripts/check-feature-parity.py
run_doc_check
python3 scripts/check-tui-shortcuts.py
python3 scripts/check-documentation-depth.py
scripts/check-rust-workspace.sh clippy
scripts/check-rust-workspace.sh test
python3 scripts/check-documentation-coverage.py
python3 scripts/check-reliability-matrix.py
python3 scripts/check-public-readonly-adapters.py
python3 scripts/check-web-ir-corpus.py --baseline benchmarks/results/web-ir-v1.json
RUSTDOCFLAGS="-D warnings" cargo doc --all-features --locked --no-deps

cargo package --package glass-browser --locked --allow-dirty --no-verify
cargo package --package glass-dev --locked --allow-dirty --no-verify \
  --config 'patch.crates-io.glass-browser.path="crates/glass-browser"'
version="$(cargo metadata --no-deps --locked --format-version 1 \
  | python3 -c 'import json,sys; print(next(p["version"] for p in json.load(sys.stdin)["packages"] if p["name"] == "glass-dev"))')"
python3 scripts/check-packaged-dependency.py \
  "target/package/glass-dev-${version}.crate" --version "$version"

cargo fetch --manifest-path fuzz/Cargo.toml --locked
cargo check --manifest-path fuzz/Cargo.toml --locked --offline --all-targets
