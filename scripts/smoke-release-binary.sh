#!/usr/bin/env bash
set -euo pipefail

binary="${1:-target/release/glass}"
if [[ ! -x "$binary" ]]; then
    echo "release binary is not executable: $binary" >&2
    exit 1
fi

"$binary" --help >/dev/null

python3 - "$binary" <<'PY'
import json
import subprocess
import sys

binary = sys.argv[1]
manifest = json.loads(subprocess.check_output([binary, "capabilities"], text=True))
manifest.pop("contextCost", None)
if manifest.get("protocolVersion") != 1:
    raise SystemExit("release binary reported an unsupported Glass protocol")
if manifest.get("capabilities", {}).get("action") is not True:
    raise SystemExit("release binary did not advertise the action capability")
for schema in ("action", "observation", "workflow", "checkpoint"):
    if 1 not in manifest.get("schemas", {}).get(schema, []):
        raise SystemExit(f"release binary did not advertise schema {schema}@1")

child = subprocess.Popen(
    [binary, "--mcp"],
    stdin=subprocess.PIPE,
    stdout=subprocess.PIPE,
    stderr=subprocess.DEVNULL,
    text=True,
)
try:
    requests = [
        {
            "jsonrpc": "2.0",
            "id": 1,
            "method": "initialize",
            "params": {
                "protocolVersion": "2024-11-05",
                "glass": {
                    "protocolVersion": 1,
                    "schemas": {
                        "action": [1],
                        "observation": [1],
                        "workflow": [1],
                        "checkpoint": [1],
                    },
                },
            },
        },
        {"jsonrpc": "2.0", "method": "notifications/initialized", "params": {}},
        {"jsonrpc": "2.0", "id": 2, "method": "tools/list", "params": {}},
    ]
    child.stdin.write("\n".join(json.dumps(request) for request in requests) + "\n")
    child.stdin.close()
    initialize = json.loads(child.stdout.readline())
    if initialize.get("result", {}).get("glass") != manifest:
        raise SystemExit("MCP and CLI capability manifests differ")
    tools = json.loads(child.stdout.readline()).get("result", {}).get("tools", [])
    names = {tool.get("name") for tool in tools}
    if "observe" not in names or "workflow" not in names or len(names) != len(tools):
        raise SystemExit("release binary returned an invalid MCP tool inventory")
finally:
    child.terminate()
    child.wait(timeout=5)

print(f"release binary smoke passed: {binary} ({len(tools)} MCP tools)")
PY

if [[ "${GLASS_RELEASE_BROWSER_SMOKE:-0}" != "1" ]]; then
    exit 0
fi

smoke_root="$(mktemp -d "${TMPDIR:-/tmp}/glass-release-browser-smoke.XXXXXX")"
server_pid=""
cleanup() {
    if [[ -n "$server_pid" ]]; then
        kill "$server_pid" 2>/dev/null || true
    fi
    rm -rf "$smoke_root"
}
trap cleanup EXIT

python3 -m http.server 18765 --directory crates/glass-browser/tests/fixtures >"$smoke_root/server.log" 2>&1 &
server_pid=$!
sleep 1

chrome_args=()
if [[ -n "${CHROME_PATH:-}" ]]; then
    chrome_args+=(--chrome-path "$CHROME_PATH")
fi

"$binary" "${chrome_args[@]}" --incognito --port 9222 navigate \
    http://127.0.0.1:18765/basic.html >"$smoke_root/navigate.json"
grep -F 'basic.html' "$smoke_root/navigate.json" >/dev/null

"$binary" "${chrome_args[@]}" --incognito --port 9223 observe >"$smoke_root/observe.json"
grep -F 'accessibility' "$smoke_root/observe.json" >/dev/null

"$binary" "${chrome_args[@]}" --incognito --port 9224 screenshot \
    --output "$smoke_root/screenshot.png" >/dev/null
test -s "$smoke_root/screenshot.png"

echo "release browser smoke passed: navigate, observe, screenshot"
