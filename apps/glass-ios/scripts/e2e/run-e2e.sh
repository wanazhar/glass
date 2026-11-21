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

python3 "$here/harness.py" >"$root/harness.log" 2>&1 &
harness=$!
trap 'kill $harness 2>/dev/null; wait $harness 2>/dev/null || true' EXIT
for _ in $(seq 60); do grep -q '^ready' "$root/harness.log" && break; kill -0 $harness || { cat "$root/harness.log"; exit 1; }; sleep 0.5; done
grep '^ready' "$root/harness.log"
curl -sf --cacert "$root/tls/ca.pem" "https://$host/api/status" -o /dev/null -w 'tailnet front end: HTTP %{http_code}\n' || true

cd "$app"
rm -rf build/results/e2e.xcresult
TEST_RUNNER_GLASS_E2E_CONTROL="http://127.0.0.1:${GLASS_E2E_CONTROL_PORT:-51999}" \
  xcodebuild test -project Glass.xcodeproj -scheme Glass -destination "platform=iOS Simulator,id=$udid" \
  -derivedDataPath build/test -resultBundlePath build/results/e2e.xcresult \
  -only-testing:GlassUITests/LiveHostE2ETests -quiet
