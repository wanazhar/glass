#!/usr/bin/env bash
# Live-host E2E: real glass-core gateway (disposable workspace) behind a simulated
# Tailscale Serve TLS front end, driven by the GlassUITests LiveHostE2ETests on a Simulator.
# Needs sudo once to map GLASS_E2E_HOST to 127.0.0.1 in /etc/hosts.
# Usage: scripts/e2e/run-e2e.sh [simulator-udid]
set -euo pipefail
here=$(cd "$(dirname "$0")" && pwd)
app=$(cd "$here/../.." && pwd)
repo=$(cd "$app/../.." && pwd)
host=${GLASS_E2E_HOST:-glass-e2e.tail0e2e.ts.net}
root=${GLASS_E2E_ROOT:-$HOME/.glass-e2e}
udid=${1:-$(xcrun simctl list devices available | awk '/^-- iOS 26/{f=1;next} /^--/{f=0} f && /iPhone 17 \(/{print; exit}' | sed -E 's/.*\(([0-9A-F-]{36})\).*/\1/')}

pkill -f "$repo/target/debug/glass-core serve $root/" 2>/dev/null || true
rm -rf "$root" && mkdir -p "$root/tls"
(cd "$repo" && cargo build -p glass-core --locked -q)
(cd "$repo/products/glass-core/web" && { [[ -d node_modules ]] || npm ci --silent; } && npm run --silent build >/dev/null)
export GLASS_CORE_BIN="$repo/target/debug/glass-core" GC_WEB_ASSETS="$repo/products/glass-core/web/dist"
export GLASS_E2E_ROOT="$root" GLASS_E2E_HOST="$host"

# Throwaway CA + leaf for the simulated tailnet name; trusted only inside the Simulator.
openssl req -x509 -newkey rsa:2048 -nodes -days 2 -subj "/CN=Glass E2E CA" \
  -addext "basicConstraints=critical,CA:TRUE" -addext "keyUsage=critical,keyCertSign" \
  -keyout "$root/tls/ca.key" -out "$root/tls/ca.pem" 2>/dev/null
openssl req -newkey rsa:2048 -nodes -subj "/CN=$host" -keyout "$root/tls/host.key" -out "$root/tls/host.csr" 2>/dev/null
printf 'subjectAltName=DNS:%s\nextendedKeyUsage=serverAuth\nbasicConstraints=CA:FALSE\n' "$host" >"$root/tls/ext"
openssl x509 -req -in "$root/tls/host.csr" -CA "$root/tls/ca.pem" -CAkey "$root/tls/ca.key" -CAcreateserial \
  -days 2 -extfile "$root/tls/ext" -out "$root/tls/host.pem" 2>/dev/null

grep -q " $host\$" /etc/hosts || echo "127.0.0.1 $host" | sudo tee -a /etc/hosts >/dev/null
xcrun simctl boot "$udid" 2>/dev/null || true
xcrun simctl bootstatus "$udid" -b >/dev/null
xcrun simctl keychain "$udid" add-root-cert "$root/tls/ca.pem"
# A known photo for the attachment test.
python3 - "$root/e2e-photo.png" <<'PY'
import struct, sys, zlib
w = h = 64
rows = b"".join(b"\x00" + b"".join(bytes((x * 4, 120, 255 - y * 4)) for x in range(w)) for y in range(h))
chunk = lambda t, d: struct.pack(">I", len(d)) + t + d + struct.pack(">I", zlib.crc32(t + d))
open(sys.argv[1], "wb").write(b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", struct.pack(">IIBBBBB", w, h, 8, 2, 0, 0, 0)) + chunk(b"IDAT", zlib.compress(rows)) + chunk(b"IEND", b""))
PY
xcrun simctl addmedia "$udid" "$root/e2e-photo.png"
xcrun simctl privacy "$udid" grant photos com.wanazhar.glass 2>/dev/null || true

python3 "$here/harness.py" >"$root/harness.log" 2>&1 &
harness=$!
# Gateway and per-thread serve processes outlive the harness; stop them with it.
stop_host() {
  kill $harness 2>/dev/null; wait $harness 2>/dev/null || true
  pkill -f "$GLASS_CORE_BIN connect _serve" 2>/dev/null || true
  pkill -f "$GLASS_CORE_BIN serve $root/" 2>/dev/null || true
}
trap stop_host EXIT
for _ in $(seq 60); do grep -q '^ready' "$root/harness.log" && break; kill -0 $harness || { cat "$root/harness.log"; exit 1; }; sleep 0.5; done
grep '^ready' "$root/harness.log"
curl -sf --cacert "$root/tls/ca.pem" "https://$host/api/status" -o /dev/null -w 'tailnet front end: HTTP %{http_code}\n' || true

# GLASS_E2E_ONLY=testName runs one test. GLASS_E2E_HARNESS_ONLY=1 keeps the gateway up for manual probing without running tests.
[[ -n ${GLASS_E2E_HARNESS_ONLY:-} ]] && { wait $harness; exit; }

cd "$app"
rm -rf build/results/e2e.xcresult
# GLASS_E2E_ONLY may list several tests separated by commas.
only=()
if [[ -n ${GLASS_E2E_ONLY:-} ]]; then
  IFS=, read -ra names <<<"$GLASS_E2E_ONLY"
  for name in "${names[@]}"; do only+=("-only-testing:GlassUITests/LiveHostE2ETests/$name"); done
else
  only=("-only-testing:GlassUITests/LiveHostE2ETests")
fi
TEST_RUNNER_GLASS_E2E_CONTROL="http://127.0.0.1:${GLASS_E2E_CONTROL_PORT:-51999}" \
TEST_RUNNER_GLASS_E2E_MEASURE_ONLY="${GLASS_E2E_MEASURE_ONLY:-}" \
  xcodebuild test -project Glass.xcodeproj -scheme Glass -collect-test-diagnostics never -destination "platform=iOS Simulator,id=$udid" \
  -derivedDataPath build/test -resultBundlePath build/results/e2e.xcresult \
  "${only[@]}" -quiet
