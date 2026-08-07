#!/usr/bin/env bash
set -euo pipefail

echo "=== Glass Release Validator ==="
echo ""

# ── Cargo package version ───────────────────────────────────────────────────

package_version=$(cargo metadata --no-deps --locked --format-version 1 2>/dev/null \
  | python3 -c "import sys,json; d=json.load(sys.stdin); pkg=next(p for p in d['packages'] if p['name']=='glass-browser'); print(pkg['version'])" \
  2>/dev/null || echo "unknown")

echo "[i] Glass package version: ${package_version}"

echo ""
echo "--- Validating Web IR corpus and deterministic baseline ---"
python3 scripts/check-web-ir-corpus.py --baseline benchmarks/results/web-ir-v1.json

# ── Build release-size profile ──────────────────────────────────────────────

echo ""
echo "--- Building release-size profile ---"
cargo build --package glass-dev --profile release-size 2>&1

# ── Binary size check ───────────────────────────────────────────────────────

binary_path="target/release-size/glass"
if [[ -f "$binary_path" ]]; then
  if command -v ls &>/dev/null; then
    size_bytes=$(stat -c%s "$binary_path" 2>/dev/null || stat -f%z "$binary_path" 2>/dev/null)
    size_mb=$(echo "scale=2; $size_bytes / 1048576" | bc 2>/dev/null || echo "N/A")
    echo ""
    echo "=== Binary Size Report ==="
    echo "  Binary:   $binary_path"
    echo "  Size:     ${size_bytes} bytes (${size_mb} MB)"
  else
    echo "[i] Binary found but cannot stat size: $binary_path"
  fi
else
  echo "[!] Binary not found at expected path: $binary_path"
  echo "[i] Searching for glass binary ..."
  find target/release-size -name 'glass' -type f 2>/dev/null || echo "  (none found)"
fi

# ── Platform matrix status ──────────────────────────────────────────────────

echo ""
echo "=== Platform Matrix Status ==="
echo "  Host OS:       $(uname -s)"
echo "  Host arch:     $(uname -m)"
echo "  Rust target:   $(rustc -vV 2>/dev/null | grep 'host:' | awk '{print $2}' || echo 'unknown')"
echo "  Rust version:  $(rustc --version 2>/dev/null || echo 'not installed')"
echo ""

echo "--- Generating revision-bound evidence ---"
scripts/generate-validation-evidence.sh "${GLASS_VALIDATION_OUTPUT_DIR:-benchmarks/results/validation/$(git rev-parse --short HEAD)}"

echo ""
echo "=== Release validation complete ==="
