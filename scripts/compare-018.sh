#!/usr/bin/env bash
set -euo pipefail

echo "=== Glass Acceptance Gate: compare-018 ==="
echo ""

# ── Prerequisites check ─────────────────────────────────────────────────────

failures=0

check_cmd() {
  local cmd="$1"
  local label="${2:-$cmd}"
  if command -v "$cmd" &>/dev/null; then
    local ver
    ver=$("$cmd" --version 2>&1 | head -1 || echo "version unavailable")
    echo "  [✓] ${label}: ${ver}"
    return 0
  else
    echo "  [✗] ${label}: not found in PATH"
    return 1
  fi
}

echo "--- Checking prerequisites ---"
check_cmd chrome "Chrome/Chromium" || ((failures++))
check_cmd google-chrome "Google Chrome" || true
check_cmd chromium "Chromium" || true
check_cmd node "Node.js" || ((failures++))
check_cmd npm "npm" || true
echo ""

if [[ $failures -gt 0 ]]; then
  echo "[!] ${failures} prerequisite(s) missing."
  echo "[i] Install missing tools and re-run this script."
else
  echo "[✓] All critical prerequisites met."
fi

# ── Acceptance gate instructions ────────────────────────────────────────────

echo ""
echo "=== Acceptance Gate Instructions ==="
echo ""
echo "  Gate 018: compare-018 — Playwright adapter parity checkpoint"
echo ""
echo "  This gate validates that the Glass CDP implementation is competitive"
echo "  with Playwright on core browser-automation metrics."
echo ""
echo "  Steps:"
echo "    1. Ensure a Chrome/Chromium instance is available on port 9222."
echo "    2. Run the Glass benchmark suite:"
echo "         GLASS_BENCH_ITERATIONS=50 cargo run --release --example benchmark"
echo "    3. Run the equivalent Playwright benchmark (Node.js):"
echo "         node benchmarks/playwright-baseline.js"
echo "    4. Compare results against thresholds in benchmarks/README.md"
echo "    5. Record findings in the best_in_class_eligible report below."
echo ""

# ── Stub GLASS_PLATFORM_MATRIX_REPORT ───────────────────────────────────────

echo "=== GLASS_PLATFORM_MATRIX_REPORT (stub) ==="
cat <<REPORT
{
  "report": "glass-platform-matrix-report.json",
  "gate": "compare-018",
  "glass_version": "$(cargo metadata --no-deps --format-version 1 2>/dev/null | python3 -c "import sys,json; d=json.load(sys.stdin); pkg=next(p for p in d['packages'] if p['name']=='glass'); print(pkg['version'])" 2>/dev/null || echo 'unknown')",
  "prerequisites": {
    "chrome": "$(command -v chrome || command -v google-chrome || command -v chromium || echo 'missing')",
    "node": "$(command -v node || echo 'missing')",
    "npm": "$(command -v npm || echo 'missing')"
  },
  "best_in_class_eligible": {
    "criterion": "Glass agent performs equal to or better than Playwright across core browser automation operations",
    "metrics": [
      "navigation_latency_ms",
      "dom_parse_latency_ms",
      "click_roundtrip_ms",
      "screenshot_capture_ms",
      "scroll_fidelity_score"
    ],
    "thresholds": {
      "max_regression_pct": 5,
      "min_improvement_pct": 0
    },
    "status": "pending_benchmark"
  },
  "platform_matrix": {
    "ubuntu-latest": { "chrome_available": false, "benchmark_passed": false },
    "macos-latest":  { "chrome_available": false, "benchmark_passed": false },
    "windows-latest": { "chrome_available": false, "benchmark_passed": false }
  },
  "generated_at": "$(date -u +%Y-%m-%dT%H:%M:%SZ)"
}
REPORT

echo ""
echo "=== compare-018 acceptance gate stub complete ==="
echo "[i] Re-run after installing Chrome and Node.js to populate real benchmark data."
