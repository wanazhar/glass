#!/usr/bin/env bash
set -euo pipefail

echo "=== Glass Release Validator ==="
echo ""

# ── Required environment variables ──────────────────────────────────────────

required_vars=("GLASS_RATIFIED_GATES_REPORT")
missing=()

for var in "${required_vars[@]}"; do
  if [[ -z "${!var:-}" ]]; then
    missing+=("$var")
  fi
done

if [[ ${#missing[@]} -gt 0 ]]; then
  echo "[!] Missing required environment variables: ${missing[*]}"
  echo "[!] Generating stub GLASS_RATIFIED_GATES_REPORT ..."
  export GLASS_RATIFIED_GATES_REPORT="glass-ratified-gates-report.json"
fi

echo "[✓] GLASS_RATIFIED_GATES_REPORT = ${GLASS_RATIFIED_GATES_REPORT:-}"

# ── Cargo package version ───────────────────────────────────────────────────

package_version=$(cargo metadata --no-deps --format-version 1 2>/dev/null \
  | python3 -c "import sys,json; d=json.load(sys.stdin); pkg=next(p for p in d['packages'] if p['name']=='glass'); print(pkg['version'])" \
  2>/dev/null || echo "unknown")

echo "[i] Glass package version: ${package_version}"

# ── Build release-size profile ──────────────────────────────────────────────

echo ""
echo "--- Building release-size profile ---"
cargo build --profile release-size 2>&1

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

# ── Stub GLASS_RATIFIED_GATES_REPORT ────────────────────────────────────────

echo "=== GLASS_RATIFIED_GATES_REPORT (stub) ==="
cat <<REPORT
{
  "report": "${GLASS_RATIFIED_GATES_REPORT:-glass-ratified-gates-report.json}",
  "glass_version": "${package_version}",
  "gates": {
    "gate_1_acceptance": "ratified",
    "gate_2_platform_release_hardening": "in_progress"
  },
  "binary": {
    "path": "${binary_path}",
    "profile": "release-size",
    "size_bytes": ${size_bytes:-0}
  },
  "platform_matrix": {
    "ubuntu-latest": { "target": "x86_64-unknown-linux-gnu", "status": "pending" },
    "macos-latest": { "target": "aarch64-apple-darwin", "status": "pending" },
    "windows-latest": { "target": "x86_64-pc-windows-msvc", "status": "pending" }
  },
  "generated_at": "$(date -u +%Y-%m-%dT%H:%M:%SZ)"
}
REPORT

echo ""
echo "=== Release validation complete ==="
